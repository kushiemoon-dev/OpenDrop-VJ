//! OpenDrop Tauri backend
//!
//! Multi-deck visualization controller supporting up to 4 simultaneous decks.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::{info, warn};

use opendrop_core::audio::{AudioConfig, AudioEngine, DeviceInfo};
use opendrop_core::midi::{
    list_midi_ports as core_list_midi_ports, create_apc_mini_preset, create_generic_dj_preset,
    create_launchpad_preset, create_nanokontrol2_preset, MidiAction, MidiController, MidiMapping,
    MidiMessageType, MidiPortInfo, MidiPreset,
};

/// Maximum number of decks supported
pub const MAX_DECKS: u8 = 4;

/// Deck identifier (0-3)
pub type DeckId = u8;

/// Renderer process health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererHealth {
    /// Renderer is starting up
    Starting,
    /// Renderer is running and healthy
    Running,
    /// Renderer reported ready
    Ready,
    /// Renderer crashed or exited unexpectedly
    Crashed,
    /// Renderer was stopped normally
    Stopped,
}

/// Renderer process handle
pub struct RendererProcess {
    child: Child,
    running: bool,
    health: Arc<Mutex<RendererHealth>>,
    started_at: std::time::Instant,
    crash_count: u32,
    stdout_reader: Option<JoinHandle<()>>,
}

/// Events received from renderer process
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum RendererEvent {
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "preset_loaded")]
    PresetLoaded { path: String },
}

impl RendererProcess {
    fn new(mut child: Child) -> Self {
        let health = Arc::new(Mutex::new(RendererHealth::Starting));
        let health_clone = Arc::clone(&health);

        // Spawn thread to read stdout events from renderer
        let stdout_reader = child.stdout.take().map(|stdout| {
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            if let Ok(event) = serde_json::from_str::<RendererEvent>(&line) {
                                match event {
                                    RendererEvent::Ready => {
                                        if let Ok(mut h) = health_clone.lock() {
                                            *h = RendererHealth::Ready;
                                        }
                                        info!("Renderer reported ready");
                                    }
                                    RendererEvent::Closed => {
                                        if let Ok(mut h) = health_clone.lock() {
                                            *h = RendererHealth::Stopped;
                                        }
                                        info!("Renderer closed");
                                        break;
                                    }
                                    RendererEvent::Error { message } => {
                                        warn!("Renderer error: {}", message);
                                    }
                                    RendererEvent::PresetLoaded { path } => {
                                        info!("Renderer loaded preset: {}", path);
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
        });

        Self {
            child,
            running: true,
            health,
            started_at: std::time::Instant::now(),
            crash_count: 0,
            stdout_reader,
        }
    }

    fn send_command(&mut self, cmd: &RendererCommand) -> Result<(), String> {
        if let Some(ref mut stdin) = self.child.stdin {
            let json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
            writeln!(stdin, "{}", json).map_err(|e| e.to_string())?;
            stdin.flush().map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("No stdin available".to_string())
        }
    }

    fn is_running(&mut self) -> bool {
        if !self.running {
            return false;
        }
        match self.child.try_wait() {
            Ok(Some(exit_status)) => {
                self.running = false;
                // Check if it was a crash or normal exit
                if let Ok(mut h) = self.health.lock() {
                    if exit_status.success() {
                        *h = RendererHealth::Stopped;
                    } else {
                        *h = RendererHealth::Crashed;
                        self.crash_count += 1;
                        warn!("Renderer crashed with status: {:?}", exit_status);
                    }
                }
                false
            }
            Ok(None) => {
                // Process still running
                if let Ok(mut h) = self.health.lock() {
                    if *h == RendererHealth::Starting {
                        // Check if it's been running long enough to consider healthy
                        // (fallback if Ready event not received)
                        if self.started_at.elapsed() > std::time::Duration::from_secs(2) {
                            *h = RendererHealth::Running;
                        }
                    }
                }
                true
            }
            Err(e) => {
                warn!("Error checking renderer status: {}", e);
                self.running = false;
                if let Ok(mut h) = self.health.lock() {
                    *h = RendererHealth::Crashed;
                }
                self.crash_count += 1;
                false
            }
        }
    }

    fn stop(&mut self) {
        if self.running {
            let _ = self.send_command(&RendererCommand::Stop);
            // Give it a moment to close gracefully
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.running = false;
            if let Ok(mut h) = self.health.lock() {
                *h = RendererHealth::Stopped;
            }
        }
        // Wait for stdout reader thread to finish
        if let Some(handle) = self.stdout_reader.take() {
            let _ = handle.join();
        }
    }

    fn get_health(&self) -> RendererHealth {
        self.health.lock().map(|h| *h).unwrap_or(RendererHealth::Crashed)
    }

    fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}

impl Drop for RendererProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Commands sent to the renderer process
#[derive(Serialize)]
#[serde(tag = "type")]
enum RendererCommand {
    #[serde(rename = "load_preset")]
    LoadPreset { path: String },
    #[serde(rename = "audio")]
    Audio { samples: Vec<f32> },
    #[serde(rename = "toggle_fullscreen")]
    ToggleFullscreen,
    #[serde(rename = "set_beat_sensitivity")]
    SetBeatSensitivity { value: f32 },
    #[serde(rename = "set_video_output")]
    SetVideoOutput {
        enabled: bool,
        device_path: Option<String>,
    },
    #[serde(rename = "set_ndi_output")]
    SetNdiOutput {
        enabled: bool,
        name: Option<String>,
    },
    #[serde(rename = "set_texture_paths")]
    SetTexturePaths { paths: Vec<String> },
    #[serde(rename = "stop")]
    Stop,
}

/// Config sent to renderer on startup
#[derive(Debug, Serialize)]
struct RendererConfig {
    width: u32,
    height: u32,
    preset_path: Option<String>,
    fullscreen: bool,
    deck_id: u8,
    /// Monitor index for fullscreen (0 = primary)
    #[serde(default)]
    monitor_index: Option<usize>,
    /// Texture search paths for presets that reference external textures
    #[serde(default)]
    texture_paths: Vec<String>,
}

/// A preset item in a playlist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub name: String,
    pub path: String,
}

/// Playlist for a deck
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub items: Vec<PlaylistItem>,
    pub current_index: usize,
    pub shuffle: bool,
    pub auto_cycle: bool,
    pub cycle_duration_secs: u32,
}

impl Playlist {
    pub fn new() -> Self {
        Self {
            name: "Untitled".to_string(),
            items: Vec::new(),
            current_index: 0,
            shuffle: false,
            auto_cycle: false,
            cycle_duration_secs: 30,
        }
    }

    pub fn current_preset(&self) -> Option<&PlaylistItem> {
        self.items.get(self.current_index)
    }

    pub fn advance(&mut self) -> Option<&PlaylistItem> {
        if self.items.is_empty() {
            return None;
        }
        if self.shuffle {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            use std::time::{SystemTime, UNIX_EPOCH};

            let mut hasher = DefaultHasher::new();
            // Use unwrap_or with fallback to avoid panic on clock issues
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
                .as_nanos();
            nanos.hash(&mut hasher);
            self.current_index = (hasher.finish() as usize) % self.items.len();
        } else {
            self.current_index = (self.current_index + 1) % self.items.len();
        }
        self.items.get(self.current_index)
    }

    pub fn previous(&mut self) -> Option<&PlaylistItem> {
        if self.items.is_empty() {
            return None;
        }
        if self.current_index == 0 {
            self.current_index = self.items.len() - 1;
        } else {
            self.current_index -= 1;
        }
        self.items.get(self.current_index)
    }
}

/// State for a single deck
pub struct DeckState {
    pub id: DeckId,
    pub renderer: Option<RendererProcess>,
    pub preset_path: Option<String>,
    pub volume: f32,
    pub beat_sensitivity: f32,
    pub active: bool,
    pub playlist: Playlist,
    pub last_cycle_time: Option<std::time::Instant>,
}

impl DeckState {
    pub fn new(id: DeckId) -> Self {
        Self {
            id,
            renderer: None,
            preset_path: None,
            volume: 1.0,
            beat_sensitivity: 1.0,
            active: false,
            playlist: Playlist::new(),
            last_cycle_time: None,
        }
    }

    pub fn is_running(&mut self) -> bool {
        self.renderer.as_mut().is_some_and(|r| r.is_running())
    }
}

/// Crossfader curve types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum CrossfaderCurve {
    /// Linear crossfade (simple volume blend)
    Linear,
    /// Equal power crossfade (maintains perceived loudness)
    #[default]
    EqualPower,
}

/// Crossfader configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossfaderConfig {
    /// Crossfader position (0.0 = Side A, 0.5 = center, 1.0 = Side B)
    pub position: f32,
    /// Which decks are assigned to Side A
    pub side_a: Vec<DeckId>,
    /// Which decks are assigned to Side B
    pub side_b: Vec<DeckId>,
    /// Crossfade curve type
    pub curve: CrossfaderCurve,
    /// Whether crossfader is enabled
    pub enabled: bool,
}

impl Default for CrossfaderConfig {
    fn default() -> Self {
        Self {
            position: 0.5, // Center by default
            side_a: vec![0, 1], // Decks 1 & 2 on Side A
            side_b: vec![2, 3], // Decks 3 & 4 on Side B
            curve: CrossfaderCurve::EqualPower,
            enabled: false, // Disabled by default
        }
    }
}

impl CrossfaderConfig {
    /// Calculate volume multiplier for a deck based on crossfader position
    pub fn volume_for_deck(&self, deck_id: DeckId) -> f32 {
        if !self.enabled {
            return 1.0; // No crossfade effect when disabled
        }

        let is_side_a = self.side_a.contains(&deck_id);
        let is_side_b = self.side_b.contains(&deck_id);

        if !is_side_a && !is_side_b {
            return 1.0; // Deck not assigned to either side
        }

        let (a_vol, b_vol) = match self.curve {
            CrossfaderCurve::Linear => {
                let a = 1.0 - self.position;
                let b = self.position;
                (a, b)
            }
            CrossfaderCurve::EqualPower => {
                // Equal power crossfade: sqrt(1-x) and sqrt(x)
                // This maintains constant perceived loudness
                let a = (1.0 - self.position).sqrt();
                let b = self.position.sqrt();
                (a, b)
            }
        };

        if is_side_a {
            a_vol
        } else {
            b_vol
        }
    }
}

/// Blend mode for compositor
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum BlendMode {
    #[default]
    Normal,     // Standard alpha blend
    Add,        // Additive blend (bright)
    Multiply,   // Darken blend
    Screen,     // Lighten blend
    Overlay,    // Contrast blend
}

/// Per-deck compositor settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckCompositorSettings {
    pub opacity: f32,           // 0.0 to 1.0
    pub blend_mode: BlendMode,
    pub layer_order: i32,       // Higher = on top
    pub enabled: bool,          // Include in composite
}

impl Default for DeckCompositorSettings {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            layer_order: 0,
            enabled: true,
        }
    }
}

/// Compositor configuration for combining deck outputs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositorConfig {
    /// Whether compositor output is enabled
    pub enabled: bool,
    /// Output resolution width
    pub output_width: u32,
    /// Output resolution height
    pub output_height: u32,
    /// Per-deck compositor settings
    pub deck_settings: HashMap<DeckId, DeckCompositorSettings>,
    /// Link deck opacity to crossfader position
    pub link_to_crossfader: bool,
}

impl Default for CompositorConfig {
    fn default() -> Self {
        let mut deck_settings = HashMap::new();
        for id in 0..MAX_DECKS {
            let settings = DeckCompositorSettings {
                layer_order: id as i32, // Default layer order matches deck ID
                ..Default::default()
            };
            deck_settings.insert(id, settings);
        }

        Self {
            enabled: false,
            output_width: 1920,
            output_height: 1080,
            deck_settings,
            link_to_crossfader: true,
        }
    }
}

/// Application state shared across Tauri commands
pub struct AppState {
    decks: Mutex<HashMap<DeckId, DeckState>>,
    audio_engine: Mutex<AudioEngine>,
    crossfader: Mutex<CrossfaderConfig>,
    compositor: Mutex<CompositorConfig>,
    midi_controller: Mutex<MidiController>,
    /// Current audio levels (left, right) for VU meters - updated by pump_audio
    audio_levels: Mutex<(f32, f32)>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let mut decks = HashMap::new();
        // Initialize all 4 decks
        for id in 0..MAX_DECKS {
            decks.insert(id, DeckState::new(id));
        }

        Self {
            decks: Mutex::new(decks),
            audio_engine: Mutex::new(AudioEngine::new()),
            crossfader: Mutex::new(CrossfaderConfig::default()),
            compositor: Mutex::new(CompositorConfig::default()),
            midi_controller: Mutex::new(MidiController::new()),
            audio_levels: Mutex::new((0.0, 0.0)),
        }
    }
}

// AppState is Send + Sync because:
// - All fields are wrapped in Mutex<T>
// - Mutex<T> is Send + Sync when T: Send
// - AudioEngine, DeckState, CrossfaderConfig, CompositorConfig are all Send

/// Response types for frontend
#[derive(Serialize, Deserialize)]
pub struct VisualizerStatus {
    pub running: bool,
    pub audio_running: bool,
    pub current_preset: Option<String>,
    pub preset_dir: String,
}

/// Crossfader info for frontend
#[derive(Serialize, Deserialize, Clone)]
pub struct CrossfaderInfo {
    pub position: f32,
    pub side_a: Vec<u8>,
    pub side_b: Vec<u8>,
    pub curve: String,
    pub enabled: bool,
}

impl From<&CrossfaderConfig> for CrossfaderInfo {
    fn from(c: &CrossfaderConfig) -> Self {
        Self {
            position: c.position,
            side_a: c.side_a.clone(),
            side_b: c.side_b.clone(),
            curve: match c.curve {
                CrossfaderCurve::Linear => "linear".to_string(),
                CrossfaderCurve::EqualPower => "equal_power".to_string(),
            },
            enabled: c.enabled,
        }
    }
}

/// Per-deck compositor info for frontend
#[derive(Serialize, Deserialize, Clone)]
pub struct DeckCompositorInfo {
    pub opacity: f32,
    pub blend_mode: String,
    pub layer_order: i32,
    pub enabled: bool,
}

impl From<&DeckCompositorSettings> for DeckCompositorInfo {
    fn from(s: &DeckCompositorSettings) -> Self {
        Self {
            opacity: s.opacity,
            blend_mode: match s.blend_mode {
                BlendMode::Normal => "normal",
                BlendMode::Add => "add",
                BlendMode::Multiply => "multiply",
                BlendMode::Screen => "screen",
                BlendMode::Overlay => "overlay",
            }.to_string(),
            layer_order: s.layer_order,
            enabled: s.enabled,
        }
    }
}

/// Compositor info for frontend
#[derive(Serialize, Deserialize, Clone)]
pub struct CompositorInfo {
    pub enabled: bool,
    pub output_width: u32,
    pub output_height: u32,
    pub link_to_crossfader: bool,
    pub deck_settings: HashMap<u8, DeckCompositorInfo>,
}

impl From<&CompositorConfig> for CompositorInfo {
    fn from(c: &CompositorConfig) -> Self {
        Self {
            enabled: c.enabled,
            output_width: c.output_width,
            output_height: c.output_height,
            link_to_crossfader: c.link_to_crossfader,
            deck_settings: c.deck_settings.iter()
                .map(|(k, v)| (*k, DeckCompositorInfo::from(v)))
                .collect(),
        }
    }
}

/// Extended status for multi-deck
#[derive(Serialize, Deserialize)]
pub struct MultiDeckStatus {
    pub decks: Vec<DeckInfo>,
    pub audio_running: bool,
    pub preset_dir: String,
    pub crossfader: CrossfaderInfo,
    pub compositor: CompositorInfo,
}

/// Playlist info for frontend
#[derive(Serialize, Deserialize, Clone)]
pub struct PlaylistInfo {
    pub name: String,
    pub items: Vec<PlaylistItem>,
    pub current_index: usize,
    pub shuffle: bool,
    pub auto_cycle: bool,
    pub cycle_duration_secs: u32,
}

impl From<&Playlist> for PlaylistInfo {
    fn from(p: &Playlist) -> Self {
        Self {
            name: p.name.clone(),
            items: p.items.clone(),
            current_index: p.current_index,
            shuffle: p.shuffle,
            auto_cycle: p.auto_cycle,
            cycle_duration_secs: p.cycle_duration_secs,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeckInfo {
    pub id: u8,
    pub running: bool,
    pub preset: Option<String>,
    pub volume: f32,
    pub beat_sensitivity: f32,
    pub playlist: PlaylistInfo,
    pub health: Option<RendererHealth>,
    pub uptime_secs: Option<u64>,
    pub crash_count: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub description: String,
    pub is_default: bool,
    pub is_monitor: bool,
    pub device_type: String, // "input", "output", or "monitor"
}

impl From<DeviceInfo> for AudioDeviceInfo {
    fn from(d: DeviceInfo) -> Self {
        let device_type = match d.device_type {
            opendrop_core::audio::DeviceType::Input => "input",
            opendrop_core::audio::DeviceType::Output => "output",
            opendrop_core::audio::DeviceType::Monitor => "monitor",
        };
        Self {
            name: d.name,
            description: d.description,
            is_default: d.is_default,
            is_monitor: d.is_monitor,
            device_type: device_type.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PresetInfo {
    pub name: String,
    pub path: String,
}

// ============ Tauri Commands ============

/// Start visualization on a specific deck
#[tauri::command]
fn start_deck(
    state: State<'_, AppState>,
    deck_id: Option<u8>,
    width: Option<u32>,
    height: Option<u32>,
    fullscreen: Option<bool>,
    preset_path: Option<String>,
    monitor_index: Option<usize>,
) -> Result<String, String> {
    let deck_id = deck_id.unwrap_or(0);
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}. Must be 0-{}", deck_id, MAX_DECKS - 1));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    // Check if already running
    if deck.is_running() {
        return Err(format!("Deck {} already running", deck_id));
    }

    // Find renderer executable
    let renderer_path = find_renderer_executable()?;
    info!("Using renderer at: {}", renderer_path);

    // Use a default preset if none specified - search all preset directories
    let preset = preset_path.or_else(|| {
        // Try to find any .milk preset in any of the default directories
        for dir in get_default_preset_dirs() {
            if !dir.exists() || !dir.is_dir() {
                continue;
            }

            // Recursively search for a preset
            fn find_first_preset(dir: &std::path::Path, depth: usize) -> Option<String> {
                if depth > 3 {
                    return None;
                }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_dir() {
                            if let Some(preset) = find_first_preset(&path, depth + 1) {
                                return Some(preset);
                            }
                        } else if path.extension().is_some_and(|ext| ext == "milk" || ext == "prjm") {
                            return Some(path.to_string_lossy().to_string());
                        }
                    }
                }
                None
            }

            if let Some(preset) = find_first_preset(&dir, 0) {
                return Some(preset);
            }
        }
        None
    });

    // Collect texture paths from default locations
    let texture_paths: Vec<String> = get_default_texture_dirs()
        .into_iter()
        .filter(|p| p.exists() && p.is_dir())
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Build config
    let config = RendererConfig {
        width: width.unwrap_or(1280),
        height: height.unwrap_or(720),
        preset_path: preset.clone(),
        fullscreen: fullscreen.unwrap_or(false),
        deck_id,
        monitor_index,
        texture_paths,
    };

    let config_json = serde_json::to_string(&config).map_err(|e| e.to_string())?;

    info!("Starting deck {} with config: {:?}", deck_id, config);

    // Spawn renderer process
    let child = Command::new(&renderer_path)
        .arg(&config_json)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to start renderer for deck {}: {}", deck_id, e))?;

    // Update deck state
    deck.preset_path = preset;
    deck.renderer = Some(RendererProcess::new(child));
    deck.active = true;

    Ok(format!("Deck {} started", deck_id))
}

/// Stop visualization on a specific deck
#[tauri::command]
fn stop_deck(state: State<'_, AppState>, deck_id: Option<u8>) -> Result<String, String> {
    let deck_id = deck_id.unwrap_or(0);
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(ref mut renderer) = deck.renderer {
        renderer.stop();
    }
    deck.renderer = None;
    deck.active = false;

    Ok(format!("Deck {} stopped", deck_id))
}

/// Load a preset on a specific deck
#[tauri::command]
fn load_preset(
    state: State<'_, AppState>,
    path: String,
    deck_id: Option<u8>,
) -> Result<String, String> {
    let deck_id = deck_id.unwrap_or(0);
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    // Validate preset path
    let preset_path = std::path::Path::new(&path);
    if !preset_path.exists() {
        return Err(format!("Preset file not found: {}", path));
    }
    if !preset_path.is_file() {
        return Err(format!("Preset path is not a file: {}", path));
    }
    // Check extension
    let valid_extensions = ["milk", "prjm"];
    let has_valid_ext = preset_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| valid_extensions.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false);
    if !has_valid_ext {
        return Err(format!("Invalid preset extension (expected .milk or .prjm): {}", path));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(ref mut renderer) = deck.renderer {
        if renderer.is_running() {
            renderer.send_command(&RendererCommand::LoadPreset { path: path.clone() })?;
            deck.preset_path = Some(path.clone());
            return Ok(format!("Loaded preset on deck {}: {}", deck_id, path));
        }
    }

    Err(format!("Deck {} not running", deck_id))
}

/// Set beat sensitivity on a specific deck
#[tauri::command]
fn set_beat_sensitivity(
    state: State<'_, AppState>,
    sensitivity: f32,
    deck_id: Option<u8>,
) -> Result<String, String> {
    let deck_id = deck_id.unwrap_or(0);
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(ref mut renderer) = deck.renderer {
        if renderer.is_running() {
            renderer.send_command(&RendererCommand::SetBeatSensitivity { value: sensitivity })?;
            deck.beat_sensitivity = sensitivity;
            return Ok(format!("Deck {} beat sensitivity set to {}", deck_id, sensitivity));
        }
    }

    Err(format!("Deck {} not running", deck_id))
}

/// Set volume on a specific deck (0.0 to 1.0)
#[tauri::command]
fn set_deck_volume(
    state: State<'_, AppState>,
    volume: f32,
    deck_id: Option<u8>,
) -> Result<String, String> {
    let deck_id = deck_id.unwrap_or(0);
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    deck.volume = volume.clamp(0.0, 1.0);
    Ok(format!("Deck {} volume set to {}", deck_id, deck.volume))
}

/// Toggle fullscreen on a specific deck
#[tauri::command]
fn toggle_fullscreen(state: State<'_, AppState>, deck_id: Option<u8>) -> Result<String, String> {
    let deck_id = deck_id.unwrap_or(0);
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(ref mut renderer) = deck.renderer {
        if renderer.is_running() {
            renderer.send_command(&RendererCommand::ToggleFullscreen)?;
            return Ok(format!("Deck {} fullscreen toggled", deck_id));
        }
    }

    Err(format!("Deck {} not running", deck_id))
}

/// Get the renderer executable name for the current platform
fn renderer_executable_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "opendrop-renderer.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "opendrop-renderer"
    }
}

/// Get the target triple for the current platform (used by Tauri sidecars)
fn target_triple() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64")
    )))]
    {
        "unknown"
    }
}

/// Find the renderer executable
fn find_renderer_executable() -> Result<String, String> {
    let exe_name = renderer_executable_name();
    let triple = target_triple();

    // Tauri sidecar name with target triple (e.g., opendrop-renderer-x86_64-pc-windows-msvc.exe)
    #[cfg(target_os = "windows")]
    let sidecar_name = format!("opendrop-renderer-{}.exe", triple);
    #[cfg(not(target_os = "windows"))]
    let sidecar_name = format!("opendrop-renderer-{}", triple);

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    // 1. Check next to current exe (Tauri sidecar location - with target triple)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Tauri bundled sidecar (with target triple suffix)
            candidates.push(exe_dir.join(&sidecar_name));
            // Plain name (development builds)
            candidates.push(exe_dir.join(exe_name));
        }
    }

    // 2. Check relative to working directory (development)
    if let Ok(cwd) = std::env::current_dir() {
        // Cargo workspace target directories
        candidates.push(cwd.join("target/release").join(exe_name));
        candidates.push(cwd.join("target/debug").join(exe_name));
        // From src-tauri directory
        candidates.push(cwd.join("../target/release").join(exe_name));
        candidates.push(cwd.join("../target/debug").join(exe_name));
    }

    // 3. Platform-specific system paths
    #[cfg(target_os = "linux")]
    {
        candidates.push(std::path::PathBuf::from("/usr/bin/opendrop-renderer"));
        candidates.push(std::path::PathBuf::from("/usr/local/bin/opendrop-renderer"));
        // Check user's home directory
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(std::path::PathBuf::from(home).join(".local/bin/opendrop-renderer"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Check Program Files
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            candidates.push(std::path::PathBuf::from(pf).join("OpenDrop").join("opendrop-renderer.exe"));
        }
        // Check AppData/Local
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(std::path::PathBuf::from(local_app_data).join("OpenDrop").join("opendrop-renderer.exe"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        candidates.push(std::path::PathBuf::from("/usr/local/bin/opendrop-renderer"));
        candidates.push(std::path::PathBuf::from("/opt/homebrew/bin/opendrop-renderer"));
        // Check within app bundle
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(bundle_path) = exe_path.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
                candidates.push(bundle_path.join("Resources/opendrop-renderer"));
            }
        }
    }

    // Search all candidates
    for candidate in &candidates {
        if candidate.exists() && candidate.is_file() {
            info!("Found renderer at: {}", candidate.display());
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    // Build helpful error message
    let searched_paths: Vec<String> = candidates
        .iter()
        .take(5) // Show first 5 paths in error
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    Err(format!(
        "Could not find opendrop-renderer executable. Searched:\n  - {}",
        searched_paths.join("\n  - ")
    ))
}

/// Get default preset directories for the current platform
fn get_default_preset_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "linux")]
    {
        // System-wide projectM presets
        dirs.push(std::path::PathBuf::from("/usr/share/projectM/presets"));
        dirs.push(std::path::PathBuf::from("/usr/local/share/projectM/presets"));

        // User-specific locations
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = std::path::PathBuf::from(home);
            dirs.push(home_path.join(".local/share/opendrop/presets"));
            dirs.push(home_path.join(".local/share/projectM/presets"));
            dirs.push(home_path.join("OpenDrop/presets"));
        }

        // XDG data directories
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
            let data_path = std::path::PathBuf::from(data_home);
            dirs.push(data_path.join("opendrop/presets"));
            dirs.push(data_path.join("projectM/presets"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        // AppData locations
        if let Some(app_data) = std::env::var_os("APPDATA") {
            let app_path = std::path::PathBuf::from(app_data);
            dirs.push(app_path.join("OpenDrop/presets"));
            dirs.push(app_path.join("projectM/presets"));
        }

        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let local_path = std::path::PathBuf::from(local_app_data);
            dirs.push(local_path.join("OpenDrop/presets"));
            dirs.push(local_path.join("projectM/presets"));
        }

        // Program Files locations
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            let pf_path = std::path::PathBuf::from(pf);
            dirs.push(pf_path.join("OpenDrop/presets"));
            dirs.push(pf_path.join("projectM/presets"));
        }

        // User's Documents folder
        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            let user_path = std::path::PathBuf::from(user_profile);
            dirs.push(user_path.join("Documents/OpenDrop/presets"));
            dirs.push(user_path.join("OpenDrop/presets"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Application Support
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = std::path::PathBuf::from(home);
            dirs.push(home_path.join("Library/Application Support/OpenDrop/presets"));
            dirs.push(home_path.join("Library/Application Support/projectM/presets"));
            dirs.push(home_path.join("OpenDrop/presets"));
        }

        // System locations
        dirs.push(std::path::PathBuf::from("/usr/local/share/projectM/presets"));
        dirs.push(std::path::PathBuf::from("/opt/homebrew/share/projectM/presets"));
    }

    // Check next to executable (for portable installs and bundled resources)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Direct next to executable
            dirs.push(exe_dir.join("presets"));
            // Tauri bundles resources to 'resources/' subdirectory
            dirs.push(exe_dir.join("resources/presets"));

            // Also check parent for app bundles
            if let Some(parent) = exe_dir.parent() {
                dirs.push(parent.join("presets"));
                dirs.push(parent.join("resources/presets"));
                // macOS app bundle
                dirs.push(parent.join("Resources/presets"));
            }

            // AppImage: check $APPDIR environment variable
            if let Some(appdir) = std::env::var_os("APPDIR") {
                let appdir_path = std::path::PathBuf::from(appdir);
                dirs.push(appdir_path.join("presets"));
                dirs.push(appdir_path.join("resources/presets"));
            }
        }
    }

    dirs
}

/// Get the first existing preset directory, or create the user's default
fn get_preset_dir() -> String {
    let default_dirs = get_default_preset_dirs();

    // Return the first directory that exists and has presets
    for dir in &default_dirs {
        if dir.exists() && dir.is_dir() {
            // Check if it has any .milk files
            if let Ok(entries) = std::fs::read_dir(dir) {
                let has_presets = entries.filter_map(|e| e.ok()).any(|entry| {
                    let path = entry.path();
                    path.extension().is_some_and(|ext| ext == "milk")
                        || (path.is_dir() && path.read_dir().ok().is_some_and(|mut d| d.next().is_some()))
                });
                if has_presets {
                    return dir.to_string_lossy().to_string();
                }
            }
        }
    }

    // Return the first candidate (user's data directory) even if it doesn't exist yet
    default_dirs
        .first()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            #[cfg(target_os = "windows")]
            {
                std::env::var("APPDATA")
                    .map(|p| format!("{}\\OpenDrop\\presets", p))
                    .unwrap_or_else(|_| "C:\\OpenDrop\\presets".to_string())
            }
            #[cfg(not(target_os = "windows"))]
            {
                std::env::var("HOME")
                    .map(|p| format!("{}/.local/share/opendrop/presets", p))
                    .unwrap_or_else(|_| "/tmp/opendrop/presets".to_string())
            }
        })
}

/// Get all preset directories that exist
#[tauri::command]
fn get_preset_directories() -> Vec<String> {
    get_default_preset_dirs()
        .into_iter()
        .filter(|p| p.exists() && p.is_dir())
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

/// Get default texture directories for the current platform
fn get_default_texture_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "linux")]
    {
        // System-wide projectM textures
        dirs.push(std::path::PathBuf::from("/usr/share/projectM/textures"));
        dirs.push(std::path::PathBuf::from("/usr/local/share/projectM/textures"));

        // User-specific locations
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = std::path::PathBuf::from(home);
            dirs.push(home_path.join(".local/share/opendrop/textures"));
            dirs.push(home_path.join(".local/share/projectM/textures"));
            dirs.push(home_path.join("OpenDrop/textures"));
        }

        // XDG data directories
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
            let data_path = std::path::PathBuf::from(data_home);
            dirs.push(data_path.join("opendrop/textures"));
            dirs.push(data_path.join("projectM/textures"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        // AppData locations
        if let Some(app_data) = std::env::var_os("APPDATA") {
            let app_path = std::path::PathBuf::from(app_data);
            dirs.push(app_path.join("OpenDrop/textures"));
            dirs.push(app_path.join("projectM/textures"));
        }

        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let local_path = std::path::PathBuf::from(local_app_data);
            dirs.push(local_path.join("OpenDrop/textures"));
            dirs.push(local_path.join("projectM/textures"));
        }

        // Program Files locations
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            let pf_path = std::path::PathBuf::from(pf);
            dirs.push(pf_path.join("OpenDrop/textures"));
            dirs.push(pf_path.join("projectM/textures"));
        }

        // User's Documents folder
        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            let user_path = std::path::PathBuf::from(user_profile);
            dirs.push(user_path.join("Documents/OpenDrop/textures"));
            dirs.push(user_path.join("OpenDrop/textures"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Application Support
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = std::path::PathBuf::from(home);
            dirs.push(home_path.join("Library/Application Support/OpenDrop/textures"));
            dirs.push(home_path.join("Library/Application Support/projectM/textures"));
            dirs.push(home_path.join("OpenDrop/textures"));
        }

        // System locations
        dirs.push(std::path::PathBuf::from("/usr/local/share/projectM/textures"));
        dirs.push(std::path::PathBuf::from("/opt/homebrew/share/projectM/textures"));
    }

    // Check next to executable (for portable installs and bundled resources)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Direct next to executable
            dirs.push(exe_dir.join("textures"));
            // Tauri bundles resources to 'resources/' subdirectory
            dirs.push(exe_dir.join("resources/textures"));

            // Also check parent for app bundles
            if let Some(parent) = exe_dir.parent() {
                dirs.push(parent.join("textures"));
                dirs.push(parent.join("resources/textures"));
                // macOS app bundle
                dirs.push(parent.join("Resources/textures"));
            }

            // AppImage: check $APPDIR environment variable
            if let Some(appdir) = std::env::var_os("APPDIR") {
                let appdir_path = std::path::PathBuf::from(appdir);
                dirs.push(appdir_path.join("textures"));
                dirs.push(appdir_path.join("resources/textures"));
            }
        }
    }

    // Also include preset directories since textures are often co-located with presets
    dirs.extend(get_default_preset_dirs());

    dirs
}

/// Get all texture directories that exist
#[tauri::command]
fn get_texture_directories() -> Vec<String> {
    get_default_texture_dirs()
        .into_iter()
        .filter(|p| p.exists() && p.is_dir())
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

/// List available audio input devices
#[tauri::command]
fn list_audio_devices() -> Vec<AudioDeviceInfo> {
    AudioEngine::list_devices()
        .into_iter()
        .map(AudioDeviceInfo::from)
        .collect()
}

/// Start audio capture
#[tauri::command]
fn start_audio(
    state: State<'_, AppState>,
    device_name: Option<String>,
) -> Result<String, String> {
    let mut audio_guard = state.audio_engine.lock().map_err(|e| e.to_string())?;

    if audio_guard.is_running() {
        return Err("Audio already running".to_string());
    }

    let config = AudioConfig {
        device_name,
        ..Default::default()
    };

    audio_guard.start(config).map_err(|e| e.to_string())?;

    Ok("Audio capture started".to_string())
}

/// Stop audio capture
#[tauri::command]
fn stop_audio(state: State<'_, AppState>) -> Result<String, String> {
    let mut audio_guard = state.audio_engine.lock().map_err(|e| e.to_string())?;
    audio_guard.stop();
    Ok("Audio capture stopped".to_string())
}

/// Get status for all decks
#[tauri::command]
fn get_multi_deck_status(state: State<'_, AppState>) -> Result<MultiDeckStatus, String> {
    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let audio_guard = state.audio_engine.lock().map_err(|e| e.to_string())?;
    let crossfader_guard = state.crossfader.lock().map_err(|e| e.to_string())?;
    let compositor_guard = state.compositor.lock().map_err(|e| e.to_string())?;

    let mut deck_infos: Vec<DeckInfo> = Vec::new();
    for id in 0..MAX_DECKS {
        if let Some(deck) = decks_guard.get_mut(&id) {
            // Get health info from renderer if available
            let (health, uptime, crashes) = if let Some(ref renderer) = deck.renderer {
                (
                    Some(renderer.get_health()),
                    Some(renderer.uptime_secs()),
                    Some(renderer.crash_count),
                )
            } else {
                (None, None, None)
            };

            deck_infos.push(DeckInfo {
                id,
                running: deck.is_running(),
                preset: deck.preset_path.clone(),
                volume: deck.volume,
                beat_sensitivity: deck.beat_sensitivity,
                playlist: PlaylistInfo::from(&deck.playlist),
                health,
                uptime_secs: uptime,
                crash_count: crashes,
            });
        }
    }

    Ok(MultiDeckStatus {
        decks: deck_infos,
        audio_running: audio_guard.is_running(),
        preset_dir: get_preset_dir(),
        crossfader: CrossfaderInfo::from(&*crossfader_guard),
        compositor: CompositorInfo::from(&*compositor_guard),
    })
}

/// Get status (backward compatible - returns deck 0 status)
#[tauri::command]
fn get_status(state: State<'_, AppState>) -> Result<VisualizerStatus, String> {
    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let audio_guard = state.audio_engine.lock().map_err(|e| e.to_string())?;

    let deck = decks_guard.get_mut(&0).ok_or("Deck 0 not found")?;

    Ok(VisualizerStatus {
        running: deck.is_running(),
        audio_running: audio_guard.is_running(),
        current_preset: deck.preset_path.clone(),
        preset_dir: get_preset_dir(),
    })
}

/// Get projectM version
#[tauri::command]
fn get_projectm_version() -> String {
    projectm_rs::ProjectM::version()
}

/// List presets in directories (defaults + custom paths, or specific directories if provided)
#[tauri::command]
fn list_presets(dirs: Option<Vec<String>>) -> Result<Vec<PresetInfo>, String> {
    let mut presets = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    fn scan_dir(
        path: &std::path::Path,
        presets: &mut Vec<PresetInfo>,
        seen_names: &mut std::collections::HashSet<String>,
        depth: usize,
    ) {
        if depth > 4 {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    scan_dir(&path, presets, seen_names, depth + 1);
                } else if path.extension().is_some_and(|ext| ext == "milk" || ext == "prjm") {
                    let path_str = path.to_string_lossy().to_string();
                    // Avoid duplicates by preset name (not full path)
                    // This prevents bundled presets from appearing twice with different paths
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        let name_str = name.to_string();
                        if !seen_names.contains(&name_str) {
                            seen_names.insert(name_str.clone());
                            presets.push(PresetInfo {
                                name: name_str,
                                path: path_str,
                            });
                        }
                    }
                }
            }
        }
    }

    // Always search default directories first
    for dir_path in get_default_preset_dirs() {
        if dir_path.exists() && dir_path.is_dir() {
            scan_dir(&dir_path, &mut presets, &mut seen_names, 0);
        }
    }

    // If additional custom directories are provided, search those too
    if let Some(custom_dirs) = dirs {
        for dir_str in custom_dirs {
            let dir_path = std::path::Path::new(&dir_str);
            if dir_path.exists() && dir_path.is_dir() {
                scan_dir(dir_path, &mut presets, &mut seen_names, 0);
            }
        }
    }

    presets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(presets)
}

/// Import presets from a source folder to the target directory
#[tauri::command]
fn import_presets_from_folder(
    source_dir: String,
    target_dir: Option<String>,
) -> Result<ImportResult, String> {
    // Use provided target or default preset location
    let target = target_dir.unwrap_or_else(|| "/usr/share/projectM/presets".to_string());

    let source_path = std::path::Path::new(&source_dir);
    let target_path = std::path::Path::new(&target);

    if !source_path.exists() {
        return Err(format!("Source directory does not exist: {}", source_dir));
    }

    // Create target directory if it doesn't exist
    std::fs::create_dir_all(&target_path).map_err(|e| e.to_string())?;

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();

    fn copy_presets(
        source: &std::path::Path,
        target: &std::path::Path,
        imported: &mut usize,
        skipped: &mut usize,
        errors: &mut Vec<String>,
        depth: usize,
    ) {
        if depth > 5 {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(source) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if path.is_dir() {
                    let new_target = target.join(file_name);
                    if std::fs::create_dir_all(&new_target).is_ok() {
                        copy_presets(&path, &new_target, imported, skipped, errors, depth + 1);
                    }
                } else if path.extension().is_some_and(|ext| ext == "milk" || ext == "prjm") {
                    let target_file = target.join(file_name);
                    if target_file.exists() {
                        *skipped += 1;
                    } else {
                        match std::fs::copy(&path, &target_file) {
                            Ok(_) => *imported += 1,
                            Err(e) => errors.push(format!("{}: {}", file_name, e)),
                        }
                    }
                }
            }
        }
    }

    copy_presets(
        source_path,
        target_path,
        &mut imported,
        &mut skipped,
        &mut errors,
        0,
    );

    Ok(ImportResult {
        imported,
        skipped,
        errors,
        target_dir: target,
    })
}

/// Export a playlist to a JSON file
#[tauri::command]
fn export_playlist(
    state: State<'_, AppState>,
    deck_id: u8,
    file_path: String,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard
        .get(&deck_id)
        .ok_or_else(|| format!("Deck {} not found", deck_id))?;

    let playlist_info = PlaylistInfo::from(&deck.playlist);
    let json = serde_json::to_string_pretty(&playlist_info).map_err(|e| e.to_string())?;

    std::fs::write(&file_path, &json).map_err(|e| e.to_string())?;

    Ok(format!("Playlist exported to {}", file_path))
}

/// Import a playlist from a JSON file
#[tauri::command]
fn import_playlist(
    state: State<'_, AppState>,
    deck_id: u8,
    file_path: String,
    replace: bool,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let json = std::fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    let imported: PlaylistInfo = serde_json::from_str(&json).map_err(|e| e.to_string())?;

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard
        .get_mut(&deck_id)
        .ok_or_else(|| format!("Deck {} not found", deck_id))?;

    if replace {
        deck.playlist.items.clear();
        deck.playlist.current_index = 0;
    }

    let initial_count = deck.playlist.items.len();
    for item in imported.items {
        // Check if file exists before adding
        if std::path::Path::new(&item.path).exists() {
            deck.playlist.items.push(PlaylistItem {
                name: item.name,
                path: item.path,
            });
        }
    }

    // Apply settings if replacing
    if replace {
        deck.playlist.name = imported.name;
        deck.playlist.shuffle = imported.shuffle;
        deck.playlist.auto_cycle = imported.auto_cycle;
        deck.playlist.cycle_duration_secs = imported.cycle_duration_secs;
    }

    let added = deck.playlist.items.len() - initial_count;
    Ok(format!("Imported {} presets to deck {}", added, deck_id))
}

/// Result of preset import operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
    pub target_dir: String,
}

/// Pump audio from capture to all active decks + handle auto-cycle
#[tauri::command]
fn pump_audio(state: State<'_, AppState>) -> Result<u32, String> {
    let audio_guard = state.audio_engine.lock().map_err(|e| e.to_string())?;
    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let crossfader_guard = state.crossfader.lock().map_err(|e| e.to_string())?;

    let mut total_samples_sent = 0u32;
    let now = std::time::Instant::now();

    // Collect all audio samples first
    let mut all_samples: Vec<Vec<f32>> = Vec::new();
    while let Some(samples) = audio_guard.try_recv() {
        all_samples.push(samples);
    }

    // Calculate RMS levels for VU meters from collected samples
    if !all_samples.is_empty() {
        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;
        let mut count = 0usize;

        for samples in &all_samples {
            // Samples are interleaved stereo: [L, R, L, R, ...]
            for chunk in samples.chunks(2) {
                if chunk.len() == 2 {
                    sum_l += chunk[0] * chunk[0];
                    sum_r += chunk[1] * chunk[1];
                    count += 1;
                }
            }
        }

        if count > 0 {
            let rms_l = (sum_l / count as f32).sqrt();
            let rms_r = (sum_r / count as f32).sqrt();
            // Store levels (clamped to 0-1)
            if let Ok(mut levels) = state.audio_levels.lock() {
                *levels = (rms_l.min(1.0), rms_r.min(1.0));
            }
        }
    }

    // Send audio to all running decks + check auto-cycle
    for id in 0..MAX_DECKS {
        if let Some(deck) = decks_guard.get_mut(&id) {
            let is_running = deck.renderer.as_mut().is_some_and(|r| r.is_running());

            if is_running {
                // Check auto-cycle timer
                if deck.playlist.auto_cycle && !deck.playlist.items.is_empty() {
                    let should_cycle = match deck.last_cycle_time {
                        Some(last_time) => {
                            now.duration_since(last_time).as_secs() >= deck.playlist.cycle_duration_secs as u64
                        }
                        None => true, // First time, start the timer
                    };

                    if should_cycle {
                        deck.last_cycle_time = Some(now);
                        if let Some(item) = deck.playlist.advance() {
                            let path = item.path.clone();
                            deck.preset_path = Some(path.clone());
                            if let Some(ref mut renderer) = deck.renderer {
                                let _ = renderer.send_command(&RendererCommand::LoadPreset { path });
                            }
                        }
                    }
                }

                // Send audio samples with crossfader applied
                if !all_samples.is_empty() {
                    // Calculate effective volume: deck volume * crossfader position
                    let crossfader_vol = crossfader_guard.volume_for_deck(id);
                    let effective_volume = deck.volume * crossfader_vol;

                    if let Some(ref mut renderer) = deck.renderer {
                        for samples in &all_samples {
                            let scaled_samples: Vec<f32> = if effective_volume < 1.0 {
                                samples.iter().map(|s| s * effective_volume).collect()
                            } else {
                                samples.clone()
                            };

                            if renderer.send_command(&RendererCommand::Audio { samples: scaled_samples }).is_ok() {
                                total_samples_sent += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(total_samples_sent)
}

/// Get current audio levels for VU meters
#[tauri::command]
fn get_audio_levels(state: State<'_, AppState>) -> Result<(f32, f32), String> {
    let levels = state.audio_levels.lock().map_err(|e| e.to_string())?;
    Ok(*levels)
}

// ============ Playlist Commands ============

/// Add a preset to a deck's playlist
#[tauri::command]
fn playlist_add(
    state: State<'_, AppState>,
    deck_id: u8,
    name: String,
    path: String,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    deck.playlist.items.push(PlaylistItem { name, path });
    Ok(format!("Added to deck {} playlist", deck_id))
}

/// Remove a preset from a deck's playlist by index
#[tauri::command]
fn playlist_remove(
    state: State<'_, AppState>,
    deck_id: u8,
    index: usize,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if index >= deck.playlist.items.len() {
        return Err("Index out of bounds".to_string());
    }

    deck.playlist.items.remove(index);
    // Adjust current_index if needed
    if deck.playlist.current_index >= deck.playlist.items.len() && !deck.playlist.items.is_empty() {
        deck.playlist.current_index = deck.playlist.items.len() - 1;
    }
    Ok(format!("Removed from deck {} playlist", deck_id))
}

/// Clear a deck's playlist
#[tauri::command]
fn playlist_clear(state: State<'_, AppState>, deck_id: u8) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    deck.playlist.items.clear();
    deck.playlist.current_index = 0;
    Ok(format!("Deck {} playlist cleared", deck_id))
}

/// Move to next preset in playlist
#[tauri::command]
fn playlist_next(state: State<'_, AppState>, deck_id: u8) -> Result<Option<String>, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(item) = deck.playlist.advance() {
        let path = item.path.clone();
        deck.preset_path = Some(path.clone());

        // Load preset if deck is running
        if let Some(ref mut renderer) = deck.renderer {
            if renderer.is_running() {
                let _ = renderer.send_command(&RendererCommand::LoadPreset { path: path.clone() });
            }
        }
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

/// Move to previous preset in playlist
#[tauri::command]
fn playlist_previous(state: State<'_, AppState>, deck_id: u8) -> Result<Option<String>, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(item) = deck.playlist.previous() {
        let path = item.path.clone();
        deck.preset_path = Some(path.clone());

        // Load preset if deck is running
        if let Some(ref mut renderer) = deck.renderer {
            if renderer.is_running() {
                let _ = renderer.send_command(&RendererCommand::LoadPreset { path: path.clone() });
            }
        }
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

/// Set playlist settings (shuffle, auto_cycle, duration)
#[tauri::command]
fn playlist_set_settings(
    state: State<'_, AppState>,
    deck_id: u8,
    shuffle: Option<bool>,
    auto_cycle: Option<bool>,
    cycle_duration_secs: Option<u32>,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(s) = shuffle {
        deck.playlist.shuffle = s;
    }
    if let Some(ac) = auto_cycle {
        deck.playlist.auto_cycle = ac;
        if ac {
            deck.last_cycle_time = Some(std::time::Instant::now());
        }
    }
    if let Some(dur) = cycle_duration_secs {
        deck.playlist.cycle_duration_secs = dur.max(5); // Min 5 seconds
    }

    Ok("Playlist settings updated".to_string())
}

/// Jump to a specific index in playlist
#[tauri::command]
fn playlist_jump_to(
    state: State<'_, AppState>,
    deck_id: u8,
    index: usize,
) -> Result<Option<String>, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if index >= deck.playlist.items.len() {
        return Err("Index out of bounds".to_string());
    }

    deck.playlist.current_index = index;
    let path = deck.playlist.items[index].path.clone();
    deck.preset_path = Some(path.clone());

    // Load preset if deck is running
    if let Some(ref mut renderer) = deck.renderer {
        if renderer.is_running() {
            let _ = renderer.send_command(&RendererCommand::LoadPreset { path: path.clone() });
        }
    }

    Ok(Some(path))
}

/// Reorder playlist item (move from one index to another)
#[tauri::command]
fn playlist_reorder(
    state: State<'_, AppState>,
    deck_id: u8,
    from_index: usize,
    to_index: usize,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    let len = deck.playlist.items.len();
    if from_index >= len || to_index >= len {
        return Err("Index out of bounds".to_string());
    }

    let item = deck.playlist.items.remove(from_index);
    deck.playlist.items.insert(to_index, item);

    // Adjust current_index if needed
    if deck.playlist.current_index == from_index {
        deck.playlist.current_index = to_index;
    } else if from_index < deck.playlist.current_index && to_index >= deck.playlist.current_index {
        deck.playlist.current_index -= 1;
    } else if from_index > deck.playlist.current_index && to_index <= deck.playlist.current_index {
        deck.playlist.current_index += 1;
    }

    Ok("Playlist reordered".to_string())
}

// ============ Crossfader Commands ============

/// Set crossfader position (0.0 = Side A, 1.0 = Side B)
#[tauri::command]
fn crossfader_set_position(
    state: State<'_, AppState>,
    position: f32,
) -> Result<String, String> {
    let mut crossfader_guard = state.crossfader.lock().map_err(|e| e.to_string())?;
    crossfader_guard.position = position.clamp(0.0, 1.0);
    Ok(format!("Crossfader position set to {:.2}", crossfader_guard.position))
}

/// Enable or disable the crossfader
#[tauri::command]
fn crossfader_set_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<String, String> {
    let mut crossfader_guard = state.crossfader.lock().map_err(|e| e.to_string())?;
    crossfader_guard.enabled = enabled;
    Ok(format!("Crossfader {}", if enabled { "enabled" } else { "disabled" }))
}

/// Set crossfader curve type
#[tauri::command]
fn crossfader_set_curve(
    state: State<'_, AppState>,
    curve: String,
) -> Result<String, String> {
    let mut crossfader_guard = state.crossfader.lock().map_err(|e| e.to_string())?;
    crossfader_guard.curve = match curve.as_str() {
        "linear" => CrossfaderCurve::Linear,
        "equal_power" | "equalPower" => CrossfaderCurve::EqualPower,
        _ => return Err(format!("Unknown curve type: {}. Use 'linear' or 'equal_power'", curve)),
    };
    Ok(format!("Crossfader curve set to {:?}", crossfader_guard.curve))
}

/// Assign a deck to Side A or Side B
#[tauri::command]
fn crossfader_assign_deck(
    state: State<'_, AppState>,
    deck_id: u8,
    side: String,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut crossfader_guard = state.crossfader.lock().map_err(|e| e.to_string())?;

    // Remove from both sides first
    crossfader_guard.side_a.retain(|&id| id != deck_id);
    crossfader_guard.side_b.retain(|&id| id != deck_id);

    // Add to the specified side
    match side.to_lowercase().as_str() {
        "a" => crossfader_guard.side_a.push(deck_id),
        "b" => crossfader_guard.side_b.push(deck_id),
        "none" => {} // Already removed from both sides
        _ => return Err(format!("Invalid side: {}. Use 'a', 'b', or 'none'", side)),
    }

    Ok(format!("Deck {} assigned to side {}", deck_id + 1, side.to_uppercase()))
}

/// Get current crossfader configuration
#[tauri::command]
fn crossfader_get_config(state: State<'_, AppState>) -> Result<CrossfaderInfo, String> {
    let crossfader_guard = state.crossfader.lock().map_err(|e| e.to_string())?;
    Ok(CrossfaderInfo::from(&*crossfader_guard))
}

// ============ Compositor Commands ============

/// Enable or disable the compositor
#[tauri::command]
fn compositor_set_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<String, String> {
    let mut compositor_guard = state.compositor.lock().map_err(|e| e.to_string())?;
    compositor_guard.enabled = enabled;
    Ok(format!("Compositor {}", if enabled { "enabled" } else { "disabled" }))
}

/// Set compositor output resolution
#[tauri::command]
fn compositor_set_resolution(
    state: State<'_, AppState>,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let mut compositor_guard = state.compositor.lock().map_err(|e| e.to_string())?;
    compositor_guard.output_width = width;
    compositor_guard.output_height = height;
    Ok(format!("Compositor resolution set to {}x{}", width, height))
}

/// Set deck opacity in compositor
#[tauri::command]
fn compositor_set_deck_opacity(
    state: State<'_, AppState>,
    deck_id: u8,
    opacity: f32,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut compositor_guard = state.compositor.lock().map_err(|e| e.to_string())?;
    if let Some(settings) = compositor_guard.deck_settings.get_mut(&deck_id) {
        settings.opacity = opacity.clamp(0.0, 1.0);
        Ok(format!("Deck {} opacity set to {:.0}%", deck_id + 1, settings.opacity * 100.0))
    } else {
        Err(format!("Deck {} not found in compositor", deck_id + 1))
    }
}

/// Set deck blend mode in compositor
#[tauri::command]
fn compositor_set_deck_blend_mode(
    state: State<'_, AppState>,
    deck_id: u8,
    blend_mode: String,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mode = match blend_mode.to_lowercase().as_str() {
        "normal" => BlendMode::Normal,
        "add" | "additive" => BlendMode::Add,
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "overlay" => BlendMode::Overlay,
        _ => return Err(format!("Unknown blend mode: {}. Use: normal, add, multiply, screen, overlay", blend_mode)),
    };

    let mut compositor_guard = state.compositor.lock().map_err(|e| e.to_string())?;
    if let Some(settings) = compositor_guard.deck_settings.get_mut(&deck_id) {
        settings.blend_mode = mode;
        Ok(format!("Deck {} blend mode set to {:?}", deck_id + 1, mode))
    } else {
        Err(format!("Deck {} not found in compositor", deck_id + 1))
    }
}

/// Set deck layer order in compositor
#[tauri::command]
fn compositor_set_deck_layer(
    state: State<'_, AppState>,
    deck_id: u8,
    layer_order: i32,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut compositor_guard = state.compositor.lock().map_err(|e| e.to_string())?;
    if let Some(settings) = compositor_guard.deck_settings.get_mut(&deck_id) {
        settings.layer_order = layer_order;
        Ok(format!("Deck {} layer order set to {}", deck_id + 1, layer_order))
    } else {
        Err(format!("Deck {} not found in compositor", deck_id + 1))
    }
}

/// Enable or disable a deck in compositor
#[tauri::command]
fn compositor_set_deck_enabled(
    state: State<'_, AppState>,
    deck_id: u8,
    enabled: bool,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut compositor_guard = state.compositor.lock().map_err(|e| e.to_string())?;
    if let Some(settings) = compositor_guard.deck_settings.get_mut(&deck_id) {
        settings.enabled = enabled;
        Ok(format!("Deck {} {} in compositor", deck_id + 1, if enabled { "enabled" } else { "disabled" }))
    } else {
        Err(format!("Deck {} not found in compositor", deck_id + 1))
    }
}

/// Link compositor to crossfader (opacity follows crossfader position)
#[tauri::command]
fn compositor_link_crossfader(
    state: State<'_, AppState>,
    linked: bool,
) -> Result<String, String> {
    let mut compositor_guard = state.compositor.lock().map_err(|e| e.to_string())?;
    compositor_guard.link_to_crossfader = linked;
    Ok(format!("Compositor crossfader link {}", if linked { "enabled" } else { "disabled" }))
}

/// Get current compositor configuration
#[tauri::command]
fn compositor_get_config(state: State<'_, AppState>) -> Result<CompositorInfo, String> {
    let compositor_guard = state.compositor.lock().map_err(|e| e.to_string())?;
    Ok(CompositorInfo::from(&*compositor_guard))
}

// ============ Monitor Commands ============

/// Information about a display monitor
#[derive(Serialize, Deserialize, Clone)]
pub struct MonitorInfo {
    /// Monitor index (0-based)
    pub index: usize,
    /// Monitor name/identifier
    pub name: String,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Whether this is the primary monitor
    pub is_primary: bool,
}

/// List available display monitors
#[tauri::command]
fn list_monitors() -> Vec<MonitorInfo> {
    let mut monitors = Vec::new();

    #[cfg(target_os = "linux")]
    {
        // Use xrandr on Linux to get monitor info
        if let Ok(output) = std::process::Command::new("xrandr")
            .arg("--query")
            .output()
        {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                let mut index = 0;
                for line in stdout.lines() {
                    // Lines like: "HDMI-1 connected primary 1920x1080+0+0 ..."
                    // or: "DP-1 connected 2560x1440+1920+0 ..."
                    if line.contains(" connected") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 3 {
                            let name = parts[0].to_string();
                            let is_primary = line.contains(" primary ");

                            // Find resolution: look for NNNNxNNNN pattern
                            let mut width = 0u32;
                            let mut height = 0u32;
                            for part in &parts {
                                if part.contains('x') && part.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                                    let res_part = part.split('+').next().unwrap_or(part);
                                    let dims: Vec<&str> = res_part.split('x').collect();
                                    if dims.len() == 2 {
                                        if let (Ok(w), Ok(h)) = (dims[0].parse(), dims[1].parse()) {
                                            width = w;
                                            height = h;
                                            break;
                                        }
                                    }
                                }
                            }

                            if width > 0 && height > 0 {
                                monitors.push(MonitorInfo {
                                    index,
                                    name,
                                    width,
                                    height,
                                    is_primary,
                                });
                                index += 1;
                            }
                        }
                    }
                }
            }
        }

        // Fallback: if xrandr failed, add a default entry
        if monitors.is_empty() {
            monitors.push(MonitorInfo {
                index: 0,
                name: "Primary".to_string(),
                width: 1920,
                height: 1080,
                is_primary: true,
            });
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::mem;
        use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
        use windows::Win32::Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
        };

        // Callback data structure
        struct MonitorData {
            monitors: Vec<MonitorInfo>,
        }

        unsafe extern "system" fn monitor_enum_proc(
            hmonitor: HMONITOR,
            _hdc: HDC,
            _lprect: *mut RECT,
            lparam: LPARAM,
        ) -> BOOL {
            let data = &mut *(lparam.0 as *mut MonitorData);

            let mut info: MONITORINFOEXW = mem::zeroed();
            info.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;

            if GetMonitorInfoW(hmonitor, &mut info.monitorInfo as *mut _).as_bool() {
                let rect = info.monitorInfo.rcMonitor;
                let width = (rect.right - rect.left) as u32;
                let height = (rect.bottom - rect.top) as u32;

                // Convert device name from wide string
                let name_slice = &info.szDevice;
                let name_len = name_slice.iter().position(|&c| c == 0).unwrap_or(name_slice.len());
                let name = String::from_utf16_lossy(&name_slice[..name_len]);

                let is_primary = (info.monitorInfo.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY = 1

                data.monitors.push(MonitorInfo {
                    index: data.monitors.len(),
                    name,
                    width,
                    height,
                    is_primary,
                });
            }

            BOOL::from(true) // Continue enumeration
        }

        let mut data = MonitorData {
            monitors: Vec::new(),
        };

        unsafe {
            let _ = EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(monitor_enum_proc),
                LPARAM(&mut data as *mut _ as isize),
            );
        }

        monitors = data.monitors;

        // Fallback if enumeration failed
        if monitors.is_empty() {
            monitors.push(MonitorInfo {
                index: 0,
                name: "Primary".to_string(),
                width: 1920,
                height: 1080,
                is_primary: true,
            });
        }
    }

    #[cfg(target_os = "macos")]
    {
        use core_graphics::display::{CGDisplay, CGMainDisplayID};

        // Get all active displays
        let max_displays = 16u32;
        let mut display_ids: Vec<u32> = vec![0; max_displays as usize];
        let mut display_count: u32 = 0;

        unsafe {
            let result = core_graphics::display::CGGetActiveDisplayList(
                max_displays,
                display_ids.as_mut_ptr(),
                &mut display_count,
            );

            if result == 0 && display_count > 0 {
                let main_display_id = CGMainDisplayID();

                for i in 0..display_count as usize {
                    let display_id = display_ids[i];
                    let display = CGDisplay::new(display_id);

                    let bounds = display.bounds();
                    let width = bounds.size.width as u32;
                    let height = bounds.size.height as u32;

                    // Generate display name
                    let name = if display_id == main_display_id {
                        "Main Display".to_string()
                    } else {
                        format!("Display {}", i + 1)
                    };

                    monitors.push(MonitorInfo {
                        index: i,
                        name,
                        width,
                        height,
                        is_primary: display_id == main_display_id,
                    });
                }
            }
        }

        // Fallback if enumeration failed
        if monitors.is_empty() {
            monitors.push(MonitorInfo {
                index: 0,
                name: "Primary".to_string(),
                width: 1920,
                height: 1080,
                is_primary: true,
            });
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        monitors.push(MonitorInfo {
            index: 0,
            name: "Primary".to_string(),
            width: 1920,
            height: 1080,
            is_primary: true,
        });
    }

    monitors
}

// ============ Video Output Commands ============

/// List available video output devices (v4l2loopback on Linux, Spout on Windows)
#[tauri::command]
fn list_video_outputs() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        opendrop_core::video::V4l2Output::list_devices()
            .into_iter()
            .map(|d| format!("{}:{}", d.path.display(), d.name))
            .collect()
    }
    #[cfg(target_os = "windows")]
    {
        // Spout is always available if SpoutLibrary.dll is present
        if opendrop_core::video::SpoutOutput::is_available() {
            vec!["Spout:OpenDrop".to_string()]
        } else {
            vec![]
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        vec![]
    }
}

/// Enable video output on a deck (sends frames to v4l2loopback)
#[tauri::command]
fn set_deck_video_output(
    state: State<'_, AppState>,
    deck_id: u8,
    enabled: bool,
    device_path: Option<String>,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(ref mut renderer) = deck.renderer {
        if renderer.is_running() {
            renderer.send_command(&RendererCommand::SetVideoOutput {
                enabled,
                device_path: device_path.clone(),
            })?;
            let status = if enabled {
                format!("Video output enabled on deck {} ({})",
                    deck_id,
                    device_path.unwrap_or_else(|| "/dev/video10".to_string()))
            } else {
                format!("Video output disabled on deck {}", deck_id)
            };
            return Ok(status);
        }
    }

    Err(format!("Deck {} not running", deck_id))
}

// ============ NDI Output Commands ============

/// Check if NDI runtime is available
#[tauri::command]
fn is_ndi_available() -> bool {
    opendrop_core::video::NdiOutput::is_available()
}

/// Enable NDI output on a deck (streams to network)
#[tauri::command]
fn set_deck_ndi_output(
    state: State<'_, AppState>,
    deck_id: u8,
    enabled: bool,
    name: Option<String>,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(ref mut renderer) = deck.renderer {
        if renderer.is_running() {
            renderer.send_command(&RendererCommand::SetNdiOutput {
                enabled,
                name: name.clone(),
            })?;
            let status = if enabled {
                format!("NDI output enabled on deck {} ({})",
                    deck_id,
                    name.unwrap_or_else(|| format!("OpenDrop Deck {}", deck_id + 1)))
            } else {
                format!("NDI output disabled on deck {}", deck_id)
            };
            return Ok(status);
        }
    }

    Err(format!("Deck {} not running", deck_id))
}

/// Set texture search paths for a deck
#[tauri::command]
fn set_deck_texture_paths(
    state: State<'_, AppState>,
    deck_id: u8,
    paths: Vec<String>,
) -> Result<String, String> {
    if deck_id >= MAX_DECKS {
        return Err(format!("Invalid deck ID: {}", deck_id));
    }

    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let deck = decks_guard.get_mut(&deck_id).ok_or("Deck not found")?;

    if let Some(ref mut renderer) = deck.renderer {
        if renderer.is_running() {
            renderer.send_command(&RendererCommand::SetTexturePaths {
                paths: paths.clone(),
            })?;
            return Ok(format!("Set {} texture paths on deck {}", paths.len(), deck_id));
        }
    }

    Err(format!("Deck {} not running", deck_id))
}

/// Set texture paths for all running decks
#[tauri::command]
fn set_all_decks_texture_paths(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<String, String> {
    let mut decks_guard = state.decks.lock().map_err(|e| e.to_string())?;
    let mut count = 0;

    for deck_id in 0..MAX_DECKS {
        if let Some(deck) = decks_guard.get_mut(&deck_id) {
            if let Some(ref mut renderer) = deck.renderer {
                if renderer.is_running() {
                    if renderer.send_command(&RendererCommand::SetTexturePaths {
                        paths: paths.clone(),
                    }).is_ok() {
                        count += 1;
                    }
                }
            }
        }
    }

    Ok(format!("Updated texture paths on {} running decks", count))
}

// ============ MIDI Commands ============

/// MIDI status info for frontend
#[derive(Serialize, Deserialize)]
pub struct MidiStatus {
    pub connected: bool,
    pub learning: bool,
    pub port_name: Option<String>,
    pub mapping_count: usize,
}

/// MIDI mapping info for frontend
#[derive(Serialize, Deserialize)]
pub struct MidiMappingInfo {
    pub id: String,
    pub name: String,
    pub midi_type: String,
    pub action: String,
    pub enabled: bool,
}

impl From<&MidiMapping> for MidiMappingInfo {
    fn from(m: &MidiMapping) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name.clone(),
            midi_type: format!("{:?}", m.midi_message),
            action: format!("{:?}", m.action),
            enabled: m.enabled,
        }
    }
}

/// MIDI preset info for frontend
#[derive(Serialize, Deserialize)]
pub struct MidiPresetInfo {
    pub name: String,
    pub description: String,
    pub controller: String,
    pub mapping_count: usize,
}

/// List available MIDI input ports
#[tauri::command]
fn list_midi_ports() -> Result<Vec<MidiPortInfo>, String> {
    core_list_midi_ports().map_err(|e| e.to_string())
}

/// Connect to a MIDI input port
#[tauri::command]
fn midi_connect(state: State<'_, AppState>, port_index: usize) -> Result<String, String> {
    let mut midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    midi_guard.connect(port_index).map_err(|e| e.to_string())?;
    Ok(format!("Connected to MIDI port {}", port_index))
}

/// Disconnect from MIDI
#[tauri::command]
fn midi_disconnect(state: State<'_, AppState>) -> Result<String, String> {
    let mut midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    midi_guard.disconnect();
    Ok("Disconnected from MIDI".to_string())
}

/// Get MIDI status
#[tauri::command]
fn midi_get_status(state: State<'_, AppState>) -> Result<MidiStatus, String> {
    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    Ok(MidiStatus {
        connected: midi_guard.is_connected(),
        learning: midi_guard.is_learning(),
        port_name: midi_guard.connected_port_name().map(String::from),
        mapping_count: midi_guard.get_mappings().len(),
    })
}

/// Get all MIDI mappings
#[tauri::command]
fn midi_get_mappings(state: State<'_, AppState>) -> Result<Vec<MidiMappingInfo>, String> {
    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    Ok(midi_guard
        .get_mappings()
        .iter()
        .map(MidiMappingInfo::from)
        .collect())
}

/// Add a MIDI mapping manually
#[tauri::command]
fn midi_add_mapping(
    state: State<'_, AppState>,
    name: String,
    channel: u8,
    controller: u8,
    action: String,
    deck_id: Option<u8>,
) -> Result<String, String> {
    let midi_message = MidiMessageType::ControlChange { channel, controller };

    // Parse action string to MidiAction
    let deck = deck_id.unwrap_or(0);
    let action = match action.to_lowercase().as_str() {
        "deck_volume" => MidiAction::DeckVolume(deck),
        "deck_start" => MidiAction::DeckStart(deck),
        "deck_stop" => MidiAction::DeckStop(deck),
        "deck_toggle" => MidiAction::DeckToggle(deck),
        "next_preset" => MidiAction::NextPreset(deck),
        "previous_preset" => MidiAction::PreviousPreset(deck),
        "random_preset" => MidiAction::RandomPreset(deck),
        "crossfader" | "crossfader_position" => MidiAction::CrossfaderPosition,
        "beat_sensitivity" => MidiAction::DeckBeatSensitivity(deck),
        _ => return Err(format!("Unknown action: {}", action)),
    };

    let mapping = MidiMapping::new(name, midi_message, action);
    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    midi_guard.add_mapping(mapping);

    Ok("Mapping added".to_string())
}

/// Remove a MIDI mapping by ID
#[tauri::command]
fn midi_remove_mapping(state: State<'_, AppState>, mapping_id: String) -> Result<String, String> {
    let id = uuid::Uuid::parse_str(&mapping_id).map_err(|e| e.to_string())?;
    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;

    if midi_guard.remove_mapping(id) {
        Ok("Mapping removed".to_string())
    } else {
        Err("Mapping not found".to_string())
    }
}

/// Clear all MIDI mappings
#[tauri::command]
fn midi_clear_mappings(state: State<'_, AppState>) -> Result<String, String> {
    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    midi_guard.clear_mappings();
    Ok("All mappings cleared".to_string())
}

/// Start MIDI learn mode for an action
#[tauri::command]
fn midi_start_learn(
    state: State<'_, AppState>,
    action: String,
    name: String,
    deck_id: Option<u8>,
) -> Result<String, String> {
    let deck = deck_id.unwrap_or(0);
    let midi_action = match action.to_lowercase().as_str() {
        "deck_volume" => MidiAction::DeckVolume(deck),
        "deck_start" => MidiAction::DeckStart(deck),
        "deck_stop" => MidiAction::DeckStop(deck),
        "deck_toggle" => MidiAction::DeckToggle(deck),
        "next_preset" => MidiAction::NextPreset(deck),
        "previous_preset" => MidiAction::PreviousPreset(deck),
        "random_preset" => MidiAction::RandomPreset(deck),
        "crossfader" | "crossfader_position" => MidiAction::CrossfaderPosition,
        "beat_sensitivity" => MidiAction::DeckBeatSensitivity(deck),
        "playlist_next" => MidiAction::PlaylistNext(deck),
        "playlist_previous" => MidiAction::PlaylistPrevious(deck),
        "shuffle_toggle" => MidiAction::PlaylistToggleShuffle(deck),
        "auto_cycle_toggle" => MidiAction::PlaylistToggleAutoCycle(deck),
        _ => return Err(format!("Unknown action: {}", action)),
    };

    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    midi_guard.start_learn_mode(midi_action, name);
    Ok("Learn mode started - move a MIDI control".to_string())
}

/// Cancel MIDI learn mode
#[tauri::command]
fn midi_cancel_learn(state: State<'_, AppState>) -> Result<String, String> {
    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    midi_guard.cancel_learn_mode();
    Ok("Learn mode cancelled".to_string())
}

/// List available built-in MIDI presets
#[tauri::command]
fn midi_list_builtin_presets() -> Vec<MidiPresetInfo> {
    vec![
        MidiPresetInfo {
            name: "Generic DJ Controller".to_string(),
            description: "Basic mapping for 2-deck DJ controllers".to_string(),
            controller: "Generic".to_string(),
            mapping_count: 11,
        },
        MidiPresetInfo {
            name: "Akai APC Mini".to_string(),
            description: "Mapping for Akai APC Mini controller".to_string(),
            controller: "Akai APC Mini".to_string(),
            mapping_count: 21,
        },
        MidiPresetInfo {
            name: "Novation Launchpad".to_string(),
            description: "Mapping for Novation Launchpad".to_string(),
            controller: "Novation Launchpad".to_string(),
            mapping_count: 16,
        },
        MidiPresetInfo {
            name: "Korg nanoKONTROL2".to_string(),
            description: "Mapping for Korg nanoKONTROL2".to_string(),
            controller: "Korg nanoKONTROL2".to_string(),
            mapping_count: 21,
        },
    ]
}

/// Load a built-in MIDI preset
#[tauri::command]
fn midi_load_builtin_preset(
    state: State<'_, AppState>,
    preset_name: String,
) -> Result<String, String> {
    let preset = match preset_name.to_lowercase().as_str() {
        "generic" | "generic dj controller" => create_generic_dj_preset(),
        "akai" | "akai apc mini" | "apc mini" => create_apc_mini_preset(),
        "launchpad" | "novation launchpad" => create_launchpad_preset(),
        "nanokontrol" | "nanokontrol2" | "korg nanokontrol2" => create_nanokontrol2_preset(),
        _ => return Err(format!("Unknown preset: {}", preset_name)),
    };

    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    midi_guard.load_mappings(preset.mappings);

    Ok(format!("Loaded preset: {}", preset.name))
}

/// Save current mappings to a JSON file
#[tauri::command]
fn midi_save_preset(
    state: State<'_, AppState>,
    name: String,
    path: String,
) -> Result<String, String> {
    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    let mappings = midi_guard.get_mappings();

    let preset = MidiPreset {
        name: name.clone(),
        description: String::new(),
        controller: "Custom".to_string(),
        mappings,
    };

    preset.save(&path).map_err(|e| e.to_string())?;
    Ok(format!("Saved preset '{}' to {}", name, path))
}

/// Load mappings from a JSON file
#[tauri::command]
fn midi_load_preset_file(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let preset = MidiPreset::load(&path).map_err(|e| e.to_string())?;
    let name = preset.name.clone();
    let count = preset.mappings.len();

    let midi_guard = state.midi_controller.lock().map_err(|e| e.to_string())?;
    midi_guard.load_mappings(preset.mappings);

    Ok(format!("Loaded preset '{}' with {} mappings", name, count))
}

// ============ Backward Compatibility Commands ============
// These wrap the new deck commands for existing frontend

/// Start visualizer (backward compatible - uses deck 0)
#[tauri::command]
fn start_visualizer(
    state: State<'_, AppState>,
    width: Option<u32>,
    height: Option<u32>,
    fullscreen: Option<bool>,
    preset_path: Option<String>,
) -> Result<String, String> {
    start_deck(state, Some(0), width, height, fullscreen, preset_path, None)
}

/// Stop visualizer (backward compatible - uses deck 0)
#[tauri::command]
fn stop_visualizer(state: State<'_, AppState>) -> Result<String, String> {
    stop_deck(state, Some(0))
}

/// Greet command (for testing)
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to OpenDrop!", name)
}

// ============ App Entry Point ============

/// Configure WebKitGTK workarounds for Linux graphics compatibility.
///
/// Addresses EGL/DMA-BUF issues causing white screens on:
/// - AppImage builds with bundled WebKitGTK on Wayland
/// - Intel i915 graphics with newer Mesa versions
///
/// References:
/// - https://github.com/tauri-apps/tauri/issues/9394
/// - https://bugs.webkit.org/show_bug.cgi?id=202362
#[cfg(target_os = "linux")]
fn configure_linux_webkit_workarounds() {
    use std::env;

    // Check if running as AppImage
    let is_appimage = env::var("APPIMAGE").is_ok() || env::var("APPDIR").is_ok();

    if is_appimage {
        // Disable DMA-BUF renderer to prevent EGL_BAD_PARAMETER errors
        // when bundled WebKitGTK conflicts with host EGL/Mesa stack
        if env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            eprintln!("[OpenDrop] AppImage detected: Setting WEBKIT_DISABLE_DMABUF_RENDERER=1");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Fix WebKitGTK EGL issues on Linux AppImage
    #[cfg(target_os = "linux")]
    configure_linux_webkit_workarounds();

    tracing_subscriber::fmt()
        .with_env_filter("opendrop=debug,opendrop_core=debug,projectm_rs=debug")
        .init();

    info!("Starting OpenDrop v{}", env!("CARGO_PKG_VERSION"));
    info!("ProjectM version: {}", projectm_rs::ProjectM::version());
    info!("Multi-deck mode: {} decks available", MAX_DECKS);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            greet,
            // Multi-deck commands
            start_deck,
            stop_deck,
            set_deck_volume,
            get_multi_deck_status,
            // Per-deck commands with deck_id parameter
            load_preset,
            set_beat_sensitivity,
            toggle_fullscreen,
            // Playlist commands
            playlist_add,
            playlist_remove,
            playlist_clear,
            playlist_next,
            playlist_previous,
            playlist_set_settings,
            playlist_jump_to,
            playlist_reorder,
            // Crossfader commands
            crossfader_set_position,
            crossfader_set_enabled,
            crossfader_set_curve,
            crossfader_assign_deck,
            crossfader_get_config,
            // Compositor commands
            compositor_set_enabled,
            compositor_set_resolution,
            compositor_set_deck_opacity,
            compositor_set_deck_blend_mode,
            compositor_set_deck_layer,
            compositor_set_deck_enabled,
            compositor_link_crossfader,
            compositor_get_config,
            // Audio commands
            list_audio_devices,
            start_audio,
            stop_audio,
            pump_audio,
            get_audio_levels,
            // Utility commands
            get_status,
            get_projectm_version,
            list_presets,
            get_preset_directories,
            get_texture_directories,
            // Preset import/export commands
            import_presets_from_folder,
            export_playlist,
            import_playlist,
            // Video output commands
            list_video_outputs,
            set_deck_video_output,
            // NDI output commands
            is_ndi_available,
            set_deck_ndi_output,
            // Texture path commands
            set_deck_texture_paths,
            set_all_decks_texture_paths,
            // Monitor commands
            list_monitors,
            // MIDI commands
            list_midi_ports,
            midi_connect,
            midi_disconnect,
            midi_get_status,
            midi_get_mappings,
            midi_add_mapping,
            midi_remove_mapping,
            midi_clear_mappings,
            midi_start_learn,
            midi_cancel_learn,
            midi_list_builtin_presets,
            midi_load_builtin_preset,
            midi_save_preset,
            midi_load_preset_file,
            // Backward compatibility
            start_visualizer,
            stop_visualizer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
