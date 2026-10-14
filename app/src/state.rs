use glutin::context::PossiblyCurrentContext;
use glutin::display::Display;
use opendrop_core::commands::{CommandId, CommandRegistry};
use opendrop_core::show::Show;
use opendrop_core::thumb_queue::ThumbJob;
use opendrop_engine::compositor::Compositor;
use opendrop_engine::deck::{self, Deck};
use opendrop_engine::readback::FrameReadback;
use opendrop_engine::timing::PassTimer;
use opendrop_io::midi::MidiTriggerKey;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use winit::keyboard::Key;
use crate::deck_video::RkbxSyncState;
use crate::overlays::OverlayTextureEntry;
use crate::preset_load::CHANNEL_PARAM_COUNT;
use crate::{preflight, thumbnails, ui, Panel, WindowSlot};

/// Power mode applied to invisible (opacity ≤ 0.001) decks, selected from
/// the Quality panel. `Eco` reproduces the original always-on
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

pub(crate) struct AppState {
    #[allow(dead_code)] // kept alive: dropping Display would invalidate every surface/context above
    pub(crate) display: Display,
    pub(crate) main_ctx: PossiblyCurrentContext,
    pub(crate) control: WindowSlot,
    pub(crate) output: WindowSlot,
    pub(crate) decks: Vec<Deck>,
    pub(crate) compositor: Compositor,
    /// GPU->CPU async pixel readback (`FrameReadback`) for the
    /// composite output and each deck's live texture, driven once per
    /// frame in `about_to_wait`. Composite is gated on `ndi_composite_active
    /// || v4l2_active` (either consumer needs the composite), each deck on
    /// that deck's own `ndi_deck_active[i]` only (v4l2 never needs per-deck
    /// readbacks). A single combined `ndi_active || v4l2_active` gate over
    /// all 5 readbacks used to make a v4l2-only session pay for 4 wasted
    /// deck `glReadPixels`+`to_vec()` calls every frame, and symmetrically
    /// made an NDI-composite-only session pay for v4l2's extra
    /// `bytes.clone()`.
    pub(crate) compositor_readback: FrameReadback,
    pub(crate) deck_readback: [FrameReadback; deck::DECK_COUNT],
    /// Outlet for `compositor_readback`'s polled RGBA bytes, feeding the NDI
    /// output thread's `compositor_rx` (its `Receiver` end was moved into
    /// `opendrop_io::ndi::spawn` at construction time; see `ndi`'s doc
    /// comment).
    pub(crate) compositor_frame_tx: mpsc::Sender<Vec<u8>>,
    /// Same compositor bytes as `compositor_frame_tx`, on a second channel
    /// pair feeding the v4l2loopback output thread's own `compositor_rx`:
    /// each consumer needs its own `Sender`, since the NDI output thread
    /// already moved the original single receiver into `opendrop_io::
    /// ndi::spawn`. Unlike `compositor_frame_tx`/`deck_frame_tx`, there is
    /// no per-deck counterpart: v4l2loopback only ever pipes the composite
    /// stream (see `io::v4l2loopback`'s module doc comment).
    pub(crate) v4l2_frame_tx: mpsc::Sender<Vec<u8>>,
    /// Same as `compositor_frame_tx`, one channel pair per deck.
    pub(crate) deck_frame_tx: [mpsc::Sender<Vec<u8>>; deck::DECK_COUNT],
    /// Handle to the dedicated NDI output thread, owns the
    /// `Receiver` ends of `compositor_frame_tx`/`deck_frame_tx` (handed to
    /// `opendrop_io::ndi::spawn` once, at construction time), and takes
    /// start/stop control messages from the NDI panel.
    pub(crate) ndi: opendrop_io::ndi::NdiHandle,
    /// The NDI panel's own composite toggle state (resynced from
    /// `NdiSnapshot::composite_active` each frame the panel is drawn; see
    /// `ui::ndi`'s doc comment). Directly
    /// gates the composite readback in `about_to_wait`.
    /// There's no separate recomputed aggregate any more: the previous
    /// `ndi_active` field, recomputed *after* the readback gate already
    /// consumed it, was one frame stale by construction; reading this
    /// field directly removes that extra staleness.
    pub(crate) ndi_composite_active: bool,
    /// Same as `ndi_composite_active`, one per deck: each element directly
    /// gates that deck's own readback.
    pub(crate) ndi_deck_active: [bool; deck::DECK_COUNT],
    /// Spout output (Windows): the sender thread and the panel's toggles.
    pub(crate) spout: opendrop_io::spout::SpoutHandle,
    pub(crate) spout_composite_active: bool,
    pub(crate) spout_deck_active: [bool; deck::DECK_COUNT],
    /// GL texture holding the most recently received NDI-in frame,
    /// composited as the topmost layer over the 4 decks. `None`
    /// until the first frame of an active receive arrives. Carries its own
    /// `(width, height)` alongside the texture handle so a same-size frame
    /// can `tex_sub_image_2d` in place, while a resolution change (or a
    /// freshly connected source) recreates it: NDI-in sources can be any
    /// resolution, unlike the fixed-size compositor/deck textures.
    pub(crate) ndi_in_texture: Option<(glow::NativeTexture, u32, u32)>,
    /// Source currently selected in the NDI panel's dropdown, same
    /// convention as `ndi_composite_active`: this is the panel's own
    /// toggle state, not `NdiSnapshot::receive_active` (which reflects
    /// whether the receiver actually started).
    pub(crate) ndi_in_selected_source: Option<opendrop_io::ndi::NdiSource>,
    /// Spout input (Windows): the receive thread, the texture its frames are
    /// uploaded into, and the sender picked in the panel (`None` follows the
    /// active sender).
    pub(crate) spout_in: opendrop_io::spout_in::SpoutInHandle,
    pub(crate) spout_in_texture: Option<(glow::NativeTexture, u32, u32)>,
    pub(crate) spout_in_selected: Option<String>,
    /// Whether `NdiControl::StartDiscovery` is the last message sent to the
    /// NDI thread (mirrors `ndi_composite_active`'s "own toggle state, not
    /// the thread's" convention): discovery only runs while the NDI-in
    /// panel is on screen (see the `about_to_wait` gate near the thumbnail
    /// pump), instead of for the whole session regardless of use. Tracked
    /// here so the gate below sends Start/Stop exactly once per transition
    /// rather than re-opening the SDK's `Finder` every tick while the panel
    /// stays open (`start_discovery` unconditionally recreates it).
    pub(crate) ndi_discovery_active: bool,
    /// Whether a v4l2loopback output consumer is active: this panel's own
    /// Start/Stop toggle, resynced from `V4l2Snapshot::running`
    /// each frame the panel is drawn (`ui::v4l2loopback::show`). There's no
    /// per-slot array to OR together, just the one
    /// stream, so no separate aggregate field is needed. Gates the
    /// composite readback in `about_to_wait` alongside `ndi_composite_active`.
    pub(crate) v4l2_active: bool,
    /// Handle to the dedicated v4l2loopback output thread, owns
    /// the `Receiver` end of `v4l2_frame_tx` (handed to `opendrop_io::
    /// v4l2loopback::spawn` once, at construction time), and takes
    /// start/stop control messages from the v4l2loopback panel.
    pub(crate) v4l2: opendrop_io::v4l2loopback::V4l2Handle,
    /// Lazily-resolved v4l2loopback device path, checked at most once per
    /// session: outer `None` means "not checked yet", `Some(None)` means
    /// "checked, no device found". Populated by `ui::v4l2loopback::show`
    /// the first frame that panel is shown; see that module's doc comment
    /// for why this is deliberately not re-queried per frame.
    pub(crate) v4l2_device: Option<Option<PathBuf>>,
    /// Handle to the dedicated video-capture thread, the input-side mirror
    /// of `v4l2` above: `latest()`
    /// gives the running/last-error snapshot, `latest_frame()` the newest
    /// decoded RGBA frame, `control_tx` sends Start/Stop/SetRate.
    pub(crate) video: opendrop_io::video_capture::VideoCaptureHandle,
    /// What the capture thread is currently decoding, or `None` while the
    /// layer is off. Compared each tick against `desired_video_input`'s
    /// answer, and the *only* thing that makes this app send a Start/Stop,
    /// so a beat-driven clip cut, a camera toggle, an NDI connect and the
    /// panel's on/off switch all converge on one restart path instead of
    /// four.
    pub(crate) video_input: Option<opendrop_io::video_capture::VideoInput>,
    /// GL texture holding the most recently decoded video frame, with its
    /// size: same shape and same "recreate on a resolution change" rule as
    /// `ndi_in_texture`, though in practice the capture pipeline pins every
    /// source to `CAPTURE_W`x`CAPTURE_H` so the recreate branch only ever
    /// runs on the first frame.
    pub(crate) video_texture: Option<(glow::NativeTexture, u32, u32)>,
    /// `VideoFrame::seq` of the frame already in `video_texture`. The
    /// capture thread publishes latest-wins into an `ArcSwap`, so without
    /// this the same 3.5 MB frame would be re-uploaded on every tick that
    /// outpaces the source's frame rate (i.e. most of them).
    pub(crate) video_frame_seq: u64,
    /// Per-deck mirror of `video` above: one dedicated video-capture
    /// thread per deck slot, so up to 4 clips can decode concurrently,
    /// independent of the global layer's own thread and of each other.
    pub(crate) deck_video_capture: [opendrop_io::video_capture::VideoCaptureHandle; 4],
    /// Per-slot mirror of `video_input`.
    pub(crate) deck_video_input: [Option<opendrop_io::video_capture::VideoInput>; 4],
    /// Per-slot mirror of `video_frame_seq`.
    pub(crate) deck_video_frame_seq: [u64; 4],
    /// Per-slot decode texture. Unlike `video_texture` (`Option`, created lazily
    /// on the first decoded frame, recreated on a resolution change), this is
    /// allocated once at bootstrap and never recreated: `opendrop_io::
    /// video_capture` pins every source to `CAPTURE_W`x`CAPTURE_H`, so there is
    /// no "first frame"/resize case to handle. Filled in place by
    /// `tick_deck_video`.
    pub(crate) deck_video_texture: [glow::NativeTexture; 4],
    /// `egui::TextureId` for each deck's video-decode texture, registered once
    /// at bootstrap alongside `deck_tex_ids`. The deck card shows this
    /// instead of `deck_tex_ids[i]` while that slot is in video mode
    /// (`show.deck_video[i].enabled`).
    pub(crate) deck_video_tex_ids: [egui::TextureId; 4],
    /// The clip library, scanned once at bootstrap and re-scanned only on
    /// the panel's Rescan button; a directory listing per frame would be
    /// pointless I/O, same reasoning as `input_devices`' bootstrap-only
    /// enumeration.
    pub(crate) video_clips: Vec<crate::video_clips::VideoClip>,
    /// Cameras enumerated once at bootstrap (`video_capture::list_cameras`,
    /// a pure sysfs scan on Linux, empty elsewhere), for the Video panel's
    /// device dropdown.
    pub(crate) video_cameras: Vec<opendrop_io::video_capture::CameraDevice>,
    /// The camera device the panel's dropdown/text field currently holds:
    /// the *candidate*, not the running one (that's
    /// `Show::video.live_device`), same "panel's own field" convention as
    /// `obs_host`/`osc_port`.
    pub(crate) video_camera_device: String,
    /// Video panel errors raised synchronously on this thread (a failed
    /// import or delete), which therefore can't live in
    /// `VideoCaptureSnapshot::last_error`, same split as
    /// `cloud_presets_secret_error`.
    pub(crate) video_local_error: Option<String>,
    /// Which `VideoState` the Video panel currently shows/edits: the
    /// global layer, or one of the 4 deck slots.
    pub(crate) video_panel_target: ui::video::VideoPanelTarget,
    /// Handle to the dedicated CloudPresets thread, `latest()`
    /// gives the current entries/busy/last-error snapshot, `control_tx`
    /// sends List/Upload/Rename/Delete/Download.
    pub(crate) cloud_presets: opendrop_io::cloud_presets::CloudPresetsHandle,
    /// The CloudPresets panel's own base-URL field, read by every action at
    /// click time, not part of `CloudPresetsSnapshot`, same reasoning as
    /// `osc_port`/`obs_host`. Empty means the feature is disabled (Override
    /// 4); mirrored to/from `UiConfig::cloud_presets_api_url: Option<
    /// String>` at bootstrap/exit (empty String <-> `None`), same pattern
    /// `selected_output_monitor` uses for its own already-`Option<String>`
    /// `UiConfig` field, just with an extra empty-string fold since this
    /// one is edited via a plain `TextEdit`.
    pub(crate) cloud_presets_api_url: String,
    /// "Link device" token-paste field, same masked/write-through/clear-
    /// after-blur convention as `kick_bearer_token_input` etc., see
    /// `ui::streaming`'s module doc comment.
    pub(crate) cloud_presets_token_input: String,
    /// Local (non-network) panel errors: keyring save/link failures for
    /// the token fields above (same convention as
    /// `streaming_secret_save_error`), plus a picked upload file that
    /// failed to read from disk. Network/API errors surface separately,
    /// through `CloudPresetsSnapshot::last_error`.
    pub(crate) cloud_presets_secret_error: Option<String>,
    /// Id + edit buffer of the cloud preset currently being renamed
    /// inline, if any; see `ui::ctx::SourcesCtx`'s field doc comment.
    pub(crate) cloud_presets_rename: Option<(String, String)>,
    /// Handle to the dedicated CloudVideo thread, `latest()` gives the
    /// current entries/listing-error/per-slug download snapshot,
    /// `control_tx` sends List/Download. Fetch-only: no token, no keyring,
    /// unlike `cloud_presets`.
    pub(crate) cloud_video: opendrop_io::cloud_video::CloudVideoHandle,
    /// The CloudVideo panel's own CDN-URL field, same empty-disables-the-
    /// panel/mirrored-to-`UiConfig` convention as `cloud_presets_api_url`.
    pub(crate) cloud_video_api_url: String,
    pub(crate) gl: Arc<glow::Context>,
    pub(crate) egui_glow: egui_glow::EguiGlow,
    pub(crate) refresh_interval: Duration,
    /// Frame-rate cap picked in the Quality panel; `None` follows the monitor.
    pub(crate) fps_override: Option<u32>,
    /// When the session file was last written; drives the periodic save.
    pub(crate) last_session_save: Instant,
    /// UI zoom factor applied to the control window's egui context.
    pub(crate) ui_scale: f32,
    pub(crate) next_frame_at: Instant,
    /// Reference instant captured once at bootstrap. Every `now_ms` fed to
    /// `core`'s beat-sync engine (`BeatDetector::process_sample`,
    /// `Show::tap_tempo`, `Show::check_volume_peak_triggers`) is derived from
    /// `t0.elapsed()`, never from `next_frame_at`: the pacing clock
    /// and the beat-sync timestamp are deliberately different clocks.
    pub(crate) t0: Instant,
    /// Handle to the dedicated audio capture thread, `latest()` gives the
    /// latest PCM chunk + energy, read once per tick and shared by every
    /// deck due that tick.
    pub(crate) audio: opendrop_audio::AudioHandle,
    /// Labels of every available input device, enumerated once at bootstrap
    /// via `opendrop_audio::list_input_devices()` and cached here for the
    /// Audio panel's dropdown. The device list doesn't change
    /// mid-session, so this is never re-scanned
    /// per frame.
    pub(crate) input_devices: Vec<String>,
    /// Name of the input device currently selected in the Audio panel's
    /// dropdown, if the user has picked one explicitly via `AudioHandle::
    /// set_device`. `None` means still on whichever device `spawn_capture`
    /// chose by default (`opendrop_audio::device::select_input_device`).
    pub(crate) selected_input_device: Option<String>,
    /// Input gain in dB, applied to the capture thread by the Audio panel.
    pub(crate) audio_gain_db: f32,
    pub(crate) audio_agc: bool,
    /// Beat detector sensitivity, 0 to 1; pushed to `Show` every frame.
    pub(crate) beat_sensitivity: f64,
    /// RMS of the current tick's PCM snapshot (`opendrop_audio::analysis::
    /// vu_level`), computed once per tick by the beat-sync engine wiring and
    /// stored here so the Audio panel's VU meter can read it without
    /// a second `vu_level` pass over the same PCM.
    pub(crate) last_vu_level: f64,
    /// Per-deck throttle for culled (invisible) decks; see IDLE_DECK_INTERVAL.
    pub(crate) deck_next_render_at: [Instant; deck::DECK_COUNT],
    /// Power mode applied to invisible decks; see `InvisibleMode`. Written
    /// by the Quality panel, read each tick by the per-deck render loop.
    pub(crate) invisible_mode: InvisibleMode,
    /// Mesh-size change requested from the Quality panel's per-deck preset
    /// buttons. Drained (and applied via `Deck::set_mesh_size`) at the point
    /// in the per-deck loop where that deck's context is already current;
    /// never call `set_mesh_size` outside a current context.
    pub(crate) pending_mesh_size: [Option<(usize, usize)>; deck::DECK_COUNT],
    pub(crate) show: Show,
    pub(crate) registry: CommandRegistry,
    pub(crate) keymap: HashMap<Key, CommandId>,
    /// Command currently in keyboard learn mode, if any: set when the
    /// Keymap panel's Learn button is clicked, cleared by `window_event`'s
    /// `WindowEvent::KeyboardInput` handler the moment it intercepts the
    /// next accepted key press to commit the binding. Simpler than `midi_
    /// learning`'s `Option<(CommandId, Option<MidiTriggerKey>)>`: keyboard
    /// events are already synchronous on this thread (no separate IO thread
    /// to diff against per-frame), so the commit happens inline in that
    /// same handler rather than being detected later in `about_to_wait`.
    /// No snapshot of the pre-existing binding needs to ride along.
    pub(crate) keymap_learning: Option<CommandId>,
    pub(crate) blit_control_timer: PassTimer,
    pub(crate) blit_output_timer: PassTimer,
    /// Wall-clock instant the output window's surface was last swapped, or
    /// `None` before the first tick. Doubles as `compute_tick_dt`'s
    /// `last_tick_at`, read near the top of the same gated tick that
    /// overwrites it near the bottom; the two ends of one
    /// tick are close enough together that this is a faithful "previous
    /// tick's real elapsed-time reference", without a second field.
    pub(crate) last_output_swap_at: Option<Instant>,
    /// Wall-clock swap-to-swap frame time in ms, mirrored from the
    /// `wall_ms` local computed right after each render (see that call
    /// site, just below `last_output_swap_at`'s own write), one frame
    /// stale by construction, same as `last_output_swap_at` itself.
    /// `None` before the first tick. Feeds the status bar's fps/frame-ms
    /// readout.
    pub(crate) last_wall_ms: Option<f64>,
    pub(crate) perf_tick: u64,
    /// Sender handed to `preflight::spawn_preflight`, cloned once per
    /// `request_preset_load` call so each validation request gets its own
    /// handle back to `preflight_rx`.
    pub(crate) preflight_tx: mpsc::Sender<(usize, String, preflight::PreflightVerdict)>,
    pub(crate) preflight_rx: mpsc::Receiver<(usize, String, preflight::PreflightVerdict)>,
    /// Name→path lookup for the full preset catalog (`show.preset_catalog`
    /// carries the name+category metadata; resolving a selected name back
    /// to a file for loading is app-side, since it's filesystem-backed).
    pub(crate) path_by_name: HashMap<String, PathBuf>,
    /// Slots currently awaiting a preflight verdict: the UI shows
    /// "validating…" on a deck-card while its slot is in here.
    pub(crate) pending_validations: HashSet<usize>,
    /// Most recent load failure per slot (preflight rejection, or a GL/load
    /// error on an otherwise-passed preset), cleared on that slot's next
    /// successful load.
    pub(crate) preset_errors: HashMap<usize, String>,
    /// Name of the preset currently loaded on each slot, for display on the
    /// deck-card.
    pub(crate) deck_preset_names: [String; 4],
    /// Soft-cut transition duration applied to every load routed through
    /// `request_preset_load`, one global setting, not per-slot.
    pub(crate) transition_seconds: f64,
    /// Live text in the Share panel's name field, transient scratch state,
    /// deliberately not part of `UiConfig`
    /// (a share link's name is a one-off label, not persistent UI state).
    pub(crate) share_set_name: String,
    /// `egui::TextureId` for each deck's live GPU texture, registered once
    /// at bootstrap via `painter.register_native_texture`; never
    /// re-registered per frame, which would leak a texture handle in
    /// egui_glow's painter every tick.
    pub(crate) deck_tex_ids: [egui::TextureId; 4],
    /// What each deck's running preset already holds for each of the
    /// [`CHANNEL_PARAM_COUNT`] side-channel parameters, the baseline
    /// `next_param_to_push` diffs against. Written at preset-load time (the
    /// values `patch_preset` baked in) and on every push, so the side channel
    /// only ever carries real changes.
    pub(crate) param_last_sent: [[f64; CHANNEL_PARAM_COUNT]; deck::DECK_COUNT],
    /// Per-deck round-robin position for that scan, so several
    /// simultaneously-moving parameters share the one-word-per-frame channel
    /// instead of the lowest-numbered one monopolising it. One cursor for
    /// Time *and* Qvar together: they compete for the same word, so two
    /// cursors would just be two families taking turns starving each other.
    pub(crate) param_cursor: [usize; deck::DECK_COUNT],
    /// Which q-var watches are baked into each deck's *currently loaded*
    /// preset. Unlike a watched value, the set of watches is compiled into
    /// the preset text at load time, so this is what `show.q_var_params` is
    /// compared against each frame to decide whether that deck needs its
    /// preset re-patched and reloaded (`resync_deck_q_var_watches`).
    pub(crate) deck_q_var_watches: [[bool; opendrop_core::q_vars::Q_VAR_COUNT]; deck::DECK_COUNT],
    /// Which panel the control window currently shows; see `Panel`.
    pub(crate) active_panel: Panel,
    /// Header's Stage toggle, wired to the Normal/Stage shell switch.
    pub(crate) stage_mode: bool,
    /// Whether the Stage bottom bar's collapsible preset drawer is open
    /// independent of `stage_mode` itself, so leaving Stage
    /// mode doesn't need to also reset it.
    pub(crate) presets_drawer_open: bool,
    /// Live text in the preset-browser search box.
    pub(crate) preset_search_query: String,
    /// Memoized `search` results for `preset_search_query`; see
    /// `ui::preset_browser::SearchCache`.
    pub(crate) preset_search_cache: ui::preset_browser::SearchCache,
    /// Preset names the user has starred, keyed by name (same stable
    /// identity `tile()` already uses for its thumbnail cache/id). Restored
    /// from `ui_config.favorite_presets` at bootstrap; persisted immediately
    /// on each toggle (`ui::preset_browser`'s star-click handler calls
    /// `config::save_config` directly), never deferred to `App::exiting`.
    pub(crate) favorite_presets: HashSet<String>,
    /// Preset Browser's "favorites only" filter toggle. Deliberately NOT
    /// persisted; matches `preset_search_query`'s own bootstrap-only reset
    /// (`:2265`, always `String::new()`): always starts `false` on launch.
    pub(crate) favorites_only: bool,
    /// Personal tags and notes, searched by the preset browser.
    pub(crate) preset_annotations: opendrop_core::preset_annotations::PresetAnnotations,
    /// Library scan results and the background scan that fills them.
    pub(crate) scan: crate::preset_scan::ScanHandle,
    pub(crate) hide_flagged: bool,
    /// Picks presets by energy when switched on per bus, and the follower of
    /// the music's energy it steers by.
    pub(crate) autopilot: opendrop_core::autopilot::Autopilot,
    pub(crate) energy_tracker: opendrop_core::autopilot::EnergyTracker,
    /// Presets played recently, newest first.
    pub(crate) preset_history: opendrop_core::preset_annotations::PresetHistory,
    /// Text being typed in the Preset notes panel.
    pub(crate) annotation_edit: crate::ui::preset_notes::AnnotationEdit,
    /// Presets whose thumbnail render failed once already. Both
    /// `thumbnails::pump_thumbnail_queue` (writer) and the preset-browser
    /// panel (reader) consult it, so a failure can't turn into a per-tick
    /// retry loop for as long as its tile is on screen.
    pub(crate) failed_thumbnails: HashSet<String>,
    /// Job queue feeding `thumbnails::pump_thumbnail_queue`,
    /// `enqueue_front`-ed by the preset-browser panel for each visible tile
    /// still missing a texture.
    pub(crate) thumb_queue: Vec<ThumbJob>,
    /// Cached preset thumbnails, keyed by preset name, populated by
    /// `pump_thumbnail_queue`, read by the preset-browser panel. Bounded to
    /// `thumbnails::MAX_RESIDENT_THUMBNAILS` entries. `thumbnail_order` is
    /// its insertion-order companion, see `thumbnails::insert_bounded`.
    pub(crate) thumbnail_textures: HashMap<String, egui::TextureHandle>,
    pub(crate) thumbnail_order: VecDeque<String>,
    /// The one outstanding `--render-thumbnail` child process, if any.
    /// Thumbnails are rendered out of process (see `thumbnails`' module doc:
    /// a preset that crashes projectM must not take the app down just
    /// because its tile scrolled into view), one at a time, polled by
    /// `pump_thumbnail_queue`.
    pub(crate) thumbnail_in_flight: Option<thumbnails::InFlightThumb>,
    /// Render children SIGKILLed after overrunning their timeout, held only
    /// until `thumbnails::reap_killed` confirms they are gone. They are
    /// never `wait()`ed on: that blocks, and this is the event-loop thread.
    pub(crate) thumbnail_killed: Vec<std::process::Child>,
    /// Disk cache dir for rendered thumbnails (see `thumbnails::cache_path`).
    pub(crate) thumbnail_cache_dir: PathBuf,
    /// Name of the monitor currently selected in the Output panel's dropdown
    /// if the user has picked one explicitly. `None` means
    /// "current monitor": passed straight through as `Fullscreen::
    /// Borderless(None)`. Monitors themselves are never cached on `AppState`:
    /// the panel re-queries `event_loop.available_monitors()` fresh every
    /// frame it's visible (see `ui::output`'s doc comment for why this is
    /// deliberately unlike the `input_devices` cache).
    pub(crate) selected_output_monitor: Option<String>,
    /// Handle to the dedicated MIDI I/O thread, `latest()` gives
    /// the current connection/mapping/clock snapshot, `events` carries raw,
    /// unfiltered `(CommandId, value01)` dispatches drained in
    /// `about_to_wait`, `control_tx` sends port/learn/LED control messages.
    pub(crate) midi: opendrop_io::midi::MidiHandle,
    /// Last-pushed LED on/off state per command, so a hotplug reconnect
    /// (detected via `hotplug_epoch`) can replay every known LED state to
    /// the freshly reopened output port instead of leaving it dark.
    pub(crate) midi_led_state: HashMap<CommandId, bool>,
    /// Command currently in MIDI learn mode, if any, paired with whatever
    /// that command's mapping entry was at the moment Learn was clicked.
    /// Set when the panel's Learn button is clicked, cleared once the
    /// snapshot's mapping entry for that command no longer equals the
    /// paired value (see `midi_learn_completed`). Comparing against
    /// "changed" rather than merely "exists" matters because
    /// `MidiControl::StartLearn` does not clear the pre-existing mapping
    /// entry, so re-learning an already-mapped command would otherwise
    /// read as instantly complete. Drives the panel's "waiting..." button
    /// state; nothing in `MidiSnapshot` signals learn-mode directly.
    pub(crate) midi_learning: Option<(CommandId, Option<MidiTriggerKey>)>,
    /// Whether the crossfader's soft-takeover gate has already let a MIDI
    /// value through this session; until then, incoming crossfader values
    /// are compared against `show.get_crossfader()` and only dispatched
    /// once they land within 0.08 of the live value (this
    /// comparison lives in `app`, not the `io` MIDI thread).
    pub(crate) midi_crossfader_taken_over: bool,
    /// Last-seen `MidiSnapshot::hotplug_epoch`, diffed each frame to detect
    /// an output port (re)connecting since the last check.
    pub(crate) midi_last_hotplug_epoch: u64,
    /// Last-seen `MidiSnapshot::clock_beat_count`, diffed each frame to
    /// detect how many MIDI clock beats fired since the last check.
    pub(crate) midi_last_beat_count: u64,
    /// Pending LED-off deadlines for the 120ms confirmation flash: a
    /// command's LED is pushed on immediately on dispatch and its deadline
    /// recorded here; `about_to_wait` checks this every call and pushes it
    /// back off once the deadline passes (no `std::thread::sleep`, no
    /// async timer).
    pub(crate) midi_led_flash_off_at: HashMap<CommandId, Instant>,
    /// Overlay id → the sprite file it was created from. The web kept the
    /// bytes themselves in IndexedDB under the same key; here the file
    /// stays where the user picked it and is read on demand by
    /// `sync_overlay_textures`. Written
    /// by the Overlays panel's `+ Sprite` button, never by `core`
    /// (`Overlay` is a pure value type with no path field, by design).
    pub(crate) overlay_assets: HashMap<String, PathBuf>,
    /// Monotonic source of overlay ids; see `ui::overlays::mint_id`.
    pub(crate) next_overlay_id: u64,
    /// Overlay id → its GL texture, rebuilt only when the overlay's
    /// content changes (`OverlayTextureEntry::key`). Purely `main.rs`'s:
    /// no panel reads it, and it must be evicted through
    /// `gl.delete_texture` while the main context is current, which is
    /// only true inside the render tick.
    pub(crate) overlay_textures: HashMap<String, OverlayTextureEntry>,
    /// When the last beat fired, driving the beat-reactive overlay pulse
    /// (`beatSyncState.beat`, true for 80 ms after each beat in the TS
    /// source; see `BEAT_PULSE_DURATION`). `None` until the first beat.
    pub(crate) last_beat_at: Option<Instant>,
    /// Handle to the dedicated OSC UDP server thread, `latest()`
    /// gives the current listening/port snapshot, `events` carries
    /// `(CommandId, value01)` dispatches drained in `about_to_wait` (no
    /// soft-takeover, unlike MIDI's crossfader: OSC
    /// has none in the existing app), `control_tx` sends Start/Stop.
    pub(crate) osc: opendrop_io::osc::OscHandle,
    /// The OSC panel's own port field, read by `Start` at click time, not
    /// `OscSnapshot::port`, which only reflects the port actually bound
    /// once listening (see `ui::osc`'s doc comment). Defaults to 7000,
    /// matching the web app's `electron-features-store.svelte.ts` default.
    pub(crate) osc_port: u16,
    /// OSC output: the live sender once started, and the destination typed in
    /// the OSC panel.
    pub(crate) osc_out: Option<opendrop_io::osc_out::OscOut>,
    pub(crate) osc_out_host: String,
    pub(crate) osc_out_port: u16,
    pub(crate) osc_out_error: Option<String>,
    /// Handle to the dedicated rkbx_link OSC UDP listener thread for
    /// synchronized music-video playback, independent of `osc` above:
    /// `latest()` gives the current listening/port/per-DJ-deck-time
    /// snapshot, `track_events` carries `RkbxTrackChanged` drained in
    /// `about_to_wait`, `control_tx` sends Start/Stop.
    pub(crate) rkbx_link: opendrop_io::rkbx_link::RkbxLinkHandle,
    /// The Rekordbox Link panel's own port field, same reasoning as
    /// `osc_port`.
    pub(crate) rkbx_link_port: u16,
    /// Set when the DJ-deck-mapping panel's `Show::set_rkbx_deck_mapping`
    /// call is refused (two DJ decks claiming the same visual slot);
    /// cleared on the next successful mapping change. Same "panel-local,
    /// synchronous error" convention as `video_local_error`.
    pub(crate) rkbx_mapping_error: Option<String>,
    /// Per-visual-slot sync bookkeeping: `Some` while that
    /// slot's currently-assigned clip was loaded by a track-change match and
    /// is still being drift-corrected against its DJ deck's reported time;
    /// `None` for an unmapped/un-synced/manually-reassigned slot. See
    /// `RkbxSyncState`'s own doc comment.
    pub(crate) rkbx_sync: [Option<RkbxSyncState>; 4],
    /// Handle to the dedicated remote-WS thread, first async
    /// integration in this codebase (its own tokio runtime lives entirely
    /// inside that thread, see `opendrop_io::remote_ws`'s module doc
    /// comment; no tokio type reaches `AppState` itself, only the same
    /// kind of `std::sync::mpsc` handle `osc`/`midi` expose). `events`
    /// carries `(CommandId, value01)` dispatches drained in
    /// `about_to_wait`, same no-soft-takeover contract as OSC.
    pub(crate) remote_ws: opendrop_io::remote_ws::RemoteWsHandle,
    /// Handle to the dedicated OBS WebSocket thread, app->OBS
    /// direction only, no `events`/`about_to_wait` drain (see
    /// `opendrop_io::obs`'s module doc comment): `latest()` gives the
    /// current connected/scenes snapshot, `control_tx` sends Connect/
    /// Disconnect/SetScene.
    pub(crate) obs: opendrop_io::obs::ObsHandle,
    /// The Streaming panel's own OBS host/port fields, read by `Connect` at
    /// click time, not part of `ObsSnapshot`, same reasoning as `osc_port`
    /// (see `ui::osc`'s doc comment). Defaults match the web app's
    /// `obs-link-store.svelte.ts` (`localhost`/`4455`).
    pub(crate) obs_host: String,
    pub(crate) obs_port: u16,
    /// Handle to the dedicated Twitch IRC thread, `latest()`
    /// gives the current connected snapshot, `control_tx` sends Connect/
    /// Disconnect. Chat messages are forwarded to `chat_events` (below),
    /// not read through this handle.
    pub(crate) twitch: opendrop_io::twitch::TwitchHandle,
    /// The Streaming panel's own Twitch channel field, read by `Connect` at
    /// click time, same reasoning as `obs_host`/`obs_port`.
    pub(crate) twitch_channel: String,
    /// Draft text for the Twitch OAuth-token secret field, never holds a
    /// value after a successful save (see `ui::streaming`'s doc comment).
    pub(crate) twitch_oauth_token_input: String,
    /// Handle to the dedicated Kick thread, same shape as
    /// `twitch` above.
    pub(crate) kick: opendrop_io::kick::KickHandle,
    /// The Streaming panel's own Kick channel field.
    pub(crate) kick_channel: String,
    /// Draft text for the 3 Kick credential secret fields, same "cleared
    /// after save" contract as `twitch_oauth_token_input`.
    pub(crate) kick_bearer_token_input: String,
    pub(crate) kick_xsrf_token_input: String,
    pub(crate) kick_cookies_input: String,
    /// Handle to the dedicated Ableton Link thread, `latest()`
    /// gives the current enabled/tempo/beat/phase/peers snapshot,
    /// `control_tx` sends Start/Stop/SetTempo. Present only when the
    /// `link` feature is enabled (OFF by default, GPL licensing
    /// boundary, see `opendrop_io::link`'s module doc comment and the
    /// root README).
    #[cfg(feature = "link")]
    pub(crate) link: opendrop_io::link::LinkHandle,
    /// The Link panel's own tempo field, read by `SetTempo` at click
    /// time, not `LinkSnapshot::tempo`, same reasoning as `osc_port`
    /// (see `ui::osc`'s doc comment).
    #[cfg(feature = "link")]
    pub(crate) link_tempo_input: f64,
    /// Receiving end of the shared chat-message channel both `twitch` and
    /// `kick` feed (mirrors `broadcastChatMessage`, `main.cjs:425-429`;
    /// see `opendrop_io::chat`'s module doc comment). Drained every
    /// `about_to_wait` call into `chat_log` below: an undrained unbounded
    /// channel with a live producer is exactly the leak hazard
    /// `io::ndi::out.rs`'s `drain_slot` doc comment warns against.
    pub(crate) chat_events: std::sync::mpsc::Receiver<opendrop_io::chat::ChatMessage>,
    /// Bounded chat history (most recent `CHAT_LOG_CAP` messages, oldest
    /// dropped first; see `push_chat_message`) rendered at the bottom of
    /// the Streaming panel, so real Twitch/Kick chat activity is actually
    /// observable in the running app.
    pub(crate) chat_log: VecDeque<opendrop_io::chat::ChatMessage>,
    /// Panel-local error from the Streaming panel's save-on-blur secret
    /// fields (Twitch OAuth token, Kick bearer/xsrf/cookies), set
    /// synchronously on the UI thread by `ui::streaming::save_secret_field`
    /// when `secrets::set_secret` fails, so it can't be an `ArcSwap`
    /// snapshot field like `ObsSnapshot`/`TwitchSnapshot`/`KickSnapshot`'s
    /// `last_error`. Cleared on the next successful save.
    pub(crate) streaming_secret_save_error: Option<String>,
}

#[derive(Default)]
pub(crate) struct App {
    pub(crate) state: Option<AppState>,
}
