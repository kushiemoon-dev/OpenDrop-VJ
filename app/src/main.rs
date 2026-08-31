use glow::HasContext;
use glutin::config::{Api, Config, ConfigSurfaceTypes, ConfigTemplateBuilder};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, PossiblyCurrentContext, Version};
use glutin::display::{Display, GetGlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::DisplayBuilder;
use opendrop_core::blend::{should_force_normal_for_lowest_slot, DEFAULT_COLOR_PARAMS, DEFAULT_SLOT_COMPOSITE};
use opendrop_core::commands::{create_default_registry, CommandContext, CommandId, CommandKind, CommandRegistry};
use opendrop_core::show::{DeckBus, Show};
use opendrop_core::thumb_queue::ThumbJob;
use opendrop_engine::compositor::{Compositor, LayerInput, COMP_H, COMP_W};
use opendrop_engine::deck::{self, Deck};
use opendrop_engine::readback::FrameReadback;
use opendrop_engine::timing::PassTimer;
use opendrop_io::midi::MidiTriggerKey;
use raw_window_handle::HasWindowHandle;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsString;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

mod config;
mod egl_headless;
mod keymap;
mod preflight;
mod thumbnail_child;
mod thumbnails;
mod theme;
mod ui;

/// ponytail: paced off the control window's monitor only, read once at
/// bootstrap. A VJ setup can have control and output on different-refresh
/// monitors; revisit if that ever causes visible judder on the output side.
const FALLBACK_REFRESH_MILLIHERTZ: u32 = 60_000;

/// Culled (opacity ≤ 0.001) decks still render, just at this much lower
/// rate: not stopped outright: so a deck doesn't show a visible cold
/// start (projectM's per-preset warm-up/transition state going stale) the
/// moment the crossfader brings it back in. This is the `Eco` invisible-mode
/// throttle (Step 20): `Pause` skips rendering the deck entirely instead,
/// and `Off` ignores this constant altogether.
const IDLE_DECK_INTERVAL: Duration = Duration::from_millis(100); // ~10fps floor

/// How long a MIDI-triggered command's LED confirmation flash stays on
/// before `about_to_wait` pushes it back off: mirrors the JS reference's
/// `setTimeout(..., 120)` (`midi-connection-actions.ts:90`), but as a
/// per-frame `Instant` deadline check instead of an async timer, since this
/// thread has no async runtime (Task 8).
const MIDI_LED_FLASH_DURATION: Duration = Duration::from_millis(120);

/// Whole-branch review Finding 3: clamp applied to `compute_tick_dt`'s
/// real elapsed-time measurement, matching the TS reference's
/// `Math.min(dt, 0.1)` (`clock.ts:53`).
const MAX_TICK_DT: Duration = Duration::from_millis(100);

/// Max messages kept in `AppState::chat_log` (whole-branch review Finding
/// 2): a bounded ring buffer, oldest dropped first, so the Streaming
/// panel's chat display can't grow unbounded over a long-running session.
const CHAT_LOG_CAP: usize = 50;

/// Power mode applied to invisible (opacity ≤ 0.001) decks: selected from
/// the Quality panel (Step 20). `Eco` reproduces the original always-on
/// behavior (throttled to `IDLE_DECK_INTERVAL`, so the texture stays warm
/// for a fast comeback); `Pause` skips rendering the deck entirely while
/// invisible, so its texture keeps showing whatever frame it last rendered;
/// `Off` disables the throttle, rendering invisible decks at full rate same
/// as visible ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvisibleMode {
    Eco,
    Pause,
    Off,
}

/// Per-slot compositor input driven by the live show state: opacity from
/// `bus_gain(deck_bus[slot], crossfader)`, composite config directly from
/// `slot_composites`, and color params from whichever bus (A/B) that slot is
/// currently assigned to: `Off` slots get the default (harmless, since
/// their opacity is 0 and composite_layer skips them at the 0.001 floor).
fn layer_inputs_from_show(show: &Show) -> [LayerInput; 4] {
    let opacities = show.slot_opacities();
    std::array::from_fn(|i| {
        let color = match show.deck_bus[i] {
            DeckBus::A => show.color_params_a,
            DeckBus::B => show.color_params_b,
            DeckBus::Off => DEFAULT_COLOR_PARAMS,
        };
        LayerInput { opacity: opacities[i] as f32, composite: show.slot_composites[i], color }
    })
}

/// Creates a new GL texture sized/uploaded from `frame`'s own dimensions
/// and bytes, storing it (with its size) into `slot`. Used by the NDI-in
/// poll in `about_to_wait` both when no texture exists yet and when the
/// source's resolution changed (the caller deletes the old texture first in
/// that case): NDI-in sources can be any resolution, so this never assumes
/// a fixed size the way the compositor/deck textures do.
fn create_ndi_in_texture(
    gl: &glow::Context,
    frame: &opendrop_io::ndi::NdiFrame,
    slot: &mut Option<(glow::NativeTexture, u32, u32)>,
) {
    unsafe {
        let tex = gl.create_texture().expect("glGenTextures failed for the NDI-in texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            frame.width as i32,
            frame.height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&frame.data)),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        *slot = Some((tex, frame.width, frame.height));
    }
}

/// Which top-level panel the control window is currently showing: gates
/// per-tick work that only matters while its panel is visible (Step 17: the
/// thumbnail pump only runs while `PresetBrowser` is on screen), besides
/// driving `ui_root`'s own tab row.
// `pub(crate)`: read by `ui::ctx::ShellCtx` (Step 9 of the Phase 7 UI
// redesign plan), a different module from this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Panel {
    #[default]
    Decks,
    PresetBrowser,
    Playlists,
    Audio,
    Quality,
    Output,
    Midi,
    // Step 9: split from a single `Ndi` variant. `ndi.rs`'s `show` itself
    // is unchanged (it already renders both the output toggles and the
    // input selector in one call): both variants currently drive that
    // same call from `ui_root`'s match. Step 10 gave this variant its own
    // nav button (SOURCES section, `ui::shell::nav`): a future step still
    // owes it its own half of `ndi.rs`.
    NdiIn,
    NdiOut,
    Osc,
    RemoteWs,
    Streaming,
    #[cfg(feature = "link")]
    Link,
    V4l2,
    About,
}

/// Whole-branch review fix wave, finding 1 (AC-10): `config::PanelId` ->
/// `Panel`, for restoring `AppState::active_panel` from a loaded `ui.json`
/// at bootstrap. Kept as an explicit conversion rather than merging the
/// two enums, per `config.rs`'s own module doc comment (`Panel` must not
/// derive `Serialize`/`Deserialize`).
impl From<config::PanelId> for Panel {
    fn from(id: config::PanelId) -> Self {
        match id {
            config::PanelId::Decks => Panel::Decks,
            config::PanelId::PresetBrowser => Panel::PresetBrowser,
            config::PanelId::Playlists => Panel::Playlists,
            config::PanelId::Audio => Panel::Audio,
            config::PanelId::Quality => Panel::Quality,
            config::PanelId::Output => Panel::Output,
            config::PanelId::Midi => Panel::Midi,
            config::PanelId::NdiIn => Panel::NdiIn,
            config::PanelId::NdiOut => Panel::NdiOut,
            config::PanelId::Osc => Panel::Osc,
            config::PanelId::RemoteWs => Panel::RemoteWs,
            config::PanelId::Streaming => Panel::Streaming,
            #[cfg(feature = "link")]
            config::PanelId::Link => Panel::Link,
            config::PanelId::V4l2 => Panel::V4l2,
            config::PanelId::About => Panel::About,
        }
    }
}

/// The reverse of the above, for saving `AppState::active_panel` into
/// `UiConfig` on exit.
impl From<Panel> for config::PanelId {
    fn from(panel: Panel) -> Self {
        match panel {
            Panel::Decks => config::PanelId::Decks,
            Panel::PresetBrowser => config::PanelId::PresetBrowser,
            Panel::Playlists => config::PanelId::Playlists,
            Panel::Audio => config::PanelId::Audio,
            Panel::Quality => config::PanelId::Quality,
            Panel::Output => config::PanelId::Output,
            Panel::Midi => config::PanelId::Midi,
            Panel::NdiIn => config::PanelId::NdiIn,
            Panel::NdiOut => config::PanelId::NdiOut,
            Panel::Osc => config::PanelId::Osc,
            Panel::RemoteWs => config::PanelId::RemoteWs,
            Panel::Streaming => config::PanelId::Streaming,
            #[cfg(feature = "link")]
            Panel::Link => config::PanelId::Link,
            Panel::V4l2 => config::PanelId::V4l2,
            Panel::About => config::PanelId::About,
        }
    }
}

struct WindowSlot {
    window: Window,
    surface: Surface<WindowSurface>,
    size: (u32, u32),
    occluded: bool,
}

impl WindowSlot {
    /// Makes `ctx` current against this slot's surface and resets the
    /// viewport: glViewport does not re-derive from the surface on its own,
    /// so every switch between the two windows' surfaces must redo this.
    fn make_current_and_size_viewport(&self, ctx: &PossiblyCurrentContext, gl: &glow::Context) -> Result<(), String> {
        ctx.make_current(&self.surface).map_err(|e| format!("make_current failed: {e}"))?;
        unsafe { gl.viewport(0, 0, self.size.0 as i32, self.size.1 as i32) };
        Ok(())
    }

    fn render_and_swap(
        &self,
        ctx: &PossiblyCurrentContext,
        gl: &glow::Context,
        compositor: &Compositor,
        blit_timer: &mut PassTimer,
    ) -> Result<(), String> {
        if self.occluded {
            return Ok(());
        }
        self.make_current_and_size_viewport(ctx, gl)?;
        blit_timer.begin(gl);
        compositor.blit_to_current_window(gl, self.size.0 as i32, self.size.1 as i32);
        blit_timer.end(gl);
        self.window.pre_present_notify();
        self.surface.swap_buffers(ctx).map_err(|e| format!("swap_buffers failed: {e}"))
    }

    /// Same as `render_and_swap`, but paints the egui overlay on top of the
    /// composite before swapping. Used only for `control`: `output` never
    /// carries UI, so a shared method taking `Option<&mut EguiGlow>` would
    /// force it to pass `None` for nothing.
    fn render_and_swap_with_egui(
        &self,
        ctx: &PossiblyCurrentContext,
        gl: &glow::Context,
        compositor: &Compositor,
        blit_timer: &mut PassTimer,
        egui_glow: &mut egui_glow::EguiGlow,
    ) -> Result<(), String> {
        if self.occluded {
            return Ok(());
        }
        self.make_current_and_size_viewport(ctx, gl)?;
        blit_timer.begin(gl);
        compositor.blit_to_current_window(gl, self.size.0 as i32, self.size.1 as i32);
        blit_timer.end(gl);
        egui_glow.paint(&self.window); // after the blit, before the swap: draws over the composite
        self.window.pre_present_notify();
        self.surface.swap_buffers(ctx).map_err(|e| format!("swap_buffers failed: {e}"))
    }
}

struct AppState {
    #[allow(dead_code)] // kept alive: dropping Display would invalidate every surface/context above
    display: Display,
    main_ctx: PossiblyCurrentContext,
    control: WindowSlot,
    output: WindowSlot,
    decks: Vec<Deck>,
    compositor: Compositor,
    /// GPU->CPU async pixel readback (Step 4's `FrameReadback`) for the
    /// composite output and each deck's live texture: driven once per
    /// frame in `about_to_wait`. Composite is gated on `ndi_composite_active
    /// || v4l2_active` (either consumer needs the composite), each deck on
    /// that deck's own `ndi_deck_active[i]` only (v4l2 never needs per-deck
    /// readbacks): whole-branch review Finding I5: a single combined
    /// `ndi_active || v4l2_active` gate over all 5 readbacks used to make a
    /// v4l2-only session pay for 4 wasted deck `glReadPixels`+`to_vec()`
    /// calls every frame, and symmetrically made an NDI-composite-only
    /// session pay for v4l2's extra `bytes.clone()`.
    compositor_readback: FrameReadback,
    deck_readback: [FrameReadback; deck::DECK_COUNT],
    /// Outlet for `compositor_readback`'s polled RGBA bytes, feeding the NDI
    /// output thread's `compositor_rx` (its `Receiver` end was moved into
    /// `opendrop_io::ndi::spawn` at construction time: see `ndi`'s doc
    /// comment).
    compositor_frame_tx: mpsc::Sender<Vec<u8>>,
    /// Same compositor bytes as `compositor_frame_tx`, on a second channel
    /// pair feeding the v4l2loopback output thread's own `compositor_rx`
    /// (Task 19): each consumer needs its own `Sender`, since Task 9/10
    /// already moved the original single receiver into `opendrop_io::
    /// ndi::spawn`. Unlike `compositor_frame_tx`/`deck_frame_tx`, there is
    /// no per-deck counterpart: v4l2loopback only ever pipes the composite
    /// stream (see `io::v4l2loopback`'s module doc comment).
    v4l2_frame_tx: mpsc::Sender<Vec<u8>>,
    /// Same as `compositor_frame_tx`, one channel pair per deck.
    deck_frame_tx: [mpsc::Sender<Vec<u8>>; deck::DECK_COUNT],
    /// Handle to the dedicated NDI output thread (Task 9): owns the
    /// `Receiver` ends of `compositor_frame_tx`/`deck_frame_tx` (handed to
    /// `opendrop_io::ndi::spawn` once, at construction time), and takes
    /// start/stop control messages from the NDI panel (Task 10).
    ndi: opendrop_io::ndi::NdiHandle,
    /// The NDI panel's own composite toggle state (resynced from
    /// `NdiSnapshot::composite_active` each frame the panel is drawn: see
    /// `ui::ndi`'s doc comment, whole-branch review Finding M5). Directly
    /// gates the composite readback in `about_to_wait` (Finding I5):
    /// there's no separate recomputed aggregate any more: the previous
    /// `ndi_active` field, recomputed *after* the readback gate already
    /// consumed it, was one frame stale by construction (whole-branch
    /// review Finding M1); reading this field directly removes that extra
    /// staleness.
    ndi_composite_active: bool,
    /// Same as `ndi_composite_active`, one per deck: each element directly
    /// gates that deck's own readback (Finding I5).
    ndi_deck_active: [bool; deck::DECK_COUNT],
    /// GL texture holding the most recently received NDI-in frame (Task
    /// 12), composited as the topmost layer over the 4 decks: `None`
    /// until the first frame of an active receive arrives. Carries its own
    /// `(width, height)` alongside the texture handle so a same-size frame
    /// can `tex_sub_image_2d` in place, while a resolution change (or a
    /// freshly connected source) recreates it: NDI-in sources can be any
    /// resolution, unlike the fixed-size compositor/deck textures.
    ndi_in_texture: Option<(glow::NativeTexture, u32, u32)>,
    /// Source currently selected in the NDI panel's dropdown (Task 12),
    /// same convention as `ndi_composite_active`: this is the panel's own
    /// toggle state, not `NdiSnapshot::receive_active` (which reflects
    /// whether the receiver actually started).
    ndi_in_selected_source: Option<opendrop_io::ndi::NdiSource>,
    /// Whether a v4l2loopback output consumer is active: this panel's own
    /// Start/Stop toggle (Task 19), resynced from `V4l2Snapshot::running`
    /// each frame the panel is drawn (`ui::v4l2loopback::show`, whole-branch
    /// review Finding M5): no per-slot array to OR together, just the one
    /// stream, so no separate aggregate field is needed. Gates the
    /// composite readback in `about_to_wait` alongside `ndi_composite_active`
    /// (Finding I5).
    v4l2_active: bool,
    /// Handle to the dedicated v4l2loopback output thread (Task 19): owns
    /// the `Receiver` end of `v4l2_frame_tx` (handed to `opendrop_io::
    /// v4l2loopback::spawn` once, at construction time), and takes
    /// start/stop control messages from the v4l2loopback panel.
    v4l2: opendrop_io::v4l2loopback::V4l2Handle,
    /// Lazily-resolved v4l2loopback device path, checked at most once per
    /// session: outer `None` means "not checked yet", `Some(None)` means
    /// "checked, no device found". Populated by `ui::v4l2loopback::show`
    /// the first frame that panel is shown: see that module's doc comment
    /// for why this is deliberately not re-queried per frame.
    v4l2_device: Option<Option<PathBuf>>,
    gl: Arc<glow::Context>,
    egui_glow: egui_glow::EguiGlow,
    refresh_interval: Duration,
    next_frame_at: Instant,
    /// Reference instant captured once at bootstrap. Every `now_ms` fed to
    /// `core`'s beat-sync engine (`BeatDetector::process_sample`,
    /// `Show::tap_tempo`, `Show::check_volume_peak_triggers`) is derived from
    /// `t0.elapsed()`, never from `next_frame_at`. Step 18: the pacing clock
    /// and the beat-sync timestamp are deliberately different clocks.
    t0: Instant,
    /// Handle to the dedicated audio capture thread: `latest()` gives the
    /// latest PCM chunk + energy, read once per tick and shared by every
    /// deck due that tick.
    audio: opendrop_audio::AudioHandle,
    /// Labels of every available input device, enumerated once at bootstrap
    /// via `opendrop_audio::list_input_devices()` and cached here for the
    /// Audio panel's dropdown (Step 19). The brief is explicit that the
    /// device list doesn't change mid-session, so this is never re-scanned
    /// per frame.
    input_devices: Vec<String>,
    /// Name of the input device currently selected in the Audio panel's
    /// dropdown, if the user has picked one explicitly via `AudioHandle::
    /// set_device`. `None` means still on whichever device `spawn_capture`
    /// chose by default (`opendrop_audio::device::select_input_device`).
    selected_input_device: Option<String>,
    /// RMS of the current tick's PCM snapshot (`opendrop_audio::analysis::
    /// vu_level`), computed once per tick by the beat-sync engine wiring and
    /// stored here so a later panel (Step 19's VU meter) can read it without
    /// a second `vu_level` pass over the same PCM.
    last_vu_level: f64,
    /// Per-deck throttle for culled (invisible) decks: see IDLE_DECK_INTERVAL.
    deck_next_render_at: [Instant; deck::DECK_COUNT],
    /// Power mode applied to invisible decks: see `InvisibleMode`. Written
    /// by the Quality panel, read each tick by the per-deck render loop.
    invisible_mode: InvisibleMode,
    /// Mesh-size change requested from the Quality panel's per-deck preset
    /// buttons. Drained (and applied via `Deck::set_mesh_size`) at the point
    /// in the per-deck loop where that deck's context is already current:
    /// never call `set_mesh_size` outside a current context.
    pending_mesh_size: [Option<(usize, usize)>; deck::DECK_COUNT],
    show: Show,
    registry: CommandRegistry,
    keymap: HashMap<Key, CommandId>,
    blit_control_timer: PassTimer,
    blit_output_timer: PassTimer,
    /// Wall-clock instant the output window's surface was last swapped, or
    /// `None` before the first tick. Doubles as `compute_tick_dt`'s
    /// `last_tick_at`, read near the top of the same gated tick that
    /// overwrites it near the bottom (Finding 3): the two ends of one
    /// tick are close enough together that this is a faithful "previous
    /// tick's real elapsed-time reference", without a second field.
    last_output_swap_at: Option<Instant>,
    /// Wall-clock swap-to-swap frame time in ms, mirrored from the
    /// `wall_ms` local computed right after each render (see that call
    /// site, just below `last_output_swap_at`'s own write): one frame
    /// stale by construction, same as `last_output_swap_at` itself.
    /// `None` before the first tick. Feeds the status bar's fps/frame-ms
    /// readout (Step 10 of the Phase 7 UI redesign plan).
    last_wall_ms: Option<f64>,
    perf_tick: u64,
    /// Sender handed to `preflight::spawn_preflight`: cloned once per
    /// `request_preset_load` call so each validation request gets its own
    /// handle back to `preflight_rx`.
    preflight_tx: mpsc::Sender<(usize, String, preflight::PreflightVerdict)>,
    preflight_rx: mpsc::Receiver<(usize, String, preflight::PreflightVerdict)>,
    /// Name→path lookup for the full preset catalog (`show.preset_catalog`
    /// carries the name+category metadata; resolving a selected name back
    /// to a file for loading is app-side, since it's filesystem-backed).
    path_by_name: HashMap<String, PathBuf>,
    /// Slots currently awaiting a preflight verdict: the UI shows
    /// "validating…" on a deck-card while its slot is in here.
    pending_validations: HashSet<usize>,
    /// Most recent load failure per slot (preflight rejection, or a GL/load
    /// error on an otherwise-passed preset): cleared on that slot's next
    /// successful load.
    preset_errors: HashMap<usize, String>,
    /// Name of the preset currently loaded on each slot, for display on the
    /// deck-card.
    deck_preset_names: [String; 4],
    /// Soft-cut transition duration applied to every load routed through
    /// `request_preset_load`: one global setting, not per-slot (see Step 16).
    transition_seconds: f64,
    /// `egui::TextureId` for each deck's live GPU texture, registered once
    /// at bootstrap via `painter.register_native_texture`: never
    /// re-registered per frame, which would leak a texture handle in
    /// egui_glow's painter every tick.
    deck_tex_ids: [egui::TextureId; 4],
    /// Which panel the control window currently shows: see `Panel`.
    active_panel: Panel,
    /// Header's Stage toggle (Step 10 of the Phase 7 UI redesign plan),
    /// wired to the Normal/Stage shell switch at Step 11.
    stage_mode: bool,
    /// Whether the Stage bottom bar's collapsible preset drawer is open
    /// (Step 11): independent of `stage_mode` itself, so leaving Stage
    /// mode doesn't need to also reset it.
    presets_drawer_open: bool,
    /// Live text in the preset-browser search box.
    preset_search_query: String,
    /// Memoized `search` results for `preset_search_query`: see
    /// `ui::preset_browser::SearchCache`.
    preset_search_cache: ui::preset_browser::SearchCache,
    /// Preset names the user has starred, keyed by name (same stable
    /// identity `tile()` already uses for its thumbnail cache/id). Restored
    /// from `ui_config.favorite_presets` at bootstrap; persisted immediately
    /// on each toggle (`ui::preset_browser`'s star-click handler calls
    /// `config::save_config` directly), never deferred to `App::exiting`.
    favorite_presets: HashSet<String>,
    /// Preset Browser's "favorites only" filter toggle. Deliberately NOT
    /// persisted: matches `preset_search_query`'s own bootstrap-only reset
    /// (`:2265`, always `String::new()`): always starts `false` on launch.
    favorites_only: bool,
    /// Presets whose thumbnail render failed once already. Both
    /// `thumbnails::pump_thumbnail_queue` (writer) and the preset-browser
    /// panel (reader) consult it, so a failure can't turn into a per-tick
    /// retry loop for as long as its tile is on screen.
    failed_thumbnails: HashSet<String>,
    /// Job queue feeding `thumbnails::pump_thumbnail_queue` (Step 15):
    /// `enqueue_front`-ed by the preset-browser panel for each visible tile
    /// still missing a texture.
    thumb_queue: Vec<ThumbJob>,
    /// Cached preset thumbnails, keyed by preset name: populated by
    /// `pump_thumbnail_queue`, read by the preset-browser panel. Bounded to
    /// `thumbnails::MAX_RESIDENT_THUMBNAILS` entries (whole-branch review
    /// Finding 4): `thumbnail_order` is its insertion-order companion,
    /// see `thumbnails::insert_bounded`.
    thumbnail_textures: HashMap<String, egui::TextureHandle>,
    thumbnail_order: VecDeque<String>,
    /// The one outstanding `--render-thumbnail` child process, if any.
    /// Thumbnails are rendered out of process (see `thumbnails`' module doc:
    /// a preset that crashes projectM must not take the app down just
    /// because its tile scrolled into view), one at a time, polled by
    /// `pump_thumbnail_queue`.
    thumbnail_in_flight: Option<thumbnails::InFlightThumb>,
    /// Render children SIGKILLed after overrunning their timeout, held only
    /// until `thumbnails::reap_killed` confirms they are gone. They are
    /// never `wait()`ed on: that blocks, and this is the event-loop thread.
    thumbnail_killed: Vec<std::process::Child>,
    /// Disk cache dir for rendered thumbnails (see `thumbnails::cache_path`).
    thumbnail_cache_dir: PathBuf,
    /// Name of the monitor currently selected in the Output panel's dropdown
    /// (Step 21), if the user has picked one explicitly. `None` means
    /// "current monitor": passed straight through as `Fullscreen::
    /// Borderless(None)`. Monitors themselves are never cached on `AppState`:
    /// the panel re-queries `event_loop.available_monitors()` fresh every
    /// frame it's visible (see `ui::output`'s doc comment for why this is
    /// deliberately unlike Step 19's `input_devices` cache).
    selected_output_monitor: Option<String>,
    /// Handle to the dedicated MIDI I/O thread (Task 8): `latest()` gives
    /// the current connection/mapping/clock snapshot, `events` carries raw,
    /// unfiltered `(CommandId, value01)` dispatches drained in
    /// `about_to_wait`, `control_tx` sends port/learn/LED control messages.
    midi: opendrop_io::midi::MidiHandle,
    /// Last-pushed LED on/off state per command, so a hotplug reconnect
    /// (detected via `hotplug_epoch`) can replay every known LED state to
    /// the freshly reopened output port instead of leaving it dark.
    midi_led_state: HashMap<CommandId, bool>,
    /// Command currently in MIDI learn mode, if any, paired with whatever
    /// that command's mapping entry was at the moment Learn was clicked.
    /// Set when the panel's Learn button is clicked, cleared once the
    /// snapshot's mapping entry for that command no longer equals the
    /// paired value (see `midi_learn_completed`): comparing against
    /// "changed" rather than merely "exists" matters because
    /// `MidiControl::StartLearn` does not clear the pre-existing mapping
    /// entry, so re-learning an already-mapped command would otherwise
    /// read as instantly complete. Drives the panel's "waiting..." button
    /// state; nothing in `MidiSnapshot` signals learn-mode directly.
    midi_learning: Option<(CommandId, Option<MidiTriggerKey>)>,
    /// Whether the crossfader's soft-takeover gate has already let a MIDI
    /// value through this session: until then, incoming crossfader values
    /// are compared against `show.get_crossfader()` and only dispatched
    /// once they land within 0.08 of the live value (Ruling A: this
    /// comparison lives in `app`, not the `io` MIDI thread).
    midi_crossfader_taken_over: bool,
    /// Last-seen `MidiSnapshot::hotplug_epoch`, diffed each frame to detect
    /// an output port (re)connecting since the last check.
    midi_last_hotplug_epoch: u64,
    /// Last-seen `MidiSnapshot::clock_beat_count`, diffed each frame to
    /// detect how many MIDI clock beats fired since the last check.
    midi_last_beat_count: u64,
    /// Pending LED-off deadlines for the 120ms confirmation flash: a
    /// command's LED is pushed on immediately on dispatch and its deadline
    /// recorded here; `about_to_wait` checks this every call and pushes it
    /// back off once the deadline passes (no `std::thread::sleep`, no
    /// async timer).
    midi_led_flash_off_at: HashMap<CommandId, Instant>,
    /// Handle to the dedicated OSC UDP server thread (Task 13): `latest()`
    /// gives the current listening/port snapshot, `events` carries
    /// `(CommandId, value01)` dispatches drained in `about_to_wait` (no
    /// soft-takeover, unlike MIDI's crossfader: the brief is explicit OSC
    /// has none in the existing app), `control_tx` sends Start/Stop.
    osc: opendrop_io::osc::OscHandle,
    /// The OSC panel's own port field, read by `Start` at click time: not
    /// `OscSnapshot::port`, which only reflects the port actually bound
    /// once listening (see `ui::osc`'s doc comment). Defaults to 7000,
    /// matching OpenDrop-VJ's `electron-features-store.svelte.ts` default.
    osc_port: u16,
    /// Handle to the dedicated remote-WS thread (Task 14): first async
    /// integration in this codebase (its own tokio runtime lives entirely
    /// inside that thread, see `opendrop_io::remote_ws`'s module doc
    /// comment; no tokio type reaches `AppState` itself, only the same
    /// kind of `std::sync::mpsc` handle `osc`/`midi` expose). `events`
    /// carries `(CommandId, value01)` dispatches drained in
    /// `about_to_wait`, same no-soft-takeover contract as OSC.
    remote_ws: opendrop_io::remote_ws::RemoteWsHandle,
    /// Handle to the dedicated OBS WebSocket thread (Task 16): app->OBS
    /// direction only, no `events`/`about_to_wait` drain (see
    /// `opendrop_io::obs`'s module doc comment): `latest()` gives the
    /// current connected/scenes snapshot, `control_tx` sends Connect/
    /// Disconnect/SetScene.
    obs: opendrop_io::obs::ObsHandle,
    /// The Streaming panel's own OBS host/port fields, read by `Connect` at
    /// click time: not part of `ObsSnapshot`, same reasoning as `osc_port`
    /// (see `ui::osc`'s doc comment). Defaults match OpenDrop-VJ's
    /// `obs-link-store.svelte.ts` (`localhost`/`4455`).
    obs_host: String,
    obs_port: u16,
    /// Handle to the dedicated Twitch IRC thread (Task 17): `latest()`
    /// gives the current connected snapshot, `control_tx` sends Connect/
    /// Disconnect. Chat messages are forwarded to `chat_events` (below),
    /// not read through this handle.
    twitch: opendrop_io::twitch::TwitchHandle,
    /// The Streaming panel's own Twitch channel field, read by `Connect` at
    /// click time: same reasoning as `obs_host`/`obs_port`.
    twitch_channel: String,
    /// Draft text for the Twitch OAuth-token secret field: never holds a
    /// value after a successful save (see `ui::streaming`'s doc comment).
    twitch_oauth_token_input: String,
    /// Handle to the dedicated Kick thread (Task 17): same shape as
    /// `twitch` above.
    kick: opendrop_io::kick::KickHandle,
    /// The Streaming panel's own Kick channel field.
    kick_channel: String,
    /// Draft text for the 3 Kick credential secret fields: same "cleared
    /// after save" contract as `twitch_oauth_token_input`.
    kick_bearer_token_input: String,
    kick_xsrf_token_input: String,
    kick_cookies_input: String,
    /// Handle to the dedicated Ableton Link thread (Task 18): `latest()`
    /// gives the current enabled/tempo/beat/phase/peers snapshot,
    /// `control_tx` sends Start/Stop/SetTempo. Present only when the
    /// `link` feature is enabled (OFF by default: GPL licensing
    /// boundary, see `opendrop_io::link`'s module doc comment and the
    /// root README).
    #[cfg(feature = "link")]
    link: opendrop_io::link::LinkHandle,
    /// The Link panel's own tempo field, read by `SetTempo` at click
    /// time: not `LinkSnapshot::tempo`, same reasoning as `osc_port`
    /// (see `ui::osc`'s doc comment).
    #[cfg(feature = "link")]
    link_tempo_input: f64,
    /// Receiving end of the shared chat-message channel both `twitch` and
    /// `kick` feed (mirrors `broadcastChatMessage`, `main.cjs:425-429`:
    /// see `opendrop_io::chat`'s module doc comment). Drained every
    /// `about_to_wait` call into `chat_log` below (whole-branch review
    /// Finding 2: AC-8/AC-9: an undrained unbounded channel with a live
    /// producer is exactly the leak hazard `io::ndi::out.rs`'s `drain_slot`
    /// doc comment warns against).
    chat_events: std::sync::mpsc::Receiver<opendrop_io::chat::ChatMessage>,
    /// Bounded chat history (most recent `CHAT_LOG_CAP` messages, oldest
    /// dropped first: see `push_chat_message`) rendered at the bottom of
    /// the Streaming panel, so real Twitch/Kick chat activity is actually
    /// observable in the running app (whole-branch review Finding 2).
    chat_log: VecDeque<opendrop_io::chat::ChatMessage>,
    /// Panel-local error from the Streaming panel's save-on-blur secret
    /// fields (Twitch OAuth token, Kick bearer/xsrf/cookies): set
    /// synchronously on the UI thread by `ui::streaming::save_secret_field`
    /// when `secrets::set_secret` fails, so it can't be an `ArcSwap`
    /// snapshot field like `ObsSnapshot`/`TwitchSnapshot`/`KickSnapshot`'s
    /// `last_error` (whole-branch review Finding 1: AC-12). Cleared on the
    /// next successful save.
    streaming_secret_save_error: Option<String>,
}

#[derive(Default)]
struct App {
    state: Option<AppState>,
}

/// Single entry point for loading a preset onto a live deck: used by both
/// the preset-browser click (Step 17) and `Show::take_fired_presets()`
/// (keyboard navigation + playlist/beat-sync advances). Never touches a
/// deck directly: it only marks the slot pending and hands the request off
/// to `preflight::spawn_preflight`. `about_to_wait`'s verdict-handling drain
/// is the only place that actually calls `Deck::load_preset`.
fn request_preset_load(state: &mut AppState, slot: usize, name: String) {
    let Some(path) = should_spawn_preflight(&state.path_by_name, &mut state.pending_validations, slot, &name) else {
        return;
    };
    preflight::spawn_preflight(path, slot, name, state.preflight_tx.clone());
}

/// Whole-branch review Finding 1/2: the dedup/backpressure guard for
/// `request_preset_load`, split out so it's testable without a real
/// `AppState` (which needs a live GL context to construct). Resolves
/// `name` to a path, and: the load-bearing part: only claims `slot` (via
/// `HashSet::insert`, marking it pending for the UI's "validating…" card)
/// and returns `Some(path)` if no validation is already in flight for that
/// slot; a slot already pending returns `None`, leaving `pending_validations`
/// untouched, so the caller never spawns a second concurrent preflight
/// child for it.
///
/// Without this guard, holding a key bound to `PresetNextActive`/
/// `PlaylistNextActive`: auto-repeating at OS key-repeat rate, ~25-30/sec:
/// spawned a brand new out-of-process `projectm_create()` validation on
/// every repeat: a couple of seconds of a held key could spawn 50-60
/// concurrent instances alongside the 4 live decks already running. This
/// also closes Finding 2's stale-verdict race as a side effect: with at
/// most one in-flight validation per slot, there is never a second one
/// around to land its verdict out of order against the first: verified by
/// grepping every `pending_validations` read/write site (`main.rs`,
/// `ui/decks.rs`): the only insert is here, the only removal is
/// `about_to_wait`'s verdict drain, once the real verdict comes back: no
/// separate Clear/Cancel path exists that could remove a slot early.
fn should_spawn_preflight(
    path_by_name: &HashMap<String, PathBuf>,
    pending_validations: &mut HashSet<usize>,
    slot: usize,
    name: &str,
) -> Option<PathBuf> {
    let path = path_by_name.get(name).cloned()?;
    if !pending_validations.insert(slot) {
        return None;
    }
    Some(path)
}

/// Whether MIDI learn mode has finished for the command being learned: the
/// snapshot's current mapping entry no longer equals what it was when
/// Learn was clicked (`prev_trigger`). Deliberately compares for a
/// *change*, not merely "is now `Some`": `MidiControl::StartLearn`
/// doesn't clear the pre-existing mapping entry (`io/src/midi/control.rs`),
/// so re-learning an already-mapped command would otherwise read as
/// complete on the very next frame, before the controller was even
/// touched.
fn midi_learn_completed(prev_trigger: Option<&MidiTriggerKey>, current_trigger: Option<&MidiTriggerKey>) -> bool {
    current_trigger != prev_trigger
}

/// Whole-branch review Finding 3: whether a MIDI-dispatched command with no
/// known persistent state (`about_to_wait`'s `persistent_led == None`)
/// should still get a confirmation flash. Mirrors the JS reference's
/// positive test (`midi-connection-actions.ts:92`: `cmd?.kind === 'trigger'
/// && getCommandLedState(action) === null`): only genuine `Trigger`-kind
/// commands flash. The previous Rust port flashed for "everything else",
/// which wrongly included `Range`-kind commands (faders/knobs): every
/// incoming CC from a mapped fader sent a real 3-byte MIDI LED write,
/// potentially hundreds per second, fighting a motorized fader or flooding
/// a DIN-MIDI link. `kind` is `None` for a `CommandId` the registry has no
/// entry for.
fn should_flash_led(kind: Option<CommandKind>) -> bool {
    kind == Some(CommandKind::Trigger)
}

/// Whole-branch review Finding 3: real (measured) dt for
/// `Show::clock.step`/`Show::tick_playlists`, instead of the nominal
/// `refresh_interval` the render-pacing loop targets. `about_to_wait`'s
/// pacing resyncs `next_frame_at = now + refresh_interval` whenever the
/// loop falls behind, discarding the overrun (see that resync branch's own
/// comment): feeding nominal dt into the beat-sync clock silently
/// under-counted exactly the time a tick that fell behind actually took.
/// At 4 decks + egui + dual-window blit genuinely running at 40fps against
/// a 60fps target, a 128 BPM beat-sync played back around 85 BPM.
///
/// `last_tick_at` is `None` only on the very first tick, before there is
/// anything to measure against yet, where nominal dt is the only sane
/// fallback. `max_dt` matches the TS reference's `Math.min(dt, 0.1)`
/// (`clock.ts:53`, "avoid large jumps on tab-visibility change"): the same
/// rationale covers a debugger pause or any other long stall here, so a
/// real one doesn't feed a huge jump into beat-sync.
///
/// Frame-pacing itself (the `WaitUntil(next_frame_at)` scheduling) is
/// unaffected: it keeps targeting `refresh_interval` as before. Only the
/// clock/playlist tick inputs change to real elapsed time. Pure, so it's
/// testable without a real event loop.
fn compute_tick_dt(now: Instant, last_tick_at: Option<Instant>, nominal: Duration, max_dt: Duration) -> f64 {
    let elapsed = match last_tick_at {
        Some(prev) => now.saturating_duration_since(prev),
        None => nominal,
    };
    elapsed.min(max_dt).as_secs_f64()
}

/// Drains `rx` completely, returning only the most recently received item
/// (or `None` if nothing was waiting). Used where a consumer only cares
/// about the latest value from an unbounded channel and polls it once per
/// tick: mirrors `io::ndi::out::drain_slot`'s "never let this back up"
/// reasoning (see that function's doc comment), applied to a `Receiver<T>`
/// a caller reads directly instead of forwarding into a sender.
fn drain_to_latest<T>(rx: &mpsc::Receiver<T>) -> Option<T> {
    let mut newest = None;
    while let Ok(item) = rx.try_recv() {
        newest = Some(item);
    }
    newest
}

/// Pushes `msg` onto `log`, then trims from the front until `log.len() <=
/// cap`: keeps only the most recent `cap` messages. Whole-branch review
/// Finding 2: pure and side-effect-free so the capping behavior is
/// testable without a real chat channel.
fn push_chat_message(log: &mut VecDeque<opendrop_io::chat::ChatMessage>, msg: opendrop_io::chat::ChatMessage, cap: usize) {
    log.push_back(msg);
    while log.len() > cap {
        log.pop_front();
    }
}

/// Root of the control window's egui content for this frame. `ui` here is
/// already `&mut egui::Ui` (this vendored `egui_glow`'s `EguiGlow::run`
/// hands a `Ui`, not a `Context`: `CentralPanel::show` in this vendored
/// `egui` 0.36.1 matches, taking `ui: &mut Ui` as its first argument; see
/// Task 2's notes on this same drift). A 4-zone shell (Step 10 of the Phase
/// 7 UI redesign plan): header, sectioned nav, status bar, then content:
/// `egui::Panel::top`/`left`/`bottom` (not `SidePanel`/`TopBottomPanel`,
/// which don't exist in this vendored egui 0.36.1), `CentralPanel` last, in
/// that mandated order. `ui::shell::{header,nav,status_bar}` hold the zone
/// bodies; this function only wires them up plus the unchanged content
/// match (Step 9's version, just relocated into the new `CentralPanel`
/// zone).
///
/// Takes individual `AppState` fields, not `&mut AppState`: see
/// `ui::decks::show`'s doc comment for why. `load_request` is an out-param:
/// the preset-browser panel can't call `request_preset_load` itself (that
/// needs the whole `AppState`), so a click just records the name here for
/// the caller to act on once this frame's egui pass is done.
// Step 9 (Phase 7 UI redesign plan): the 51 (53 with `--features link`)
// individual `AppState`-derived params this function used to take are
// grouped into 7 disjoint `&mut` context structs (`ui::ctx`) plus
// `theme_request` as an 8th, standalone out-param: pure re-packaging, see
// `ui::ctx`'s module doc comment for the field-to-struct assignment and the
// borrow-check reasoning behind it.
#[allow(clippy::too_many_arguments)]
fn ui_root(
    ui: &mut egui::Ui,
    shell: &mut ui::ctx::ShellCtx,
    perform: &mut ui::ctx::PerformCtx,
    library: &mut ui::ctx::LibraryCtx,
    sources: &mut ui::ctx::SourcesCtx,
    output: &mut ui::ctx::OutputCtx,
    stream: &mut ui::ctx::StreamCtx,
    control: &mut ui::ctx::ControlCtx,
    theme_request: &mut Option<theme::registry::ThemeId>,
) {
    // Step 11 of the Phase 7 UI redesign plan: Stage mode retracts the nav
    // and switches the header/status bar to compact, translucent variants
    // while live-mixing. `Panel::show_switched` is a `Panel`-associated
    // function, not a method chained onto one builder as the brief's own
    // prose first suggested: verified against vendored egui 0.36.1 source
    // (`egui-0.36.1/src/containers/panel.rs:563`): it takes the collapsed
    // and expanded `Panel` builders as two separate arguments plus one
    // shared `is_expanded: &mut bool`, and animates the slide between
    // them. Normal is always the "expanded" side, Stage the "collapsed"
    // side, for both the header and the status bar: matching `nav`'s own
    // collapse direction below. Both switches use a *local* `bool` derived
    // fresh from `*shell.stage_mode` every frame (never written back):
    // every panel below is `resizable(false)`, so egui never mutates
    // `is_expanded` internally (drag-to-collapse/expand and the resize
    // double-click both live inside `if resizable { .. }` in
    // `show_inside_dyn`): the only writer of `*shell.stage_mode` is the
    // `F11` handler in `window_event` and the header's own `⛶` button
    // (`ui::shell::header`), and a write-back here would race the latter:
    // the button flips `*shell.stage_mode` *during* this same call, and
    // overwriting it with a value computed before the call would silently
    // discard that click.
    //
    // Distinct `Id`s are mandatory between each zone's Normal/Stage panel
    // (`"od_header"` vs `"od_header_stage"`, `"od_status"` vs `"od_status_
    // stage"`): `show_switched` itself `debug_assert!`s on this. All
    // Stage-mode chrome shares one `Frame::multiply_with_opacity(0.6)`:
    // background + border + shadow modulated together in one operation
    // (`egui-0.36.1/src/containers/frame.rs:313`), never a per-color
    // `gamma_multiply` hack: so the live GL compositor (confirmed opaque
    // and hidden behind Normal-mode chrome, Task 1) shows through the
    // retracted areas.
    let stage_frame = egui::Frame::side_top_panel(ui.style()).multiply_with_opacity(0.6);

    let mut header_expanded = !*shell.stage_mode;
    egui::Panel::show_switched(
        ui,
        &mut header_expanded,
        egui::Panel::top("od_header_stage").resizable(false).exact_size(28.0).show_separator_line(false).frame(stage_frame),
        egui::Panel::top("od_header").resizable(false).exact_size(48.0).show_separator_line(false),
        |ui, expanded| {
            if expanded {
                ui::shell::header(ui, shell, perform, theme_request);
            } else {
                ui::shell::header_stage(ui, shell.stage_mode);
            }
        },
    );

    let mut nav_open = !*shell.stage_mode;
    egui::Panel::left("od_nav").resizable(false).exact_size(184.0).show_collapsible(ui, &mut nav_open, |ui| {
        ui::shell::nav(ui, shell);
    });

    let mut status_expanded = !*shell.stage_mode;
    egui::Panel::show_switched(
        ui,
        &mut status_expanded,
        egui::Panel::bottom("od_status_stage").resizable(false).exact_size(64.0).frame(stage_frame),
        egui::Panel::bottom("od_status").resizable(false).exact_size(26.0),
        |ui, expanded| {
            if expanded {
                ui::shell::status_bar(ui, shell, perform, library, sources, output, stream, control);
            } else {
                ui::shell::status_bar_stage(ui, shell, perform, sources);
            }
        },
    );

    // Preset drawer: Stage-only, collapsible over the live content
    // (`Panel::bottom("od_presets_drawer").show_collapsible`), toggled by
    // a `ghost_button` inside `status_bar_stage` rather than a new
    // keyboard binding (the brief's own "Bascule clavier" section only
    // calls out `stage_mode`'s toggle). Gated on `*shell.stage_mode`
    // itself, not just left permanently mounted with `presets_drawer_open`
    // pinned to `false`: Normal mode already reaches the same content via
    // `Panel::PresetBrowser` in the nav, so there's no reason to reserve
    // an extra `Panel::bottom` id/slide state for it there.
    if *shell.stage_mode {
        egui::Panel::bottom("od_presets_drawer")
            .resizable(false)
            .exact_size(260.0)
            .frame(stage_frame)
            .show_collapsible(ui, shell.presets_drawer_open, |ui| {
                ui::preset_browser::show(ui, perform, library);
            });
    }

    egui::CentralPanel::default().show(ui, |ui| {
        match shell.active_panel {
            Panel::Decks => {
                ui::decks::show(
                    ui,
                    perform.show,
                    perform.deck_tex_ids,
                    perform.deck_preset_names,
                    perform.pending_validations,
                    perform.preset_errors,
                    perform.transition_seconds,
                );
            }
            Panel::PresetBrowser => {
                ui::preset_browser::show(ui, perform, library);
            }
            Panel::Playlists => {
                ui::playlists::show(ui, perform.show, library);
            }
            Panel::Audio => {
                ui::audio::show(ui, sources.audio, sources.input_devices, sources.selected_input_device, sources.last_vu_level);
            }
            Panel::Quality => {
                ui::quality::show(ui, output.refresh_interval, output.invisible_mode, output.pending_mesh_size);
            }
            Panel::Output => {
                ui::output::show(ui, output.event_loop, output.output_window, output.selected_output_monitor);
            }
            Panel::Midi => {
                ui::midi::show(ui, sources.midi, sources.registry, sources.midi_learning);
            }
            Panel::NdiIn => {
                ui::ndi::show_in(ui, output.ndi, sources.ndi_in_selected_source);
            }
            Panel::NdiOut => {
                ui::ndi::show_out(ui, output.ndi, output.ndi_composite_active, output.ndi_deck_active);
            }
            Panel::Osc => {
                ui::osc::show(ui, sources.osc, sources.osc_port);
            }
            Panel::RemoteWs => {
                ui::remote::show(ui, sources.remote_ws);
            }
            Panel::Streaming => {
                ui::streaming::show(
                    ui,
                    stream.obs,
                    stream.obs_host,
                    stream.obs_port,
                    stream.twitch,
                    stream.twitch_channel,
                    stream.twitch_oauth_token_input,
                    stream.kick,
                    stream.kick_channel,
                    stream.kick_bearer_token_input,
                    stream.kick_xsrf_token_input,
                    stream.kick_cookies_input,
                    stream.chat_log,
                    stream.streaming_secret_save_error,
                );
            }
            #[cfg(feature = "link")]
            Panel::Link => {
                ui::link::show(ui, control.link, control.link_tempo_input);
            }
            Panel::V4l2 => {
                ui::v4l2loopback::show(ui, sources.v4l2, sources.v4l2_active, sources.v4l2_device);
            }
            Panel::About => {
                ui::about::show(ui);
            }
        }
    });
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        self.state = Some(bootstrap(event_loop).expect("GL/EGL bootstrap failed"));
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now()));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };

        // egui first, control window only: output never carries UI.
        if window_id == state.control.window.id() {
            let _egui_response = state.egui_glow.on_window_event(&state.control.window, &event);
        }

        // Handled regardless of which window has focus: both windows show
        // the same show state, so the keymap isn't per-window. Gated on
        // egui_wants_keyboard_input() (not EventResponse.consumed, which is
        // also true for e.g. a mouse click on a button) so debug shortcuts
        // keep working except while an egui text widget (e.g. the preset
        // browser search) actually has focus.
        if let WindowEvent::KeyboardInput { event: key_event, .. } = &event {
            // Whole-branch review Finding 1: `!key_event.repeat` excludes
            // OS auto-repeat: every command dispatched from this path is a
            // discrete press/trigger (deck navigation, toggles), and a
            // held key re-firing one at ~25-30/sec is never the intended
            // UX for any of them, not just the navigation commands that
            // exposed the preflight-flood bug. Filtered globally here
            // rather than per-command.
            if key_event.state == ElementState::Pressed
                && !key_event.repeat
                && !state.egui_glow.egui_ctx.egui_wants_keyboard_input()
            {
                if let Some(&cmd_id) = state.keymap.get(&key_event.logical_key) {
                    state.registry.dispatch(cmd_id, 1.0, &mut state.show);
                }

                // Step 11 of the Phase 7 UI redesign plan: Stage mode is a
                // transient UI bool on `AppState`, not a `CommandId`:
                // matched directly against the logical key here rather
                // than routed through `state.keymap`/`state.registry.
                // dispatch`, under the same guard as the dispatch above.
                if key_event.logical_key == Key::Named(NamedKey::F11) {
                    state.stage_mode = !state.stage_mode;
                }
            }
        }

        let slot = if window_id == state.control.window.id() {
            &mut state.control
        } else if window_id == state.output.window.id() {
            &mut state.output
        } else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Occluded(occluded) => slot.occluded = occluded,
            WindowEvent::Resized(new_size) => {
                slot.size = (new_size.width.max(1), new_size.height.max(1));
                if let (Some(w), Some(h)) = (NonZeroU32::new(slot.size.0), NonZeroU32::new(slot.size.1)) {
                    slot.surface.resize(&state.main_ctx, w, h);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else { return };

        // Non-blocking drain so results from spawn_preflight's threads
        // never back up in the channel. This is the only place that ever
        // calls `Deck::load_preset` on a running deck (besides the
        // untouched 4-preset bootstrap load): a preset only reaches here
        // after its own preflight child process has already loaded it
        // successfully in isolation.
        while let Ok((slot, name, verdict)) = state.preflight_rx.try_recv() {
            state.pending_validations.remove(&slot);
            match verdict {
                preflight::PreflightVerdict::Ok => {
                    if let Some(path) = state.path_by_name.get(&name) {
                        if let Err(e) = state.decks[slot].context.make_current(&state.decks[slot].surface) {
                            state.preset_errors.insert(slot, format!("GL error: {e}"));
                        } else {
                            state.decks[slot].set_soft_cut_duration(state.transition_seconds);
                            if let Err(e) = state.decks[slot].load_preset(path, state.transition_seconds > 0.0) {
                                state.preset_errors.insert(slot, e);
                            } else {
                                state.preset_errors.remove(&slot);
                                state.deck_preset_names[slot] = name;
                            }
                        }
                    }
                }
                preflight::PreflightVerdict::Failed(msg) => {
                    state.preset_errors.insert(slot, msg);
                }
            }
        }
        for load in state.show.take_fired_presets() {
            request_preset_load(state, load.slot, load.name);
        }

        // MIDI (Task 8). `midi.events` carries raw, unfiltered
        // `(CommandId, value01)` dispatches (Task 7's `MidiDispatch`):
        // soft-takeover (crossfader only, Ruling A), LED flash vs.
        // persistent-state confirmation, and mapping/clock/hotplug sync all
        // happen here, mirroring `midi-connection-actions.ts:70-134`.
        while let Ok((id, value01)) = state.midi.events.try_recv() {
            if id == CommandId::Crossfader && !state.midi_crossfader_taken_over {
                if (value01 - state.show.get_crossfader()).abs() > 0.08 {
                    continue; // soft-takeover gate: knob not yet in phase with the live value
                }
                state.midi_crossfader_taken_over = true;
            }
            state.registry.dispatch(id, value01, &mut state.show);

            // Persistent-state commands push their real on/off state
            // immediately, with no confirmation flash (mirrors the
            // `cmd?.kind === 'trigger'` guard in the JS reference: a
            // flash here would wrongly overwrite the state just pushed).
            // Every other dispatched *Trigger*-kind command gets a 120ms
            // flash instead; a `Range`-kind command (fader/knob) gets no
            // LED write at all: whole-branch review Finding 3 (real bug):
            // the previous `_ => flash` branch swept in every Range-kind
            // command too, sending a real MIDI LED write for every incoming
            // CC message from a mapped fader, potentially hundreds per
            // second, fighting a motorized fader or flooding a DIN-MIDI
            // link. See `should_flash_led`'s doc comment for the JS
            // reference this now mirrors.
            let persistent_led = match id {
                CommandId::PlaylistToggleA => Some(state.show.get_playlist_playing(opendrop_core::commands::Deck::A)),
                CommandId::PlaylistToggleB => Some(state.show.get_playlist_playing(opendrop_core::commands::Deck::B)),
                CommandId::PlaylistToggleActive => {
                    let deck = state.show.get_active_deck();
                    Some(state.show.get_playlist_playing(deck))
                }
                _ => None,
            };
            if let Some(on) = persistent_led {
                state.midi.push_led(id, on);
                state.midi_led_state.insert(id, on);
            } else if should_flash_led(state.registry.get(id).map(|cmd| cmd.kind)) {
                state.midi.push_led(id, true);
                state.midi_led_state.insert(id, true);
                state.midi_led_flash_off_at.insert(id, Instant::now() + MIDI_LED_FLASH_DURATION);
            }
        }

        // LED flash expiry: a per-frame `Instant` deadline check, not a
        // blocking timer (this thread has no async runtime). Swapped out
        // via `mem::take` so the `retain` closure can freely touch
        // `state.midi`/`state.midi_led_state` without aliasing the very
        // map it's iterating.
        let midi_flash_now = Instant::now();
        let mut midi_led_flash_off_at = std::mem::take(&mut state.midi_led_flash_off_at);
        midi_led_flash_off_at.retain(|&id, &mut deadline| {
            if midi_flash_now >= deadline {
                state.midi.push_led(id, false);
                state.midi_led_state.insert(id, false);
                false
            } else {
                true
            }
        });
        state.midi_led_flash_off_at = midi_led_flash_off_at;

        // Clock sync, hotplug LED replay, and learn-mode completion: all
        // diffed against the same snapshot fetch. A learning command never
        // appears on `midi.events` (the io thread consumes that message to
        // complete the learn instead of dispatching it), so learn
        // completion can only be observed via the mapping snapshot here.
        let midi_snapshot = state.midi.latest();
        let midi_new_beats = midi_snapshot.clock_beat_count.wrapping_sub(state.midi_last_beat_count);
        for _ in 0..midi_new_beats {
            state.show.clock.pulse(Some(midi_snapshot.clock_bpm));
            // Pulse-only mode (bpm unknown/0, e.g. right as the MIDI clock
            // locks on): `step()` never advances phase at bpm 0, so this
            // mirrors the audio-detector's own `if clock.bpm() == 0.0 {
            // on_beat() }` fallback a few lines below: without it a MIDI
            // clock beat fired while bpm is still 0 would silently never
            // call `on_beat()`.
            if state.show.clock.bpm() == 0.0 {
                state.show.on_beat();
            }
        }
        state.midi_last_beat_count = midi_snapshot.clock_beat_count;

        if midi_snapshot.hotplug_epoch != state.midi_last_hotplug_epoch {
            for (&id, &on) in &state.midi_led_state {
                state.midi.push_led(id, on);
            }
            state.midi_last_hotplug_epoch = midi_snapshot.hotplug_epoch;
            // A (re)connection may bring in a different controller (or the
            // same one moved) now out of phase with the live crossfader
            // value: mirrors the JS reference's per-connection `takenOver`
            // reset (a fresh `Set` on every `toggleMidi` call).
            state.midi_crossfader_taken_over = false;
        }

        let midi_learn_done = match &state.midi_learning {
            Some((learning_id, prev_trigger)) => {
                midi_learn_completed(prev_trigger.as_ref(), midi_snapshot.mapping.get(learning_id))
            }
            None => false,
        };
        if midi_learn_done {
            state.midi_learning = None;
        }

        // OSC (Task 13). `osc.events` carries `(CommandId, value01)`
        // dispatches already filtered/clamped by the OSC thread (address
        // prefix + command-name lookup + 0..1 clamp all happen in
        // `opendrop_io::osc`, not here). No soft-takeover: unlike MIDI's
        // crossfader, OSC has no such gate in the existing app.
        while let Ok((id, value01)) = state.osc.events.try_recv() {
            state.registry.dispatch(id, value01, &mut state.show);
        }

        // Remote WS (Task 14). Same shape as the OSC drain just above:
        // `remote_ws.events` carries `(CommandId, value01)` dispatches
        // already filtered (token check + command-name lookup) and
        // clamped by the remote-WS thread itself (`opendrop_io::
        // remote_ws`, not here). No soft-takeover, same as OSC.
        while let Ok((id, value01)) = state.remote_ws.events.try_recv() {
            state.registry.dispatch(id, value01, &mut state.show);
        }

        // Ableton Link (Task 18). The Link thread never touches `Show`
        // itself (see `opendrop_io::link`'s module doc comment): it only
        // publishes the latest polled `(tempo, phase01)`; applying that
        // to the clock happens here, once per `about_to_wait` call, same
        // as the MIDI clock-pulse handling above. `sync_external` forces
        // both bpm and phase from Link's authoritative timeline and
        // fires a beat when phase wraps.
        #[cfg(feature = "link")]
        {
            let link_snapshot = state.link.latest();
            if link_snapshot.enabled {
                for _ in 0..state.show.clock.sync_external(link_snapshot.tempo, link_snapshot.phase01) {
                    state.show.on_beat();
                }
            }
        }

        // Chat (Task 17 follow-up, whole-branch review Finding 2). Same
        // non-blocking drain shape as the OSC/remote-WS drains above, but
        // there's no command dispatch here: just a bounded history so
        // this unbounded `mpsc::Sender` (fed by `twitch`/`kick`) never
        // backs up, mirroring `io::ndi::out.rs`'s `drain_slot` doc comment
        // on why an undrained unbounded channel is a real leak.
        while let Ok(msg) = state.chat_events.try_recv() {
            push_chat_message(&mut state.chat_log, msg, CHAT_LOG_CAP);
        }

        let now = Instant::now();
        // Wayland can wake this loop for reasons unrelated to pacing (e.g.
        // buffer-release protocol traffic generated by our own previous
        // swap): about_to_wait fires far more often than the WaitUntil
        // deadline requests. Gating the render on next_frame_at, instead of
        // rendering on every call, is what keeps that from turning into a
        // self-sustaining busy loop (measured: ~10 kHz without this gate).
        if now >= state.next_frame_at {
            let layer_inputs = layer_inputs_from_show(&state.show);

            // Each deck context injects one PCM chunk, renders one projectM
            // frame, and copies it into its shared texture; then, back on
            // the main context, each texture is drawn through the
            // compositor shader into the composite FBO. A deck at or below
            // the 0.001 opacity floor: never sampled by composite_layer
            // either way: is culled down to IDLE_DECK_INTERVAL instead of
            // rendering at full rate for nothing: the "4 decks rendered at
            // full resolution even while invisible" pathology from the
            // diagnostic, killed at the root rather than papered over.
            let audio = state.audio.latest();

            // Beat-sync engine wiring (Step 18): runs once per tick, before
            // the deck-rendering loop below. `take_fired_presets` is drained
            // at the top of `about_to_wait`, once per call, before this
            // render-gated block: a preset/playlist advance fired by
            // `show.on_beat()` here becomes visible on that drain at the
            // start of the next `about_to_wait` call, same path as a
            // keyboard-driven `navigate_preset`. Placing this block ahead of
            // the deck-rendering loop, rather than after it, starts that
            // preset's async pre-flight validation as early as possible in
            // the tick. `now_ms` is `t0.elapsed()`, a monotonic clock
            // independent of `next_frame_at` (the render-pacing deadline,
            // which drifts by design, see the resync branch below). Never
            // derive it from `next_frame_at`.
            let now_ms = state.t0.elapsed().as_secs_f64() * 1000.0;
            // Real elapsed time, not the nominal refresh_interval (Finding
            // 3): see `compute_tick_dt`'s doc comment.
            let dt = compute_tick_dt(now, state.last_output_swap_at, state.refresh_interval, MAX_TICK_DT);
            if state.show.manual_bpm == 0.0 {
                let r = state.show.beat_detector.process_sample(audio.energy_byte, now_ms);
                if r.beat_triggered {
                    state.show.clock.pulse(Some(r.bpm));
                    if state.show.clock.bpm() == 0.0 {
                        state.show.on_beat();
                    }
                }
            }
            for _ in 0..state.show.clock.step(dt) {
                state.show.on_beat();
            }
            // The interval-driven half of the same playlist engines the
            // beats above drive: without this the Playlists panel's
            // "Interval (s)" slider does nothing and Play only ever loads
            // the current item. Same `dt` as `clock.step`, converted to the
            // milliseconds `PlaylistEngine` works in.
            state.show.tick_playlists(dt * 1000.0);
            state.last_vu_level = opendrop_audio::analysis::vu_level(&audio.pcm);
            state.show.check_volume_peak_triggers(state.last_vu_level, now_ms);

            for (i, layer_input) in layer_inputs.iter().enumerate() {
                let visible = layer_input.opacity > 0.001;
                // See `InvisibleMode`: `Eco` is the original always-on
                // throttle, `Pause` skips rendering this deck entirely while
                // invisible (its texture keeps showing the last frame it
                // rendered), `Off` renders invisible decks at full rate.
                let should_render = match (visible, state.invisible_mode) {
                    (true, _) | (false, InvisibleMode::Off) => true,
                    (false, InvisibleMode::Eco) => now >= state.deck_next_render_at[i],
                    (false, InvisibleMode::Pause) => false,
                };
                if !should_render {
                    continue;
                }
                if let Err(e) = state.decks[i].context.make_current(&state.decks[i].surface) {
                    eprintln!("[app] deck {i} make_current failed: {e}");
                    continue;
                }
                // Applied here, not before make_current above: `set_mesh_size`
                // is an FFI call into this deck's projectM instance and must
                // run while its context is current, same as `render_frame`.
                if let Some((w, h)) = state.pending_mesh_size[i].take() {
                    state.decks[i].set_mesh_size(w, h);
                }
                state.decks[i].render_frame(&audio.pcm);
                if !visible && state.invisible_mode == InvisibleMode::Eco {
                    state.deck_next_render_at[i] = now + IDLE_DECK_INTERVAL;
                }
            }

            // Preset-browser thumbnail pump (Step 17): at most one unit of
            // work per tick (see `pump_thumbnail_queue`'s own doc comment),
            // and only while the panel that could actually show the result
            // is visible: otherwise this would keep rendering thumbnails no
            // one can see. The `is_some()` half is what reaps a child that
            // was still rendering when the user switched panels; the pump
            // returns as soon as it has handled that child, so it never
            // drains the rest of the queue while the browser is hidden.
            //
            // Unlike the deck loop above, this touches no GL context of its
            // own any more: the render happens in a child process, and all
            // this does here is poll it and upload the bytes it wrote.
            // Deliberately outside the gate below: a render child killed on
            // timeout has to be reaped whether or not the browser panel is
            // still on screen, and this is a `try_wait` per outstanding
            // corpse, normally zero of them.
            thumbnails::reap_killed(&mut state.thumbnail_killed);
            if state.active_panel == Panel::PresetBrowser || state.thumbnail_in_flight.is_some() {
                if let Err(e) = thumbnails::pump_thumbnail_queue(
                    &mut state.thumb_queue,
                    &mut state.thumbnail_in_flight,
                    &mut state.thumbnail_killed,
                    &state.thumbnail_cache_dir,
                    &state.path_by_name,
                    &state.egui_glow.egui_ctx,
                    &mut state.thumbnail_textures,
                    &mut state.thumbnail_order,
                    &mut state.failed_thumbnails,
                ) {
                    eprintln!("[app] thumbnail pump failed: {e}");
                }
            }

            // Reacquire the main context (any of its surfaces works: the
            // composite FBO belongs to the context, not the surface) before
            // touching the compositor or either window.
            if let Err(e) = state.main_ctx.make_current(&state.control.surface) {
                eprintln!("[app] failed to reacquire main context: {e}");
            }

            // NDI-in (Task 12): drains `frame_rx` to its newest frame per
            // tick, before compositing (whole-branch review Finding 5: a
            // single `try_recv()` took exactly one frame per app tick,
            // which falls behind and can never recatch against this
            // unbounded channel if the NDI source's rate ever briefly
            // exceeds the app's tick rate: mirrors `io::ndi::out.rs`'s own
            // `drain_slot`, which loops for exactly this reason). `frame_rx`
            // only yields a frame once `NdiControl::StartReceive` is active
            // and a source is actually connected (io/src/ndi/in_.rs): most
            // ticks this is empty. The texture is sized from the frame's
            // own width/height (`NdiFrame`, see io's Task-12 prerequisite
            // fix) rather than a fixed constant, since NDI-in sources can be
            // any resolution; a same-size frame is uploaded in place, a
            // resolution change (or first connect) recreates the texture.
            if let Some(frame) = drain_to_latest(&state.ndi.frame_rx) {
                let expected_len = frame.width as usize * frame.height as usize * 4;
                if frame.data.len() != expected_len {
                    eprintln!(
                        "[app] dropping NDI-in frame with unexpected size: {} bytes, expected {expected_len} for {}x{}",
                        frame.data.len(),
                        frame.width,
                        frame.height
                    );
                } else if let Some((tex, w, h)) = state.ndi_in_texture {
                    if (w, h) == (frame.width, frame.height) {
                        unsafe {
                            state.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                            state.gl.tex_sub_image_2d(
                                glow::TEXTURE_2D,
                                0,
                                0,
                                0,
                                frame.width as i32,
                                frame.height as i32,
                                glow::RGBA,
                                glow::UNSIGNED_BYTE,
                                glow::PixelUnpackData::Slice(Some(&frame.data)),
                            );
                        }
                    } else {
                        unsafe { state.gl.delete_texture(tex) };
                        state.ndi_in_texture = None;
                        create_ndi_in_texture(&state.gl, &frame, &mut state.ndi_in_texture);
                    }
                } else {
                    create_ndi_in_texture(&state.gl, &frame, &mut state.ndi_in_texture);
                }
            }

            let lowest_active = (0..deck::DECK_COUNT).find(|&i| layer_inputs[i].opacity > 0.001);
            state.compositor.begin_frame(&state.gl);
            for (i, layer_input) in layer_inputs.iter().enumerate() {
                // Whole-branch review Finding I5: this used to be
                // open-coded (`lowest_active == Some(i)`), untested here:
                // now the tested `opendrop_core::blend::
                // should_force_normal_for_lowest_slot` port of `compositor.
                // ts:140`'s `shouldForceNormalForLowestSlot`.
                let force_normal = should_force_normal_for_lowest_slot(i, lowest_active);
                state.compositor.composite_layer(&state.gl, state.decks[i].texture, layer_input, force_normal);
            }
            // NDI-in layer, composited last, over every deck, as part of
            // this same shared pass: `render_and_swap*` later just blits
            // the resulting `color_tex` to each window, so drawing this
            // here (not per-window) is what makes it appear in both control
            // and output. `force_normal: false`: this layer is never the
            // lowest/first-drawn slot (see `force_normal`'s doc comment in
            // `compositor.rs`), so there is nothing to override:
            // `DEFAULT_SLOT_COMPOSITE` already carries `BlendMode::Normal`,
            // full opacity, no keying. Gated on `receive_active`, not just
            // `ndi_in_texture.is_some()`: the texture is never deleted on
            // `StopReceive` (cheap to keep around for a fast reconnect), so
            // without this check a disconnected session would keep drawing
            // its last received frame forever instead of nothing.
            if state.ndi.latest().receive_active {
                if let Some((tex, _, _)) = state.ndi_in_texture {
                    let ndi_in_input = LayerInput { opacity: 1.0, composite: DEFAULT_SLOT_COMPOSITE, color: DEFAULT_COLOR_PARAMS };
                    state.compositor.composite_layer(&state.gl, tex, &ndi_in_input, false);
                }
            }
            state.compositor.end_frame(&state.gl);

            // Step 5: GPU->CPU readback feeding the NDI / v4l2loopback
            // output paths: gated per consumer so an idle (or
            // partially-idle) session never pays for a copy nobody wants
            // (whole-branch review Finding I5). The composite readback is
            // needed by either `ndi_composite_active` or `v4l2_active`
            // (v4l2 only ever pipes the composite stream); each deck
            // readback is needed only by that specific deck's
            // `ndi_deck_active[i]`: v4l2 never needs a per-deck readback.
            // A polled `Some(bytes)` is just pushed onto its channel and
            // silently dropped if nothing is receiving, same non-blocking,
            // ignore-on-fail convention as `AudioHandle::set_device`.
            if state.ndi_composite_active || state.v4l2_active {
                state.compositor_readback.begin_read(&state.gl);
                if let Some(bytes) = state.compositor_readback.poll(&state.gl) {
                    // Two consumers, two channels (see `v4l2_frame_tx`'s doc
                    // comment): only clone when both actually want a copy,
                    // otherwise hand the one copy straight to whichever
                    // consumer needs it.
                    match (state.v4l2_active, state.ndi_composite_active) {
                        (true, true) => {
                            let _ = state.v4l2_frame_tx.send(bytes.clone());
                            let _ = state.compositor_frame_tx.send(bytes);
                        }
                        (true, false) => {
                            let _ = state.v4l2_frame_tx.send(bytes);
                        }
                        (false, true) => {
                            let _ = state.compositor_frame_tx.send(bytes);
                        }
                        (false, false) => {} // unreachable: the outer `if` guarantees one of the two
                    }
                }
            }
            for i in 0..deck::DECK_COUNT {
                if state.ndi_deck_active[i] {
                    state.deck_readback[i].begin_read(&state.gl);
                    if let Some(bytes) = state.deck_readback[i].poll(&state.gl) {
                        let _ = state.deck_frame_tx[i].send(bytes);
                    }
                }
            }

            // Copied out (Instant/f64 are Copy) before the destructure below
            // so the Playlists panel's Tap Tempo button can compute its own
            // `t0.elapsed()` at click time, and the Audio panel's VU meter
            // can read this tick's level. Neither is named in that
            // destructure, so `state.t0`/`state.last_vu_level` stay readable
            // either way.
            let t0 = state.t0;
            let last_vu_level = state.last_vu_level;
            // Status bar's fps/frame-ms readout (Step 10): same by-value
            // copy-before-destructure convention as `last_vu_level` above,
            // since `ShellCtx::last_wall_ms` is a plain `Option<f64>`, not
            // a `&mut` field.
            let last_wall_ms = state.last_wall_ms;

            // Decks (Step 16), preset-browser (Step 17), playlists (Step
            // 18), and audio (Step 19) panels: real content, replacing the
            // Step 2 placeholder. Destructured so the closure below only
            // borrows the fields it needs, disjoint from `egui_glow` itself
            // (see `ui_root`'s doc comment).
            let AppState {
                egui_glow,
                control,
                output,
                show,
                deck_tex_ids,
                deck_preset_names,
                pending_validations,
                preset_errors,
                transition_seconds,
                active_panel,
                stage_mode,
                presets_drawer_open,
                preset_search_query,
                preset_search_cache,
                favorite_presets,
                favorites_only,
                thumb_queue,
                thumbnail_textures,
                failed_thumbnails,
                audio,
                input_devices,
                selected_input_device,
                refresh_interval,
                invisible_mode,
                pending_mesh_size,
                selected_output_monitor,
                registry,
                midi,
                midi_learning,
                ndi,
                ndi_composite_active,
                ndi_deck_active,
                ndi_in_selected_source,
                osc,
                osc_port,
                remote_ws,
                obs,
                obs_host,
                obs_port,
                twitch,
                twitch_channel,
                twitch_oauth_token_input,
                kick,
                kick_channel,
                kick_bearer_token_input,
                kick_xsrf_token_input,
                kick_cookies_input,
                chat_log,
                streaming_secret_save_error,
                #[cfg(feature = "link")]
                link,
                #[cfg(feature = "link")]
                link_tempo_input,
                v4l2,
                v4l2_active,
                v4l2_device,
                ..
            } = state;
            // Out-param for the preset-browser click path: see `ui_root`'s
            // doc comment. `show` is still borrowed (via the destructure
            // above) for the duration of this closure, so the actual
            // `request_preset_load` call has to wait until after `run()`
            // returns, below.
            let mut preset_load_request: Option<String> = None;
            // Step 9: plumbing only, not consumed yet: see `ui_root`'s own
            // comment on this parameter.
            let mut theme_request: Option<theme::registry::ThemeId> = None;

            // Step 9: group the bindings the destructure above just
            // produced into the 7 context structs `ui_root` now takes,
            // instead of 51/53 individual arguments. Every field here is
            // exactly one of those bindings: no new state. Named with a
            // `_ctx` suffix only where the destructure already bound a
            // same-named `WindowSlot` (`control`, `output`) that's still
            // needed below for `egui_glow.run`/`&output.window`.
            let mut shell_ctx = ui::ctx::ShellCtx { active_panel, stage_mode, last_wall_ms, presets_drawer_open };
            let mut perform_ctx = ui::ctx::PerformCtx {
                show,
                deck_tex_ids,
                deck_preset_names,
                pending_validations,
                preset_errors,
                transition_seconds,
                t0,
            };
            let mut library_ctx = ui::ctx::LibraryCtx {
                preset_search_query,
                search_cache: preset_search_cache,
                thumb_queue,
                thumbnail_textures,
                failed_thumbnails,
                load_request: &mut preset_load_request,
                favorite_presets,
                favorites_only,
            };
            let mut sources_ctx = ui::ctx::SourcesCtx {
                audio,
                input_devices,
                selected_input_device,
                last_vu_level,
                registry,
                midi,
                midi_learning,
                ndi_in_selected_source,
                osc,
                osc_port,
                remote_ws,
                v4l2,
                v4l2_active,
                v4l2_device,
            };
            let mut output_ctx = ui::ctx::OutputCtx {
                refresh_interval,
                invisible_mode,
                pending_mesh_size,
                event_loop,
                output_window: &output.window,
                selected_output_monitor,
                ndi,
                ndi_composite_active,
                ndi_deck_active,
            };
            let mut stream_ctx = ui::ctx::StreamCtx {
                obs,
                obs_host,
                obs_port,
                twitch,
                twitch_channel,
                twitch_oauth_token_input,
                kick,
                kick_channel,
                kick_bearer_token_input,
                kick_xsrf_token_input,
                kick_cookies_input,
                chat_log,
                streaming_secret_save_error,
            };
            let mut control_ctx = ui::ctx::ControlCtx {
                #[cfg(feature = "link")]
                link,
                #[cfg(feature = "link")]
                link_tempo_input,
                #[cfg(not(feature = "link"))]
                _marker: std::marker::PhantomData,
            };
            egui_glow.run(&control.window, |ui| {
                ui_root(
                    ui,
                    &mut shell_ctx,
                    &mut perform_ctx,
                    &mut library_ctx,
                    &mut sources_ctx,
                    &mut output_ctx,
                    &mut stream_ctx,
                    &mut control_ctx,
                    &mut theme_request,
                );
            });
            if let Some(name) = preset_load_request {
                // Same pipeline as keyboard navigation and playlist/beat-sync
                // advances (`take_fired_presets`, above): never a direct
                // `Deck::load_preset` call, which would bypass the pre-flight
                // validation Step 14 added.
                request_preset_load(state, state.show.selected_slot, name);
            }
            if let Some(new_id) = theme_request {
                // `state.egui_glow`, not the local `egui_glow` binding from
                // the destructure above (its last use is `egui_glow.run(..)`
                // ahead of `preset_load_request`'s handling, which already
                // needs `state` whole again for `request_preset_load`):
                // same reborrow-after-`state`-is-whole-again idiom that
                // block uses for `state.show`.
                //
                // Step 12: same idiom as bootstrap's own Step 6 wiring
                // (`set_style_of` on both `egui::Theme::Dark` and `Light`
                // with the same `Arc<Style>`): `set_fonts` is deliberately
                // not re-called here, only ever at bootstrap.
                let new_theme = theme::registry::get(new_id);
                let new_style = Arc::new(theme::visuals::style(new_theme));
                state.egui_glow.egui_ctx.set_style_of(egui::Theme::Dark, new_style.clone());
                state.egui_glow.egui_ctx.set_style_of(egui::Theme::Light, new_style);
                // Ruling B (controller pre-flight analysis): also refresh
                // THEME_ID_KEY in ctx.data, the same key Step 6's bootstrap
                // wiring wrote: `set_style_of` alone only re-themes native
                // egui controls; `ui::widgets::theme(ui)` (Step 8) reads
                // this key for every custom-painted helper (badges, pills,
                // the crossfader, the nav rail), which would otherwise keep
                // rendering the old theme's palette after this switch.
                state.egui_glow.egui_ctx.data_mut(|d| d.insert_temp(egui::Id::new(theme::THEME_ID_KEY), new_id));

                // Persist immediately (Step 7's `config.rs`): read-modify-
                // write just the `theme` field so this doesn't clobber
                // `active_panel`/`stage_mode`/the other already-persisted
                // fields, which are wired at other call sites (bootstrap
                // and `App::exiting`, "Whole-branch review fix wave,
                // finding 1 (AC-10)").
                let config_path = config::config_file_path();
                let mut ui_config = config_path.as_deref().map(config::load_config).unwrap_or_default();
                ui_config.theme = new_id;
                config::save_config(config_path.as_deref(), &ui_config);
            }

            // Two windows, one context: each surface is made current in
            // turn. Skipping render+swap for an Occluded(true) window is
            // load-bearing on Wayland: see the DontWait/WaitUntil comment
            // in bootstrap().
            if let Err(e) = state.control.render_and_swap_with_egui(
                &state.main_ctx,
                &state.gl,
                &state.compositor,
                &mut state.blit_control_timer,
                &mut state.egui_glow,
            ) {
                eprintln!("[app] control window render failed: {e}");
            }
            if let Err(e) =
                state.output.render_and_swap(&state.main_ctx, &state.gl, &state.compositor, &mut state.blit_output_timer)
            {
                eprintln!("[app] output window render failed: {e}");
            }
            // Wall-clock swap-to-swap time is the ground truth for frame
            // time: the GPU pass timers below measure execution time in
            // their own context and never sum into this, since passes
            // across contexts can overlap on real hardware.
            let swap_now = Instant::now();
            let wall_ms = state.last_output_swap_at.map(|prev| (swap_now - prev).as_secs_f64() * 1000.0);
            state.last_output_swap_at = Some(swap_now);
            // Status bar's fps/frame-ms readout (Step 10): same one-frame
            // staleness as `last_output_swap_at` above, by construction.
            state.last_wall_ms = wall_ms;

            state.perf_tick += 1;
            if state.perf_tick % 60 == 0 {
                // Minor #21: same expression as `lowest_active` above,
                // computed once and reused instead of twice.
                let active = lowest_active.unwrap_or(0);
                let fmt = |v: Option<f64>| v.map(|ms| format!("{ms:.3}ms")).unwrap_or_else(|| "n/a".to_string());
                println!(
                    "[timing] deck{active} render={} copy={} | composite={} | blit control={} output={} | wall(swap-to-swap)={}",
                    fmt(state.decks[active].render_ms()),
                    fmt(state.decks[active].copy_ms()),
                    fmt(state.compositor.composite_ms()),
                    fmt(state.blit_control_timer.last_ms()),
                    fmt(state.blit_output_timer.last_ms()),
                    fmt(wall_ms),
                );
            }

            state.next_frame_at += state.refresh_interval;
            if state.next_frame_at < now {
                state.next_frame_at = now + state.refresh_interval; // fell behind; resync instead of catching up frame-by-frame
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(state.next_frame_at));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &mut self.state {
            // Whole-branch review fix wave, finding 1 (AC-10): persist
            // `active_panel`/`stage_mode` on exit, matching Step 7's
            // original "How" text ("écriture sur exiting ... et
            // immédiatement à chaque changement de thème runtime"). Same
            // read-modify-write idiom as the runtime theme-switch handler,
            // so this never clobbers `theme` (or any other already-
            // persisted field) saved elsewhere. Also owns the 8 already-
            // UI-controlled fields below, not just `active_panel`/
            // `stage_mode`.
            let config_path = config::config_file_path();
            let mut ui_config = config_path.as_deref().map(config::load_config).unwrap_or_default();
            ui_config.active_panel = state.active_panel.into();
            ui_config.stage_mode = state.stage_mode;
            ui_config.output_monitor = state.selected_output_monitor.clone();
            ui_config.audio_input_device = state.selected_input_device.clone();
            ui_config.osc_port = state.osc_port;
            ui_config.obs_host = state.obs_host.clone();
            ui_config.obs_port = state.obs_port;
            ui_config.twitch_channel = state.twitch_channel.clone();
            ui_config.kick_channel = state.kick_channel.clone();
            ui_config.invisible_mode = state.invisible_mode;
            config::save_config(config_path.as_deref(), &ui_config);

            state.egui_glow.destroy();
        }
    }
}

/// Builds one window + the EGL Display/Config it negotiated, via
/// glutin-winit's DisplayBuilder (the only way to get the first window and
/// the Config in one negotiation).
fn bootstrap_display(event_loop: &ActiveEventLoop, attrs: WindowAttributes) -> Result<(Window, Config), String> {
    let template = ConfigTemplateBuilder::new()
        .with_api(Api::OPENGL)
        .with_surface_type(ConfigSurfaceTypes::WINDOW | ConfigSurfaceTypes::PBUFFER)
        .with_alpha_size(8)
        .with_depth_size(0)
        .with_stencil_size(0);

    let (window, gl_config) = DisplayBuilder::new()
        .with_window_attributes(Some(attrs))
        .build(event_loop, template, |mut configs| {
            // DisplayBuilder's picker callback must return a Config, not a
            // Result: an empty match here means the template's constraints
            // (see above) can't be satisfied on this driver at all.
            configs.next().expect("EGL returned zero configs matching the WINDOW|PBUFFER/OpenGL/alpha8/depth0/stencil0 template")
        })
        .map_err(|e| format!("failed to bootstrap EGL display/config: {e}"))?;
    let window = window.ok_or_else(|| "DisplayBuilder did not create the requested window".to_string())?;
    Ok((window, gl_config))
}

fn create_window_slot(display: &Display, gl_config: &Config, window: Window) -> Result<WindowSlot, String> {
    let raw_window_handle = window
        .window_handle()
        .map_err(|e| format!("window has no raw handle: {e}"))?
        .as_raw();
    let size = window.inner_size();
    let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(size.width.max(1)).expect("width.max(1) is nonzero"),
        NonZeroU32::new(size.height.max(1)).expect("height.max(1) is nonzero"),
    );
    let surface = unsafe { display.create_window_surface(gl_config, &surface_attrs) }
        .map_err(|e| format!("failed to create window surface: {e}"))?;
    Ok(WindowSlot {
        window,
        surface,
        size: (size.width.max(1), size.height.max(1)),
        occluded: false,
    })
}

fn preset_dir() -> Result<PathBuf, String> {
    preset_dir_from(
        std::env::var_os("OPENDROP_PRESET_DIR"),
        std::env::var_os("APPDIR"),
        std::env::current_exe().ok(),
    )
}

/// The env-reading half of `preset_dir`, split out so the fallback order is
/// testable without mutating process-global environment state.
///
/// Resolution order: `OPENDROP_PRESET_DIR` wins unconditionally if set; then
/// a platform-specific packaged location, only if it actually exists on
/// disk; else the same error `cargo run` has always raised for a missing
/// dev override.
fn preset_dir_from(
    env_override: Option<OsString>,
    appdir: Option<OsString>,
    exe_path: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(raw) = env_override {
        return Ok(PathBuf::from(raw));
    }

    // Linux: AppImage's own runtime sets APPDIR at launch (not something
    // this repo sets). Must match the layout Step 6 creates in the AppDir.
    if let Some(appdir) = appdir {
        let candidate = PathBuf::from(appdir).join("usr/share/opendrop/presets");
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }

    // Windows: sibling-of-exe, flat layout (no `usr/bin`/`usr/share`
    // AppImage-style structure). Must match the layout Step 15 creates in
    // the portable zip.
    #[cfg(target_os = "windows")]
    if let Some(exe_path) = exe_path {
        if let Some(exe_dir) = exe_path.parent() {
            let candidate = exe_dir.join("presets");
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = exe_path;

    // macOS packaged fallback: not implemented, no build/test machine
    // available (see PHASE6-PACKAGING.REQUIREMENTS.md)

    Err(
        "OPENDROP_PRESET_DIR is not set. Point it at a directory of .milk presets, e.g.:\n  \
         OPENDROP_PRESET_DIR=/srv/http/opendrop-presets cargo run -p opendrop-app"
            .to_string(),
    )
}

/// Display name for a preset file: its path relative to `preset_dir`, with
/// the extension stripped and `/` replaced by ` - `. Stable and
/// manifest-free, and it doubles as the categorization key
/// `category_from_name` expects. The full catalog scan and the 4-deck
/// bootstrap load both derive names through here, so a deck card shows
/// exactly the name the preset browser lists for the same file.
fn preset_display_name(preset_dir: &Path, path: &Path) -> String {
    path.strip_prefix(preset_dir).unwrap_or(path).with_extension("").to_string_lossy().replace('/', " - ")
}

/// Disk-cache root for rendered preset thumbnails:
/// `$XDG_CACHE_HOME/opendrop/thumbnails`, falling back to
/// `$HOME/.cache/opendrop/thumbnails`. Deliberately not
/// `std::env::temp_dir()`: a fixed, world-predictable `/tmp` path can be
/// pre-created by another user as a symlink pointing anywhere, and it is
/// not multi-user-safe on a shared machine either. A non-absolute
/// `XDG_CACHE_HOME` is ignored, as the XDG spec requires.
fn thumbnail_cache_dir() -> PathBuf {
    thumbnail_cache_dir_from(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))
}

/// The env-reading half of `thumbnail_cache_dir`, split out so the fallback
/// order is testable without mutating process-global environment state.
fn thumbnail_cache_dir_from(xdg_cache_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    let xdg = xdg_cache_home.map(PathBuf::from);
    let home_cache = home.map(PathBuf::from).map(|h| h.join(".cache"));
    xdg.into_iter()
        .chain(home_cache)
        .find(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir)
        .join("opendrop")
        .join("thumbnails")
}

fn walk_milk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk_milk_files(&p));
            } else if p.extension().map(|e| e.eq_ignore_ascii_case("milk")).unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out
}

/// Picks up to `count` visually distinct presets: one per top-level
/// category subdirectory where possible: so the 4 decks don't all end up
/// on lookalike presets. Skips transition-only presets (near-static/black
/// by design, a bad default for eyeballing that rendering actually works),
/// same skip the earlier prototype applied when picking its one preset.
fn pick_distinct_presets(dir: &Path, count: usize) -> Vec<PathBuf> {
    let mut categories: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    categories.sort();

    let mut picks = Vec::with_capacity(count);
    for cat in &categories {
        if picks.len() >= count {
            break;
        }
        let mut files = walk_milk_files(cat);
        files.sort();
        if let Some(p) = files.into_iter().find(|p| !p.to_string_lossy().contains("Transition")) {
            picks.push(p);
        }
    }
    if picks.len() < count {
        let mut all = walk_milk_files(dir);
        all.sort();
        for p in all {
            if picks.len() >= count {
                break;
            }
            if !picks.contains(&p) && !p.to_string_lossy().contains("Transition") {
                picks.push(p);
            }
        }
    }
    picks
}

/// Step 3 of Phase 2: two windows sharing one GL context, paced explicitly
/// instead of relying on vsync. See `piped-rolling-sunrise.md` step 3: a
/// Wayland surface that stops being visible stops receiving frame
/// callbacks, so `SwapInterval::Wait` on it would block `swap_buffers` and
/// freeze the whole single-threaded render loop, output window included.
fn bootstrap(event_loop: &ActiveEventLoop) -> Result<AppState, String> {
    let control_attrs = Window::default_attributes()
        .with_title("OpenDrop: control")
        .with_transparent(false);
    let (control_window, gl_config) = bootstrap_display(event_loop, control_attrs)?;
    let display = gl_config.display();

    let output_attrs = Window::default_attributes()
        .with_title("OpenDrop: output")
        .with_transparent(false);
    let output_window = glutin_winit::finalize_window(event_loop, output_attrs, &gl_config)
        .map_err(|e| format!("failed to create output window: {e}"))?;

    let raw_window_handle = control_window
        .window_handle()
        .map_err(|e| format!("control window has no raw handle: {e}"))?
        .as_raw();
    let ctx_attrs = ContextAttributesBuilder::new()
        .with_debug(cfg!(debug_assertions))
        .with_profile(GlProfile::Core)
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(raw_window_handle));

    // Anchor context: created here, converted to PossiblyCurrent, but not
    // actually made current yet. engine::deck::create_decks creates its 4
    // contexts sharing this anchor's namespace: EGL allows creating a
    // share-group context regardless of whether a sibling is already
    // current, so there's no ordering requirement here (confirmed live on
    // real GPU hardware; see create_one_deck_context's doc comment).
    let not_current_main = unsafe { display.create_context(&gl_config, &ctx_attrs) }
        .map_err(|e| format!("failed to create main GL context: {e}"))?;
    let main_ctx = not_current_main.treat_as_possibly_current();

    let decks = deck::create_decks(&display, &gl_config, &main_ctx)?;

    // No thumbnail context here. Preset thumbnails used to get a 6th
    // in-process GL context of their own; they are now rendered in a
    // `--render-thumbnail` child process (see `thumbnails`' module doc), so
    // this process holds exactly the 4 deck contexts plus the main one.

    let preset_root = preset_dir()?;
    let presets = pick_distinct_presets(&preset_root, deck::DECK_COUNT);
    if presets.len() < deck::DECK_COUNT {
        return Err(format!(
            "found only {} distinct, non-transition preset(s) under OPENDROP_PRESET_DIR, need {}",
            presets.len(),
            deck::DECK_COUNT
        ));
    }
    for (i, dk) in decks.iter().enumerate() {
        dk.context.make_current(&dk.surface).map_err(|e| format!("make_current(deck {i}) failed: {e}"))?;
        dk.load_preset(&presets[i], false)?;
        println!("[app] deck {i} preset: {}", presets[i].display());
    }

    // Full catalog scan (all presets, not just the 4 bootstrap picks above):
    // builds the name→path lookup for resolving UI selections back to a
    // file, plus the Vec<PresetMeta> (name + category) shown in the preset
    // browser. Names come from `preset_display_name`, the same helper the
    // 4 bootstrap picks above are named through.
    let all_milk = walk_milk_files(&preset_root);
    let mut path_by_name: HashMap<String, PathBuf> = HashMap::with_capacity(all_milk.len());
    let mut catalog: Vec<opendrop_core::preset_index::PresetMeta> = Vec::with_capacity(all_milk.len());
    for p in &all_milk {
        let name = preset_display_name(&preset_root, p);
        let category = opendrop_core::preset_index::category_from_name(&name);
        catalog.push(opendrop_core::preset_index::PresetMeta { name: name.clone(), category });
        path_by_name.insert(name, p.clone());
    }
    catalog.sort_by(|a, b| a.name.cmp(&b.name));

    let refresh_millihertz = control_window
        .current_monitor()
        .and_then(|m| m.refresh_rate_millihertz())
        .unwrap_or(FALLBACK_REFRESH_MILLIHERTZ);
    let refresh_interval = Duration::from_secs_f64(1000.0 / refresh_millihertz as f64);

    let control = create_window_slot(&display, &gl_config, control_window)?;
    let output = create_window_slot(&display, &gl_config, output_window)?;

    // set_swap_interval must run while its own surface is current, else EGL
    // applies it to whichever surface happens to be current instead.
    main_ctx.make_current(&control.surface).map_err(|e| format!("make_current(control) failed: {e}"))?;
    control
        .surface
        .set_swap_interval(&main_ctx, SwapInterval::DontWait)
        .map_err(|e| format!("set_swap_interval(control) failed: {e}"))?;

    let mut gl = unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };
    if cfg!(debug_assertions) {
        opendrop_engine::gl_debug::install(&mut gl, "main");
    }
    // Arc, not an owned glow::Context: EguiGlow::new (below) requires
    // shared ownership. The 4 Deck::gl contexts stay owned, unshared: see
    // engine/src/deck.rs.
    let gl = Arc::new(gl);
    let version = unsafe { gl.get_parameter_string(glow::VERSION) };
    println!("[app] main context: GL {version}");

    // shader_version=None (auto-detect), native_pixels_per_point=None (no
    // forced ratio), dithering=true: same as egui_glow's own example
    // (examples/pure_glow.rs:188).
    let mut egui_glow = egui_glow::EguiGlow::new(event_loop, Arc::clone(&gl), None, None, true);

    // Whole-branch review fix wave, finding 1 (AC-10): load the persisted
    // `ui.json` once, here, before the theme/active_panel/stage_mode it
    // carries are wired onto the Context/AppState below: Step 7's `config.
    // rs` landed with this load call deferred (Ruling D: `AppState` didn't
    // have `active_panel`/`stage_mode` yet), and that wiring was never
    // revisited once Steps 10/11 added them. Without this, the runtime
    // theme-switch handler's `config::save_config` call was writing a
    // preference file the app then ignored on every restart.
    let ui_config = config::config_file_path().map(|p| config::load_config(&p)).unwrap_or_default();

    // Step 6 (Phase 7 UI redesign plan): wire the persisted (or default)
    // theme + fonts onto the bootstrap Context, once. `set_fonts` is only
    // ever called here: a runtime theme change (Step 12) re-applies
    // `set_style_of` but never re-registers fonts. `set_style_of` is applied
    // to both `Theme::Dark` and `Theme::Light` with the same `Arc<Style>` so
    // no internal egui route (tooltips, `Area`, anything reading
    // `ctx.global_style()`) is left on an unthemed style even if
    // `theme_preference` changes later.
    egui_glow.egui_ctx.set_fonts(theme::fonts::font_definitions());
    egui_glow.egui_ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);
    let default_theme_id = ui_config.theme;
    let default_theme = theme::registry::get(default_theme_id);
    let default_style = Arc::new(theme::visuals::style(default_theme));
    egui_glow.egui_ctx.set_style_of(egui::Theme::Dark, default_style.clone());
    egui_glow.egui_ctx.set_style_of(egui::Theme::Light, default_style);
    // Also record the active ThemeId in ctx.data (see `theme::THEME_ID_KEY`)
    // for Step 8's widgets.rs and Step 12, which resolve theme tokens for
    // custom-painted widgets not driven by egui's `Style` directly.
    egui_glow.egui_ctx.data_mut(|d| d.insert_temp(egui::Id::new(theme::THEME_ID_KEY), default_theme_id));

    // Register each deck's live GPU texture with egui's painter once, here
    // at bootstrap: never per-frame, which would leak a new texture handle
    // into the painter every tick. `register_native_texture` touches no GL
    // state and is safe to call at any time (egui_glow 0.36.1
    // src/painter.rs:649-655). `glow::Texture` is a type alias that
    // resolves to `glow::NativeTexture` for a native (non-wasm) `glow::
    // Context` (glow 0.17 src/native.rs:205), the same type as
    // `Deck::texture`, so it passes through directly with no `.0`.
    let deck_tex_ids: [egui::TextureId; 4] =
        std::array::from_fn(|i| egui_glow.painter.register_native_texture(decks[i].texture));

    // Compositor FBO/texture belong to whichever context is current at
    // creation: main_ctx is current here (on control's surface), same as
    // it will be every time the compositor's FBO is touched later.
    let compositor = Compositor::new(&gl)?;
    let blit_control_timer = PassTimer::new(&gl).map_err(|e| format!("blit_control_timer: {e}"))?;
    let blit_output_timer = PassTimer::new(&gl).map_err(|e| format!("blit_output_timer: {e}"))?;

    // Step 5: one FrameReadback per shared texture (compositor + 4 decks).
    // Built here, main_ctx still current on control's surface: FrameReadback::
    // new must run on the main context, the only one that sees every texture
    // in the share group (decks included, already created above). Sized off
    // the same COMP_W/COMP_H and DECK_W/DECK_H the textures themselves were
    // allocated at, never hardcoded.
    let compositor_readback = FrameReadback::new(&gl, compositor.color_tex, COMP_W, COMP_H)?;
    let deck_readback: [FrameReadback; deck::DECK_COUNT] = {
        let mut v = Vec::with_capacity(deck::DECK_COUNT);
        for dk in &decks {
            v.push(FrameReadback::new(&gl, dk.texture, deck::DECK_W, deck::DECK_H)?);
        }
        v.try_into().unwrap_or_else(|_| unreachable!("DECK_COUNT readbacks pushed"))
    };

    // Output channels for the readback bytes above. `compositor_frame_rx`/
    // `deck_frame_rx` are moved into the NDI thread below; `v4l2_frame_rx`
    // is a second, v4l2loopback-only channel for the same compositor bytes
    // (Task 19: see `v4l2_frame_tx`'s doc comment on `AppState`).
    let (compositor_frame_tx, compositor_frame_rx) = mpsc::channel::<Vec<u8>>();
    let (v4l2_frame_tx, v4l2_frame_rx) = mpsc::channel::<Vec<u8>>();
    type DeckFrameChannels = ([mpsc::Sender<Vec<u8>>; deck::DECK_COUNT], [mpsc::Receiver<Vec<u8>>; deck::DECK_COUNT]);
    let (deck_frame_tx, deck_frame_rx): DeckFrameChannels = {
        let mut tx_v = Vec::with_capacity(deck::DECK_COUNT);
        let mut rx_v = Vec::with_capacity(deck::DECK_COUNT);
        for _ in 0..deck::DECK_COUNT {
            let (tx, rx) = mpsc::channel::<Vec<u8>>();
            tx_v.push(tx);
            rx_v.push(rx);
        }
        (
            tx_v.try_into().unwrap_or_else(|_| unreachable!("DECK_COUNT senders pushed")),
            rx_v.try_into().unwrap_or_else(|_| unreachable!("DECK_COUNT receivers pushed")),
        )
    };

    main_ctx.make_current(&output.surface).map_err(|e| format!("make_current(output) failed: {e}"))?;
    output
        .surface
        .set_swap_interval(&main_ctx, SwapInterval::DontWait)
        .map_err(|e| format!("set_swap_interval(output) failed: {e}"))?;

    // Cross-context share-group proof: a texture created while the main
    // context was current must be visible (glIsTexture) from a deck
    // context: that's the whole point of sharing. Checked against deck
    // context index 3, the last of the 4 (5th context overall, after main).
    let probe_tex = unsafe {
        let tex = gl.create_texture().map_err(|e| format!("glGenTextures failed on main context: {e}"))?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            4,
            4,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        tex
    };
    let visible_from_deck3 = unsafe { decks[3].gl.is_texture(probe_tex) };
    println!("[app] texture created in main context, visible from deck context 4: {visible_from_deck3}");
    if !visible_from_deck3 {
        return Err("share group broken: texture created in the main context is not visible from deck context 4".to_string());
    }

    let (preflight_tx, preflight_rx) = mpsc::channel();
    // One shared channel for both Twitch and Kick chat messages, mirroring
    // `broadcastChatMessage` fanning both platforms into one function
    // (`main.cjs:425-429`: see `opendrop_io::chat`'s module doc comment).
    // `chat_tx` is cloned once per platform thread below; `chat_events` is
    // the receiving end, stored on `AppState`.
    let (chat_tx, chat_events) = mpsc::channel();

    let mut show = Show::default();
    // Whole-branch review Finding I4: `core` is zero-I/O and has no clock of
    // its own, so its playlist engines start from a fixed, hardcoded RNG
    // seed: shuffle mode would otherwise replay the exact same sequence
    // every single app launch. Real per-launch entropy is supplied here,
    // the one place in the codebase allowed to touch a wall clock for this.
    let bootstrap_rng_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    show.reseed_rng(bootstrap_rng_seed);
    show.preset_catalog = catalog;

    Ok(AppState {
        display,
        main_ctx,
        control,
        output,
        decks,
        compositor,
        compositor_readback,
        deck_readback,
        compositor_frame_tx,
        v4l2_frame_tx,
        deck_frame_tx,
        // Receivers moved into the NDI thread here, once: they are no
        // longer needed as `AppState` fields once `spawn` takes ownership.
        ndi: {
            let handle = opendrop_io::ndi::spawn(compositor_frame_rx, deck_frame_rx);
            // Task 12: discovery is started once, here, rather than behind a
            // manual "Scan" button: it's a cheap, non-blocking poll on the
            // NDI thread's own tick (`in_::find` with `timeout_ms = 0`, see
            // that function's doc comment), and the source dropdown should
            // just work whenever the NDI panel is opened, with no extra step.
            let _ = handle.control_tx.send(opendrop_io::ndi::NdiControl::StartDiscovery);
            handle
        },
        ndi_composite_active: false,
        ndi_deck_active: [false; deck::DECK_COUNT],
        ndi_in_texture: None,
        ndi_in_selected_source: None,
        v4l2_active: false,
        v4l2: opendrop_io::v4l2loopback::spawn(v4l2_frame_rx),
        v4l2_device: None,
        gl,
        egui_glow,
        refresh_interval,
        next_frame_at: Instant::now(),
        t0: Instant::now(),
        audio: opendrop_audio::spawn_capture(),
        // Step 19: enumerated once here at bootstrap and cached; the Audio
        // panel never calls `list_input_devices()` itself, per the brief
        // ("the list doesn't change mid-session").
        input_devices: opendrop_audio::list_input_devices(),
        // Panel settings restored from the same `ui_config` loaded above,
        // not just navigation state.
        selected_input_device: ui_config.audio_input_device.clone(),
        last_vu_level: 0.0,
        deck_next_render_at: [Instant::now(); deck::DECK_COUNT],
        invisible_mode: ui_config.invisible_mode,
        pending_mesh_size: [None; deck::DECK_COUNT],
        show,
        registry: create_default_registry(),
        keymap: keymap::default_keymap(),
        blit_control_timer,
        blit_output_timer,
        last_output_swap_at: None,
        last_wall_ms: None,
        perf_tick: 0,
        preflight_tx,
        preflight_rx,
        path_by_name,
        pending_validations: HashSet::new(),
        preset_errors: HashMap::new(),
        // Seeded from the 4 presets the bootstrap loop above actually
        // loaded: leaving these empty until the first UI-driven load meant
        // every deck card started blank even though a preset was running
        // on it.
        deck_preset_names: std::array::from_fn(|i| preset_display_name(&preset_root, &presets[i])),
        transition_seconds: 0.0,
        deck_tex_ids,
        // Whole-branch review fix wave, finding 1 (AC-10): restored from
        // the same `ui_config` loaded above, instead of always starting on
        // the hardcoded defaults.
        active_panel: ui_config.active_panel.into(),
        stage_mode: ui_config.stage_mode,
        presets_drawer_open: false,
        preset_search_query: String::new(),
        preset_search_cache: ui::preset_browser::SearchCache::default(),
        favorite_presets: ui_config.favorite_presets.clone(),
        favorites_only: false,
        failed_thumbnails: HashSet::new(),
        thumb_queue: Vec::new(),
        thumbnail_textures: HashMap::new(),
        thumbnail_order: VecDeque::new(),
        thumbnail_in_flight: None,
        thumbnail_killed: Vec::new(),
        thumbnail_cache_dir: thumbnail_cache_dir(),
        selected_output_monitor: ui_config.output_monitor.clone(),
        midi: opendrop_io::midi::spawn(),
        midi_led_state: HashMap::new(),
        midi_learning: None,
        midi_crossfader_taken_over: false,
        midi_last_hotplug_epoch: 0,
        midi_last_beat_count: 0,
        midi_led_flash_off_at: HashMap::new(),
        osc: opendrop_io::osc::spawn(),
        osc_port: ui_config.osc_port,
        remote_ws: opendrop_io::remote_ws::spawn(),
        obs: opendrop_io::obs::spawn(),
        obs_host: ui_config.obs_host.clone(),
        obs_port: ui_config.obs_port,
        twitch: opendrop_io::twitch::spawn(chat_tx.clone()),
        twitch_channel: ui_config.twitch_channel.clone(),
        twitch_oauth_token_input: String::new(),
        kick: opendrop_io::kick::spawn(chat_tx),
        kick_channel: ui_config.kick_channel.clone(),
        kick_bearer_token_input: String::new(),
        kick_xsrf_token_input: String::new(),
        kick_cookies_input: String::new(),
        #[cfg(feature = "link")]
        link: opendrop_io::link::spawn(),
        #[cfg(feature = "link")]
        link_tempo_input: 120.0,
        chat_events,
        chat_log: VecDeque::new(),
        streaming_secret_save_error: None,
    })
}

fn main() {
    // Hidden subcommands, both of them this same binary re-invoked as a
    // child process by the parent app, both of them `-> !`. Neither is a
    // user-facing CLI: the parent builds these argv lines itself, and the
    // whole result protocol is the exit code (plus, for --render-thumbnail,
    // the file it wrote).
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--preflight-check" {
        preflight::run_preflight_check(Path::new(&args[2]));
    }
    if args.len() >= 4 && args[1] == "--render-thumbnail" {
        thumbnail_child::run_render_thumbnail(Path::new(&args[2]), Path::new(&args[3]));
    }

    let event_loop = EventLoop::new().expect("failed to create winit event loop");
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("event loop exited with an error");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whole-branch review Finding 3: real elapsed-time dt for beat-sync/
    /// playlist ticking.
    mod compute_tick_dt_tests {
        use super::*;

        #[test]
        fn first_tick_uses_nominal_dt() {
            let now = Instant::now();
            let dt = compute_tick_dt(now, None, Duration::from_millis(16), Duration::from_millis(100));
            assert!((dt - 0.016).abs() < 1e-6);
        }

        #[test]
        fn measures_real_elapsed_time_not_nominal() {
            let prev = Instant::now();
            let now = prev + Duration::from_millis(25);
            // Nominal would be 16ms (60fps), but the tick actually took
            // 25ms (fell behind): the real, measured value has to win.
            let dt = compute_tick_dt(now, Some(prev), Duration::from_millis(16), Duration::from_millis(100));
            assert!((dt - 0.025).abs() < 1e-6);
        }

        #[test]
        fn clamps_a_long_stall_to_max_dt() {
            let prev = Instant::now();
            let now = prev + Duration::from_secs(2);
            let dt = compute_tick_dt(now, Some(prev), Duration::from_millis(16), Duration::from_millis(100));
            assert_eq!(dt, 0.1);
        }

        #[test]
        fn exactly_at_the_clamp_boundary_is_not_reduced() {
            let prev = Instant::now();
            let now = prev + Duration::from_millis(100);
            let dt = compute_tick_dt(now, Some(prev), Duration::from_millis(16), Duration::from_millis(100));
            assert_eq!(dt, 0.1);
        }
    }

    /// Whole-branch review Finding 1/2: the preset-load dedup/backpressure
    /// guard.
    mod should_spawn_preflight_tests {
        use super::*;

        fn catalog() -> HashMap<String, PathBuf> {
            HashMap::from([("Preset A".to_string(), PathBuf::from("/presets/a.milk"))])
        }

        #[test]
        fn first_request_for_a_slot_is_allowed_and_marks_it_pending() {
            let mut pending = HashSet::new();
            let path = should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A");
            assert_eq!(path, Some(PathBuf::from("/presets/a.milk")));
            assert!(pending.contains(&0));
        }

        #[test]
        fn a_second_request_for_the_same_in_flight_slot_is_a_no_op() {
            // Regression: holding a key bound to PresetNextActive/
            // PlaylistNextActive auto-repeats at OS rate and used to spawn
            // a fresh preflight child on every repeat.
            let mut pending = HashSet::new();
            assert!(should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A").is_some());
            assert_eq!(should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A"), None);
            assert_eq!(pending, HashSet::from([0]));
        }

        #[test]
        fn a_different_slot_is_independent() {
            let mut pending = HashSet::new();
            assert!(should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A").is_some());
            assert!(should_spawn_preflight(&catalog(), &mut pending, 1, "Preset A").is_some());
            assert_eq!(pending, HashSet::from([0, 1]));
        }

        #[test]
        fn an_unknown_preset_name_is_a_no_op_and_never_marks_the_slot_pending() {
            let mut pending = HashSet::new();
            assert_eq!(should_spawn_preflight(&catalog(), &mut pending, 0, "Nonexistent"), None);
            assert!(pending.is_empty());
        }

        #[test]
        fn once_the_slot_is_cleared_a_new_request_is_allowed_again() {
            // Mirrors about_to_wait's verdict-drain: `pending_validations.
            // remove(&slot)` runs once the real verdict comes back.
            let mut pending = HashSet::from([0]);
            pending.remove(&0);
            assert!(should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A").is_some());
        }
    }

    mod midi_learn_completed_tests {
        use super::*;
        use opendrop_io::midi::TriggerKind;

        fn key(number: u8) -> MidiTriggerKey {
            MidiTriggerKey { device_id: "dev".to_string(), kind: TriggerKind::Cc, channel: 1, number }
        }

        #[test]
        fn unmapped_to_freshly_mapped_is_complete() {
            assert!(midi_learn_completed(None, Some(&key(1))));
        }

        #[test]
        fn still_unmapped_is_not_complete() {
            assert!(!midi_learn_completed(None, None));
        }

        #[test]
        fn remapping_and_still_seeing_the_old_entry_is_not_complete() {
            // Regression: StartLearn doesn't clear the pre-existing mapping
            // entry, so the snapshot briefly still holds the OLD trigger
            // while the thread waits for the next MIDI message.
            let old = key(1);
            assert!(!midi_learn_completed(Some(&old), Some(&old)));
        }

        #[test]
        fn remapping_to_a_different_trigger_is_complete() {
            assert!(midi_learn_completed(Some(&key(1)), Some(&key(2))));
        }
    }

    mod preset_display_name_tests {
        use super::*;

        #[test]
        fn strips_the_preset_root_and_the_extension() {
            let name = preset_display_name(Path::new("/presets"), Path::new("/presets/Fractal/Blobby/306 nz+.milk"));
            assert_eq!(name, "Fractal - Blobby - 306 nz+");
        }

        #[test]
        fn a_file_directly_under_the_root_keeps_just_its_stem() {
            assert_eq!(preset_display_name(Path::new("/presets"), Path::new("/presets/Solo.milk")), "Solo");
        }

        #[test]
        fn a_path_outside_the_root_falls_back_to_the_whole_path() {
            let name = preset_display_name(Path::new("/presets"), Path::new("/elsewhere/Odd.milk"));
            assert_eq!(name, " - elsewhere - Odd");
        }
    }

    mod thumbnail_cache_dir_tests {
        use super::*;

        // Only used by the POSIX-only fixtures below; gated the same way
        // so it doesn't trip an unused-function warning on Windows.
        #[cfg(not(target_os = "windows"))]
        fn os(s: &str) -> Option<OsString> {
            Some(OsString::from(s))
        }

        // These three fixtures use POSIX-absolute literals (`/xdg`,
        // `/home/u`) to exercise the `.is_absolute()` branch in
        // `thumbnail_cache_dir_from`. `Path::is_absolute()` requires a
        // drive/UNC prefix on Windows, so a bare `/xdg` is NOT absolute
        // there and the fallback-to-temp_dir branch fires instead,
        // failing these assertions: a test-fixture limitation, not a
        // bug in the function (XDG_CACHE_HOME/HOME are POSIX-only
        // conventions to begin with; see the doc comment above).
        #[test]
        #[cfg(not(target_os = "windows"))]
        fn prefers_xdg_cache_home() {
            let dir = thumbnail_cache_dir_from(os("/xdg"), os("/home/u"));
            assert_eq!(dir, PathBuf::from("/xdg/opendrop/thumbnails"));
        }

        #[test]
        #[cfg(not(target_os = "windows"))]
        fn falls_back_to_home_dot_cache() {
            let dir = thumbnail_cache_dir_from(None, os("/home/u"));
            assert_eq!(dir, PathBuf::from("/home/u/.cache/opendrop/thumbnails"));
        }

        #[test]
        #[cfg(not(target_os = "windows"))]
        fn ignores_a_relative_xdg_cache_home() {
            let dir = thumbnail_cache_dir_from(os("relative/cache"), os("/home/u"));
            assert_eq!(dir, PathBuf::from("/home/u/.cache/opendrop/thumbnails"));
        }

        #[test]
        fn never_lands_directly_in_a_shared_tmp_root() {
            // Even the last-resort branch nests under its own subdirectory
            // rather than a bare, world-predictable path.
            let dir = thumbnail_cache_dir_from(None, None);
            assert!(dir.ends_with("opendrop/thumbnails"));
        }
    }

    mod preset_dir_from_tests {
        use super::*;

        fn os(s: &str) -> Option<OsString> {
            Some(OsString::from(s))
        }

        /// A fresh temp directory under `std::env::temp_dir()`, removed
        /// when dropped. No `tempdir`-style crate is in use elsewhere in
        /// this suite, so this is the plain-`std` equivalent.
        struct ScratchDir {
            path: PathBuf,
        }

        impl ScratchDir {
            fn new(unique: &str) -> Self {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system clock before UNIX epoch")
                    .as_nanos();
                let path = std::env::temp_dir().join(format!(
                    "opendrop-preset_dir_from_tests-{unique}-{}-{nanos}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&path).expect("create scratch dir");
                ScratchDir { path }
            }
        }

        impl Drop for ScratchDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }

        const MISSING_ENV_MESSAGE: &str = "OPENDROP_PRESET_DIR is not set. Point it at a directory of .milk presets, e.g.:\n  \
             OPENDROP_PRESET_DIR=/srv/http/opendrop-presets cargo run -p opendrop-app";

        #[test]
        fn env_override_wins_even_when_appdir_and_exe_path_also_exist() {
            let appdir = ScratchDir::new("env-wins-appdir");
            std::fs::create_dir_all(appdir.path.join("usr/share/opendrop/presets")).unwrap();
            let exe_dir = ScratchDir::new("env-wins-exe");
            std::fs::create_dir_all(exe_dir.path.join("presets")).unwrap();

            let dir = preset_dir_from(
                os("/explicit/override"),
                Some(appdir.path.clone().into_os_string()),
                Some(exe_dir.path.join("opendrop-app")),
            );
            assert_eq!(dir, Ok(PathBuf::from("/explicit/override")));
        }

        #[test]
        fn appdir_set_and_the_presets_subdir_exists_is_returned() {
            let appdir = ScratchDir::new("appdir-exists");
            let presets = appdir.path.join("usr/share/opendrop/presets");
            std::fs::create_dir_all(&presets).unwrap();

            let dir = preset_dir_from(None, Some(appdir.path.clone().into_os_string()), None);
            assert_eq!(dir, Ok(presets));
        }

        #[test]
        fn appdir_set_but_the_presets_subdir_is_missing_is_an_error() {
            let appdir = ScratchDir::new("appdir-missing-subdir");
            // appdir.path itself exists, but usr/share/opendrop/presets under
            // it does not: must never fall back to a silent bogus path.

            let dir = preset_dir_from(None, Some(appdir.path.clone().into_os_string()), None);
            assert_eq!(dir, Err(MISSING_ENV_MESSAGE.to_string()));
        }

        #[test]
        #[cfg(target_os = "windows")]
        fn windows_exe_with_a_sibling_presets_dir_is_returned() {
            let exe_dir = ScratchDir::new("windows-exe-sibling");
            let presets = exe_dir.path.join("presets");
            std::fs::create_dir_all(&presets).unwrap();

            let dir = preset_dir_from(None, None, Some(exe_dir.path.join("opendrop-app.exe")));
            assert_eq!(dir, Ok(presets));
        }

        #[test]
        #[cfg(target_os = "windows")]
        fn windows_exe_without_a_sibling_presets_dir_is_an_error() {
            let exe_dir = ScratchDir::new("windows-exe-no-sibling");

            let dir = preset_dir_from(None, None, Some(exe_dir.path.join("opendrop-app.exe")));
            assert_eq!(dir, Err(MISSING_ENV_MESSAGE.to_string()));
        }

        #[test]
        fn nothing_set_is_an_error_with_the_unchanged_dev_message() {
            // Regression coverage for the current `cargo run` dev behavior:
            // same message, now returned instead of panicking.
            let dir = preset_dir_from(None, None, None);
            assert_eq!(dir, Err(MISSING_ENV_MESSAGE.to_string()));
        }
    }

    /// Whole-branch review Finding 3: the LED-flash Trigger-kind gating.
    mod should_flash_led_tests {
        use super::*;

        #[test]
        fn trigger_kind_flashes() {
            assert!(should_flash_led(Some(CommandKind::Trigger)));
        }

        #[test]
        fn range_kind_does_not_flash() {
            // Regression: the previous "not a persistent-state command ->
            // flash" logic wrongly included Range-kind commands (faders/
            // knobs), sending a real MIDI LED write for every incoming CC
            // message: potentially hundreds per second.
            assert!(!should_flash_led(Some(CommandKind::Range)));
        }

        #[test]
        fn unknown_command_does_not_flash() {
            assert!(!should_flash_led(None));
        }
    }

    /// Whole-branch review Finding 5: the NDI-in drain-to-latest behavior.
    mod drain_to_latest_tests {
        use super::*;

        #[test]
        fn empty_channel_returns_none() {
            let (_tx, rx) = mpsc::channel::<i32>();
            assert_eq!(drain_to_latest(&rx), None);
        }

        #[test]
        fn keeps_only_the_most_recently_sent_item() {
            let (tx, rx) = mpsc::channel::<i32>();
            tx.send(1).unwrap();
            tx.send(2).unwrap();
            tx.send(3).unwrap();
            assert_eq!(drain_to_latest(&rx), Some(3));
            // Fully drained by the call above: nothing left for a second call.
            assert_eq!(drain_to_latest(&rx), None);
        }
    }

    /// Whole-branch review Finding 2: the chat-log ring-buffer capping.
    mod push_chat_message_tests {
        use super::*;
        use opendrop_io::chat::{ChatMessage, ChatPlatform};

        fn msg(content: &str) -> ChatMessage {
            ChatMessage {
                platform: ChatPlatform::Twitch,
                user_id: "1".to_string(),
                username: "someviewer".to_string(),
                content: content.to_string(),
            }
        }

        #[test]
        fn under_cap_keeps_everything_in_order() {
            let mut log = VecDeque::new();
            push_chat_message(&mut log, msg("a"), 5);
            push_chat_message(&mut log, msg("b"), 5);
            let contents: Vec<&str> = log.iter().map(|m| m.content.as_str()).collect();
            assert_eq!(contents, vec!["a", "b"]);
        }

        #[test]
        fn over_cap_drops_the_oldest_first() {
            let mut log = VecDeque::new();
            for i in 0..5 {
                push_chat_message(&mut log, msg(&i.to_string()), 3);
            }
            let contents: Vec<&str> = log.iter().map(|m| m.content.as_str()).collect();
            assert_eq!(contents, vec!["2", "3", "4"]);
        }
    }
}
