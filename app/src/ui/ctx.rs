//! `ui_root`'s 51 (53 with
//! `--features link`) positional parameters, grouped into 7 disjoint `&mut`
//! context structs. Pure re-packaging: every field below is exactly the
//! binding `main.rs`'s `let AppState { ... } = state;` destructure already
//! produces, only regrouped; no new state, no behavior/visual change.
//!
//! Each struct mirrors one region of the control window's panel set, so a
//! panel's `show` call only needs to borrow the structs it actually reads:
//! `ShellCtx` (nav chrome), `PerformCtx` (decks/playlists/browser's shared
//! `Show`), `LibraryCtx` (preset browser), `SourcesCtx` (external control
//! surfaces: audio input, MIDI, OSC, remote WS, V4L2, the NDI-in selection,
//! the keyboard remap table),
//! `OutputCtx` (NDI-out, the Output panel's monitor picker, the Quality
//! panel), `StreamCtx` (OBS/Twitch/Kick), `ControlCtx` (Ableton Link, `#[cfg(
//! feature = "link")]`-gated per field, never on the struct itself; see
//! that struct's own doc comment).
//!
//! Two borrow-check points worth calling out explicitly (both already
//! resolved by construction, not something a caller needs to work around):
//! - `show` (the `Show` business object) lives in `PerformCtx` only, never
//!   duplicated into another struct. `ui::preset_browser::show`, the one
//!   panel that needs both a `Show` borrow and the browser's own local
//!   state, takes `(&mut PerformCtx, &mut LibraryCtx)` as two distinct
//!   `&mut` parameters rather than one combined borrow, and reborrows
//!   `perform.show` to `&Show` only for the instruction that calls
//!   `SearchCache::resolve` (see that call site's own comment).
//! - `ndi_in_selected_source` lives in `SourcesCtx` while the NDI-out
//!   handle/toggles live in `OutputCtx`, required both to avoid a double
//!   borrow of one NDI-shaped bundle and because `Panel::NdiIn`/`NdiOut`
//!   (this same step) will eventually want the in/out panels reading from
//!   different structs.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use opendrop_core::commands::{CommandId, CommandRegistry};
use opendrop_core::show::Show;
use opendrop_core::thumb_queue::ThumbJob;
use opendrop_engine::deck;
use opendrop_io::midi::MidiTriggerKey;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::Key;
use winit::window::Window;

use crate::ui::preset_browser::SearchCache;
use crate::{InvisibleMode, Panel};

/// Nav chrome: which panel is active, the Stage toggle, and the status
/// bar's frame-timing readout (Step 10). The mini-transport (crossfader,
/// BPM, tap) stays out
/// of this struct on purpose: it lives on `Show` (`PerformCtx::show`, the
/// single owner of that business object; see this module's doc comment),
/// so `ui::shell::header` takes `(&mut ShellCtx, &mut PerformCtx)` as two
/// distinct parameters instead, same two-struct idiom as
/// `ui::preset_browser::show`. `theme_request` is deliberately NOT a field
/// of this struct: it's `ui_root`'s own 8th, standalone parameter (same
/// idiom as `LibraryCtx::load_request`), so it stays out-of-band the same
/// way across every struct instead of only for whichever one happens to
/// hold the nav row.
pub(crate) struct ShellCtx<'a> {
    pub(crate) active_panel: &'a mut Panel,
    /// Header's Stage toggle (`⛶`, `ghost_button`) and the `F11` keyboard
    /// toggle in `main.rs`'s `window_event`, drives `ui_root`'s choice
    /// between the Normal and Stage variants of the header/nav/status
    /// bar.
    pub(crate) stage_mode: &'a mut bool,
    /// Status bar's fps/frame-ms readout: wall-clock swap-to-swap time
    /// from `main.rs`'s `about_to_wait`, one frame stale by construction
    /// (mirrors `AppState::last_output_swap_at`'s own staleness note).
    /// `None` before the first tick. Copied by value out of `AppState`
    /// before the destructure that produces the other 7 structs' fields
    /// (same convention as `SourcesCtx::last_vu_level`), since nothing
    /// needs to mutate it from inside `ui_root`.
    pub(crate) last_wall_ms: Option<f64>,
    /// Whether the Stage bottom bar's preset drawer (`Panel::bottom("od_
    /// presets_drawer").show_collapsible`) is open, Stage-mode-only UI
    /// state (Step 11), toggled by a `ghost_button` in `status_bar_stage`,
    /// not by `stage_mode` itself.
    pub(crate) presets_drawer_open: &'a mut bool,
}

/// Live performance state: the `Show` business object plus per-deck UI
/// state, shared by the Decks, Playlists, and Preset Browser panels (the
/// browser also needs `LibraryCtx`; see this module's doc comment on the
/// two-struct split).
pub(crate) struct PerformCtx<'a> {
    pub(crate) show: &'a mut Show,
    pub(crate) deck_tex_ids: &'a [egui::TextureId; 4],
    pub(crate) deck_preset_names: &'a [String; 4],
    pub(crate) deck_video_tex_ids: &'a [egui::TextureId; 4],
    pub(crate) deck_video_errors: &'a [Option<String>; 4],
    pub(crate) pending_validations: &'a HashSet<usize>,
    pub(crate) preset_errors: &'a HashMap<usize, String>,
    pub(crate) transition_seconds: &'a mut f64,
    /// Share panel's name field, lives here
    /// rather than a new struct because the Share panel needs the same
    /// `show`/`deck_preset_names`/`transition_seconds` this struct already
    /// carries (crossfade duration doubles as `SharedSet::transition_time`,
    /// no native equivalent existed before this field).
    pub(crate) share_set_name: &'a mut String,
    pub(crate) t0: Instant,
}

/// Preset Browser panel state: search box, cached results, thumbnail
/// pipeline. `search_cache` is named to match `SearchCache::resolve`'s call
/// site exactly (`library.search_cache.resolve(&*perform.show, query)`,
/// inside `ui::preset_browser::show`, a shared reborrow of a `PerformCtx`
/// field, scoped to that one call, never stored). `load_request` is the
/// browser's click-to-load out-param, same idiom as the pre-existing
/// `preset_load_request` local in `main.rs`'s `run()` closure.
pub(crate) struct LibraryCtx<'a> {
    pub(crate) preset_search_query: &'a mut String,
    pub(crate) search_cache: &'a mut SearchCache,
    pub(crate) thumb_queue: &'a mut Vec<ThumbJob>,
    pub(crate) thumbnail_textures: &'a HashMap<String, egui::TextureHandle>,
    pub(crate) failed_thumbnails: &'a HashSet<String>,
    pub(crate) load_request: &'a mut Option<String>,
    pub(crate) favorite_presets: &'a mut HashSet<String>,
    pub(crate) favorites_only: &'a mut bool,
}

/// External control/device surfaces: Audio input, MIDI, OSC, remote WS,
/// V4L2loopback, CloudPresets, and the NDI-in panel's own selected-source
/// state (the NDI handle and its composite/deck toggles are output-side;
/// see `OutputCtx`).
pub(crate) struct SourcesCtx<'a> {
    pub(crate) audio: &'a opendrop_audio::AudioHandle,
    pub(crate) input_devices: &'a Vec<String>,
    pub(crate) selected_input_device: &'a mut Option<String>,
    pub(crate) last_vu_level: f64,
    pub(crate) registry: &'a CommandRegistry,
    pub(crate) keymap: &'a mut HashMap<Key, CommandId>,
    pub(crate) keymap_learning: &'a mut Option<CommandId>,
    pub(crate) midi: &'a opendrop_io::midi::MidiHandle,
    pub(crate) midi_learning: &'a mut Option<(CommandId, Option<MidiTriggerKey>)>,
    pub(crate) ndi_in_selected_source: &'a mut Option<opendrop_io::ndi::NdiSource>,
    pub(crate) osc: &'a opendrop_io::osc::OscHandle,
    pub(crate) osc_port: &'a mut u16,
    pub(crate) rkbx_link: &'a opendrop_io::rkbx_link::RkbxLinkHandle,
    pub(crate) rkbx_link_port: &'a mut u16,
    pub(crate) rkbx_mapping_error: &'a mut Option<String>,
    /// Overlays panel: overlay id
    /// → the sprite file it was created from, and the id counter. The
    /// overlay list itself lives on `Show` (`PerformCtx::show`) like every
    /// other business object: these two are the I/O-side halves
    /// `core::overlay` deliberately does not own (see `ui::overlays`'s
    /// module doc comment). The GL texture cache keyed by the same ids
    /// stays out of every context struct: `main.rs`'s render loop is its
    /// only reader, no panel touches it.
    pub(crate) overlay_assets: &'a mut HashMap<String, PathBuf>,
    pub(crate) next_overlay_id: &'a mut u64,
    pub(crate) remote_ws: &'a opendrop_io::remote_ws::RemoteWsHandle,
    pub(crate) v4l2: &'a opendrop_io::v4l2loopback::V4l2Handle,
    pub(crate) v4l2_active: &'a mut bool,
    pub(crate) v4l2_device: &'a mut Option<Option<PathBuf>>,
    /// Video panel: the clip
    /// library, the camera picker's cached device list and its current
    /// selection, and the panel's own synchronous import/delete errors.
    /// Same split as the Overlays fields above: the layer's *state* lives
    /// on `Show` (`PerformCtx::show`), these are the I/O-side halves
    /// `core::video` deliberately does not own. The capture handle itself
    /// stays out of every context struct, like the GL texture cache: the
    /// panel takes a `VideoCaptureSnapshot`, and `main.rs` (its only other
    /// reader) owns the handle.
    pub(crate) video_clips: &'a mut Vec<crate::video_clips::VideoClip>,
    pub(crate) video_cameras: &'a Vec<opendrop_io::video_capture::CameraDevice>,
    pub(crate) video_camera_device: &'a mut String,
    pub(crate) video_local_error: &'a mut Option<String>,
    pub(crate) video_capture: &'a opendrop_io::video_capture::VideoCaptureSnapshot,
    /// The Video panel's one outbound NDI intent for this frame, applied
    /// by `main.rs` right after `ui_root` returns, same out-param idiom
    /// as `LibraryCtx::load_request`, and for the same reason: the NDI
    /// handle is borrowed by `OutputCtx` for the whole closure, so the
    /// panel records what it wants instead of sending it itself.
    pub(crate) video_ndi_request: &'a mut Option<crate::ui::video::VideoNdiRequest>,
    pub(crate) video_panel_target: &'a mut crate::ui::video::VideoPanelTarget,
    pub(crate) cloud_presets: &'a opendrop_io::cloud_presets::CloudPresetsHandle,
    pub(crate) cloud_presets_api_url: &'a mut String,
    pub(crate) cloud_presets_token_input: &'a mut String,
    /// Local (non-network) panel errors: see `AppState::
    /// cloud_presets_secret_error`'s doc comment.
    pub(crate) cloud_presets_secret_error: &'a mut Option<String>,
    /// Id + edit buffer of the entry currently being renamed inline, if
    /// any, mirrors `SidebarCloudPresets.svelte`'s `renamingId`/
    /// `renameValue` local state, promoted to `AppState` since this panel
    /// (like every other one) takes individual fields, not a place to
    /// stash cross-frame widget state of its own (see `ui::quality`'s
    /// module doc comment on that convention).
    pub(crate) cloud_presets_rename: &'a mut Option<(String, String)>,
}

/// Output-side state: the NDI-out handle and its composite/per-deck
/// toggles, the Output panel's monitor picker, and the Quality panel.
pub(crate) struct OutputCtx<'a> {
    pub(crate) refresh_interval: &'a mut Duration,
    pub(crate) invisible_mode: &'a mut InvisibleMode,
    pub(crate) pending_mesh_size: &'a mut [Option<(usize, usize)>; deck::DECK_COUNT],
    pub(crate) event_loop: &'a ActiveEventLoop,
    pub(crate) output_window: &'a Window,
    pub(crate) selected_output_monitor: &'a mut Option<String>,
    pub(crate) ndi: &'a opendrop_io::ndi::NdiHandle,
    pub(crate) ndi_composite_active: &'a mut bool,
    pub(crate) ndi_deck_active: &'a mut [bool; deck::DECK_COUNT],
}

/// Streaming panel state: OBS, Twitch, Kick, and the shared chat log.
pub(crate) struct StreamCtx<'a> {
    pub(crate) obs: &'a opendrop_io::obs::ObsHandle,
    pub(crate) obs_host: &'a mut String,
    pub(crate) obs_port: &'a mut u16,
    pub(crate) twitch: &'a opendrop_io::twitch::TwitchHandle,
    pub(crate) twitch_channel: &'a mut String,
    pub(crate) twitch_oauth_token_input: &'a mut String,
    pub(crate) kick: &'a opendrop_io::kick::KickHandle,
    pub(crate) kick_channel: &'a mut String,
    pub(crate) kick_bearer_token_input: &'a mut String,
    pub(crate) kick_xsrf_token_input: &'a mut String,
    pub(crate) kick_cookies_input: &'a mut String,
    pub(crate) chat_log: &'a VecDeque<opendrop_io::chat::ChatMessage>,
    pub(crate) streaming_secret_save_error: &'a mut Option<String>,
}

/// Ableton Link panel state. `#[cfg(feature = "link")]` is on the two
/// fields individually, never on the struct itself, so `ControlCtx` exists
/// (and `ui_root`'s arity stays 8 params) in both build configurations.
/// With the `link`
/// feature off (the default), both real fields disappear and `_marker` is
/// the struct's only field: a zero-sized `PhantomData` purely so the `'a`
/// lifetime parameter stays used (an empty struct with a declared-but-
/// unused lifetime is a hard compile error, `E0392`), not business state,
/// never read.
pub(crate) struct ControlCtx<'a> {
    #[cfg(feature = "link")]
    pub(crate) link: &'a opendrop_io::link::LinkHandle,
    #[cfg(feature = "link")]
    pub(crate) link_tempo_input: &'a mut f64,
    #[cfg(not(feature = "link"))]
    pub(crate) _marker: std::marker::PhantomData<&'a ()>,
}
