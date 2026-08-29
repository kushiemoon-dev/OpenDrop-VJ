use glow::HasContext;
use glutin::config::{Api, Config, ConfigSurfaceTypes, ConfigTemplateBuilder};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, PossiblyCurrentContext, Version};
use glutin::display::{Display, GetGlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::DisplayBuilder;
use opendrop_core::blend::DEFAULT_COLOR_PARAMS;
use opendrop_core::commands::{create_default_registry, CommandId, CommandRegistry};
use opendrop_core::show::{DeckBus, Show};
use opendrop_core::thumb_queue::ThumbJob;
use opendrop_engine::compositor::{Compositor, LayerInput, COMP_H, COMP_W};
use opendrop_engine::deck::{self, Deck};
use opendrop_engine::readback::FrameReadback;
use opendrop_engine::timing::PassTimer;
use raw_window_handle::HasWindowHandle;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::Key;
use winit::window::{Window, WindowAttributes, WindowId};

mod egl_headless;
mod keymap;
mod preflight;
mod thumbnail_child;
mod thumbnails;
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

/// Power mode applied to invisible (opacity ≤ 0.001) decks: selected from
/// the Quality panel (Step 20). `Eco` reproduces the original always-on
/// behavior (throttled to `IDLE_DECK_INTERVAL`, so the texture stays warm
/// for a fast comeback); `Pause` skips rendering the deck entirely while
/// invisible, so its texture keeps showing whatever frame it last rendered;
/// `Off` disables the throttle, rendering invisible decks at full rate same
/// as visible ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InvisibleMode {
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

/// Which top-level panel the control window is currently showing: gates
/// per-tick work that only matters while its panel is visible (Step 17: the
/// thumbnail pump only runs while `PresetBrowser` is on screen). Deliberately
/// minimal: just enough to gate that one thing, not a general tabbed-panel
/// system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Panel {
    #[default]
    Decks,
    PresetBrowser,
    Playlists,
    Audio,
    Quality,
    Output,
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
    /// frame in `about_to_wait`, gated behind `ndi_active || v4l2_active` so
    /// idle sessions never pay the readback cost.
    compositor_readback: FrameReadback,
    deck_readback: [FrameReadback; deck::DECK_COUNT],
    /// Outlet for `compositor_readback`'s polled RGBA bytes. The `Receiver`
    /// end is held here too, for now: no consumer exists on this branch
    /// yet; the not-yet-built NDI output and v4l2loopback output threads
    /// take it over once they exist.
    compositor_frame_tx: mpsc::Sender<Vec<u8>>,
    #[allow(dead_code)] // held for the not-yet-built NDI/v4l2loopback output threads to take over; unread on this branch
    compositor_frame_rx: mpsc::Receiver<Vec<u8>>,
    /// Same as `compositor_frame_tx`/`_rx`, one channel pair per deck.
    deck_frame_tx: [mpsc::Sender<Vec<u8>>; deck::DECK_COUNT],
    #[allow(dead_code)] // held for the not-yet-built NDI/v4l2loopback output threads to take over; unread on this branch
    deck_frame_rx: [mpsc::Receiver<Vec<u8>>; deck::DECK_COUNT],
    /// Whether an NDI output consumer is active: set by the NDI panel
    /// (Step 10), not yet wired on this branch. Gates the readback loop.
    ndi_active: bool,
    /// Whether a v4l2loopback output consumer is active: set by its panel
    /// (Step 19), not yet wired on this branch. Gates the readback loop.
    v4l2_active: bool,
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
    last_output_swap_at: Option<Instant>,
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
    /// Live text in the preset-browser search box.
    preset_search_query: String,
    /// Memoized `search` results for `preset_search_query`: see
    /// `ui::preset_browser::SearchCache`.
    preset_search_cache: ui::preset_browser::SearchCache,
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
    /// `pump_thumbnail_queue`, read by the preset-browser panel.
    thumbnail_textures: HashMap<String, egui::TextureHandle>,
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
    let Some(path) = state.path_by_name.get(&name).cloned() else { return };
    state.pending_validations.insert(slot); // for the UI: "validating…" on this card
    preflight::spawn_preflight(path, slot, name, state.preflight_tx.clone());
}

/// Root of the control window's egui content for this frame. `ui` here is
/// already `&mut egui::Ui` (this vendored `egui_glow`'s `EguiGlow::run`
/// hands a `Ui`, not a `Context`: `CentralPanel::show` in this vendored
/// `egui` 0.36.1 matches, taking `ui: &mut Ui` as its first argument; see
/// Task 2's notes on this same drift). Decks (Step 16) and preset-browser
/// (Step 17) panels so far, switched via the tab row; later steps add more
/// panels here.
///
/// Takes individual `AppState` fields, not `&mut AppState`: see
/// `ui::decks::show`'s doc comment for why. `load_request` is an out-param:
/// the preset-browser panel can't call `request_preset_load` itself (that
/// needs the whole `AppState`), so a click just records the name here for
/// the caller to act on once this frame's egui pass is done.
#[allow(clippy::too_many_arguments)]
fn ui_root(
    ui: &mut egui::Ui,
    show: &mut Show,
    deck_tex_ids: &[egui::TextureId; 4],
    deck_preset_names: &[String; 4],
    pending_validations: &HashSet<usize>,
    preset_errors: &HashMap<usize, String>,
    transition_seconds: &mut f64,
    active_panel: &mut Panel,
    preset_search_query: &mut String,
    preset_search_cache: &mut ui::preset_browser::SearchCache,
    thumb_queue: &mut Vec<ThumbJob>,
    thumbnail_textures: &HashMap<String, egui::TextureHandle>,
    failed_thumbnails: &HashSet<String>,
    audio: &opendrop_audio::AudioHandle,
    input_devices: &Vec<String>,
    selected_input_device: &mut Option<String>,
    last_vu_level: f64,
    load_request: &mut Option<String>,
    t0: Instant,
    refresh_interval: &mut Duration,
    invisible_mode: &mut InvisibleMode,
    pending_mesh_size: &mut [Option<(usize, usize)>; deck::DECK_COUNT],
    event_loop: &ActiveEventLoop,
    output_window: &Window,
    selected_output_monitor: &mut Option<String>,
) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(active_panel, Panel::Decks, "Decks");
            ui.selectable_value(active_panel, Panel::PresetBrowser, "Presets");
            ui.selectable_value(active_panel, Panel::Playlists, "Playlists");
            ui.selectable_value(active_panel, Panel::Audio, "Audio");
            ui.selectable_value(active_panel, Panel::Quality, "Quality");
            ui.selectable_value(active_panel, Panel::Output, "Output");
        });
        ui.separator();
        match active_panel {
            Panel::Decks => {
                ui::decks::show(ui, show, deck_tex_ids, deck_preset_names, pending_validations, preset_errors, transition_seconds);
            }
            Panel::PresetBrowser => {
                ui::preset_browser::show(
                    ui,
                    show,
                    preset_search_query,
                    preset_search_cache,
                    thumb_queue,
                    thumbnail_textures,
                    failed_thumbnails,
                    load_request,
                );
            }
            Panel::Playlists => {
                ui::playlists::show(ui, show, t0);
            }
            Panel::Audio => {
                ui::audio::show(ui, audio, input_devices, selected_input_device, last_vu_level);
            }
            Panel::Quality => {
                ui::quality::show(ui, refresh_interval, invisible_mode, pending_mesh_size);
            }
            Panel::Output => {
                ui::output::show(ui, event_loop, output_window, selected_output_monitor);
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
            if key_event.state == ElementState::Pressed && !state.egui_glow.egui_ctx.egui_wants_keyboard_input() {
                if let Some(&cmd_id) = state.keymap.get(&key_event.logical_key) {
                    state.registry.dispatch(cmd_id, 1.0, &mut state.show);
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
            let dt = state.refresh_interval.as_secs_f64();
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

            for i in 0..deck::DECK_COUNT {
                let visible = layer_inputs[i].opacity > 0.001;
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
            let lowest_active = (0..deck::DECK_COUNT).find(|&i| layer_inputs[i].opacity > 0.001);
            state.compositor.begin_frame(&state.gl);
            for i in 0..deck::DECK_COUNT {
                let force_normal = lowest_active == Some(i);
                state.compositor.composite_layer(&state.gl, state.decks[i].texture, &layer_inputs[i], force_normal);
            }
            state.compositor.end_frame(&state.gl);

            // Step 5: GPU->CPU readback feeding the future NDI / v4l2loopback
            // output paths: gated behind whichever of those consumers is
            // actually active so an idle session never pays the ~8MB/frame
            // RGBA copy. No consumer is wired up on this branch yet: a
            // polled `Some(bytes)` is just pushed onto its channel and
            // silently dropped if nothing is receiving, same non-blocking,
            // ignore-on-fail convention as `AudioHandle::set_device`.
            if state.ndi_active || state.v4l2_active {
                state.compositor_readback.begin_read(&state.gl);
                if let Some(bytes) = state.compositor_readback.poll(&state.gl) {
                    let _ = state.compositor_frame_tx.send(bytes);
                }
                for i in 0..deck::DECK_COUNT {
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
                preset_search_query,
                preset_search_cache,
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
                ..
            } = state;
            // Out-param for the preset-browser click path: see `ui_root`'s
            // doc comment. `show` is still borrowed (via the destructure
            // above) for the duration of this closure, so the actual
            // `request_preset_load` call has to wait until after `run()`
            // returns, below.
            let mut preset_load_request: Option<String> = None;
            egui_glow.run(&control.window, |ui| {
                ui_root(
                    ui,
                    show,
                    deck_tex_ids,
                    deck_preset_names,
                    pending_validations,
                    preset_errors,
                    transition_seconds,
                    active_panel,
                    preset_search_query,
                    preset_search_cache,
                    thumb_queue,
                    thumbnail_textures,
                    failed_thumbnails,
                    audio,
                    input_devices,
                    selected_input_device,
                    last_vu_level,
                    &mut preset_load_request,
                    t0,
                    refresh_interval,
                    invisible_mode,
                    pending_mesh_size,
                    event_loop,
                    &output.window,
                    selected_output_monitor,
                );
            });
            if let Some(name) = preset_load_request {
                // Same pipeline as keyboard navigation and playlist/beat-sync
                // advances (`take_fired_presets`, above): never a direct
                // `Deck::load_preset` call, which would bypass the pre-flight
                // validation Step 14 added.
                request_preset_load(state, state.show.selected_slot, name);
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

            state.perf_tick += 1;
            if state.perf_tick % 60 == 0 {
                let active = (0..deck::DECK_COUNT).find(|&i| layer_inputs[i].opacity > 0.001).unwrap_or(0);
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

fn preset_dir() -> PathBuf {
    let raw = std::env::var("OPENDROP_PRESET_DIR").unwrap_or_else(|_| {
        panic!(
            "OPENDROP_PRESET_DIR is not set. Point it at a directory of .milk presets, e.g.:\n  \
             OPENDROP_PRESET_DIR=/srv/http/opendrop-presets cargo run -p opendrop-app"
        )
    });
    PathBuf::from(raw)
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

    let preset_root = preset_dir();
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

    // Output channels for the readback bytes above. No consumer exists on
    // this branch: the `Receiver` ends are just held on `AppState` until
    // the future NDI / v4l2loopback output threads take them over.
    let (compositor_frame_tx, compositor_frame_rx) = mpsc::channel::<Vec<u8>>();
    let (deck_frame_tx, deck_frame_rx): ([mpsc::Sender<Vec<u8>>; deck::DECK_COUNT], [mpsc::Receiver<Vec<u8>>; deck::DECK_COUNT]) = {
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

    let mut show = Show::default();
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
        compositor_frame_rx,
        deck_frame_tx,
        deck_frame_rx,
        ndi_active: false,
        v4l2_active: false,
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
        selected_input_device: None,
        last_vu_level: 0.0,
        deck_next_render_at: [Instant::now(); deck::DECK_COUNT],
        invisible_mode: InvisibleMode::Eco,
        pending_mesh_size: [None; deck::DECK_COUNT],
        show,
        registry: create_default_registry(),
        keymap: keymap::default_keymap(),
        blit_control_timer,
        blit_output_timer,
        last_output_swap_at: None,
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
        active_panel: Panel::default(),
        preset_search_query: String::new(),
        preset_search_cache: ui::preset_browser::SearchCache::default(),
        failed_thumbnails: HashSet::new(),
        thumb_queue: Vec::new(),
        thumbnail_textures: HashMap::new(),
        thumbnail_in_flight: None,
        thumbnail_killed: Vec::new(),
        thumbnail_cache_dir: thumbnail_cache_dir(),
        selected_output_monitor: None,
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

        fn os(s: &str) -> Option<OsString> {
            Some(OsString::from(s))
        }

        #[test]
        fn prefers_xdg_cache_home() {
            let dir = thumbnail_cache_dir_from(os("/xdg"), os("/home/u"));
            assert_eq!(dir, PathBuf::from("/xdg/opendrop/thumbnails"));
        }

        #[test]
        fn falls_back_to_home_dot_cache() {
            let dir = thumbnail_cache_dir_from(None, os("/home/u"));
            assert_eq!(dir, PathBuf::from("/home/u/.cache/opendrop/thumbnails"));
        }

        #[test]
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
}
