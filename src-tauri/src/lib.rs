//! OpenDrop Tauri backend
//!
//! Multi-deck visualization controller supporting up to 4 simultaneous decks.

use std::collections::HashMap;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

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
    health: RendererHealth,
    started_at: std::time::Instant,
    crash_count: u32,
}

impl RendererProcess {
    fn new(child: Child) -> Self {
        Self {
            child,
            running: true,
            health: RendererHealth::Starting,
            started_at: std::time::Instant::now(),
            crash_count: 0,
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
                if exit_status.success() {
                    self.health = RendererHealth::Stopped;
                } else {
                    self.health = RendererHealth::Crashed;
                    self.crash_count += 1;
                    warn!("Renderer crashed with status: {:?}", exit_status);
                }
                false
            }
            Ok(None) => {
                // Process still running
                if self.health == RendererHealth::Starting {
                    // Check if it's been running long enough to consider healthy
                    if self.started_at.elapsed() > std::time::Duration::from_secs(2) {
                        self.health = RendererHealth::Running;
                    }
                }
                true
            }
            Err(e) => {
                warn!("Error checking renderer status: {}", e);
                self.running = false;
                self.health = RendererHealth::Crashed;
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
            self.health = RendererHealth::Stopped;
        }
    }

    fn get_health(&self) -> RendererHealth {
        self.health
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
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos().hash(&mut hasher);
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
    pub is_default: bool,
}

impl From<DeviceInfo> for AudioDeviceInfo {
    fn from(d: DeviceInfo) -> Self {
        Self {
            name: d.name,
            is_default: d.is_default,
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

    // Use a default preset if none specified
    let preset = preset_path.or_else(|| {
        std::fs::read_dir("/usr/share/projectM/presets/presets_milkdrop_104")
            .ok()
            .and_then(|mut dir| {
                dir.find_map(|entry| {
                    entry.ok().and_then(|e| {
                        let path = e.path();
                        if path.extension().is_some_and(|ext| ext == "milk") {
                            Some(path.to_string_lossy().to_string())
                        } else {
                            None
                        }
                    })
                })
            })
    });

    // Build config
    let config = RendererConfig {
        width: width.unwrap_or(1280),
        height: height.unwrap_or(720),
        preset_path: preset.clone(),
        fullscreen: fullscreen.unwrap_or(false),
        deck_id,
        monitor_index,
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

/// Find the renderer executable
fn find_renderer_executable() -> Result<String, String> {
    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("opendrop-renderer")))
            .map(|p| p.to_string_lossy().to_string()),
        Some("/srv/http/opendrop/target/debug/opendrop-renderer".to_string()),
        Some("/srv/http/opendrop/target/release/opendrop-renderer".to_string()),
        Some("/usr/bin/opendrop-renderer".to_string()),
        Some("/usr/local/bin/opendrop-renderer".to_string()),
    ];

    for candidate in candidates.into_iter().flatten() {
        if std::path::Path::new(&candidate).exists() {
            return Ok(candidate);
        }
    }

    Err("Could not find opendrop-renderer executable".to_string())
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
        preset_dir: "/usr/share/projectM/presets".to_string(),
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
        preset_dir: "/usr/share/projectM/presets".to_string(),
    })
}

/// Get projectM version
#[tauri::command]
fn get_projectm_version() -> String {
    projectm_rs::ProjectM::version()
}

/// List presets in a directory
#[tauri::command]
fn list_presets(dir: Option<String>) -> Result<Vec<PresetInfo>, String> {
    let dir = dir.unwrap_or_else(|| "/usr/share/projectM/presets".to_string());

    let mut presets = Vec::new();

    fn scan_dir(path: &std::path::Path, presets: &mut Vec<PresetInfo>, depth: usize) {
        if depth > 3 {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    scan_dir(&path, presets, depth + 1);
                } else if path.extension().is_some_and(|ext| ext == "milk") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        presets.push(PresetInfo {
                            name: name.to_string(),
                            path: path.to_string_lossy().to_string(),
                        });
                    }
                }
            }
        }
    }

    scan_dir(std::path::Path::new(&dir), &mut presets, 0);
    presets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    presets.truncate(500);

    Ok(presets)
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
        // On Windows, use winapi to enumerate monitors
        // For now, provide a simple fallback
        monitors.push(MonitorInfo {
            index: 0,
            name: "Primary".to_string(),
            width: 1920,
            height: 1080,
            is_primary: true,
        });
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
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
        port_name: None, // TODO: track connected port name
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter("opendrop=debug,opendrop_core=debug,projectm_rs=debug")
        .init();

    info!("Starting OpenDrop v{}", env!("CARGO_PKG_VERSION"));
    info!("ProjectM version: {}", projectm_rs::ProjectM::version());
    info!("Multi-deck mode: {} decks available", MAX_DECKS);

    tauri::Builder::default()
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
            // Video output commands
            list_video_outputs,
            set_deck_video_output,
            // NDI output commands
            is_ndi_available,
            set_deck_ndi_output,
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
