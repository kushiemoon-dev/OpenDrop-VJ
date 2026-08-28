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
use opendrop_engine::compositor::{Compositor, LayerInput};
use opendrop_engine::deck::{self, Deck};
use opendrop_engine::timing::PassTimer;
use raw_window_handle::HasWindowHandle;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::Key;
use winit::window::{Window, WindowAttributes, WindowId};

mod keymap;
mod preflight;

/// ponytail: paced off the control window's monitor only, read once at
/// bootstrap. A VJ setup can have control and output on different-refresh
/// monitors; revisit if that ever causes visible judder on the output side.
const FALLBACK_REFRESH_MILLIHERTZ: u32 = 60_000;

/// Culled (opacity ≤ 0.001) decks still render, just at this much lower
/// rate: not stopped outright: so a deck doesn't show a visible cold
/// start (projectM's per-preset warm-up/transition state going stale) the
/// moment the crossfader brings it back in.
const IDLE_DECK_INTERVAL: Duration = Duration::from_millis(100); // ~10fps floor

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
    gl: Arc<glow::Context>,
    egui_glow: egui_glow::EguiGlow,
    refresh_interval: Duration,
    next_frame_at: Instant,
    /// Handle to the dedicated audio capture thread: `latest()` gives the
    /// latest PCM chunk + energy, read once per tick and shared by every
    /// deck due that tick.
    audio: opendrop_audio::AudioHandle,
    /// Per-deck throttle for culled (invisible) decks: see IDLE_DECK_INTERVAL.
    deck_next_render_at: [Instant; deck::DECK_COUNT],
    show: Show,
    registry: CommandRegistry,
    keymap: HashMap<Key, CommandId>,
    blit_control_timer: PassTimer,
    blit_output_timer: PassTimer,
    last_output_swap_at: Option<Instant>,
    perf_tick: u64,
    /// Sender handed to `preflight::spawn_preflight`: kept for Task 14,
    /// which triggers validations from the UI; unused until then.
    #[allow(dead_code)]
    preflight_tx: mpsc::Sender<(usize, String, preflight::PreflightVerdict)>,
    preflight_rx: mpsc::Receiver<(usize, String, preflight::PreflightVerdict)>,
}

#[derive(Default)]
struct App {
    state: Option<AppState>,
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
        // never back up in the channel. Verdict-handling (load the preset
        // onto a live deck, surface it in the UI, etc.) is Task 14's job:
        // this just logs for now.
        while let Ok((slot, name, verdict)) = state.preflight_rx.try_recv() {
            match verdict {
                preflight::PreflightVerdict::Ok => println!("[app] preflight ok: slot {slot} preset {name}"),
                preflight::PreflightVerdict::Failed(reason) => {
                    println!("[app] preflight failed: slot {slot} preset {name}: {reason}")
                }
            }
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
            for i in 0..deck::DECK_COUNT {
                let visible = layer_inputs[i].opacity > 0.001;
                if !visible && now < state.deck_next_render_at[i] {
                    continue;
                }
                if let Err(e) = state.decks[i].context.make_current(&state.decks[i].surface) {
                    eprintln!("[app] deck {i} make_current failed: {e}");
                    continue;
                }
                state.decks[i].render_frame(&audio.pcm);
                if !visible {
                    state.deck_next_render_at[i] = now + IDLE_DECK_INTERVAL;
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

            // Placeholder panel only: real content lands in later steps
            // (16-21). This just proves the egui_glow pipeline is wired up.
            state.egui_glow.run(&state.control.window, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("egui OK");
                });
            });

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
/// same skip the Phase 0 spike applied when picking its one preset.
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

    let presets = pick_distinct_presets(&preset_dir(), deck::DECK_COUNT);
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
    let egui_glow = egui_glow::EguiGlow::new(event_loop, Arc::clone(&gl), None, None, true);

    // Compositor FBO/texture belong to whichever context is current at
    // creation: main_ctx is current here (on control's surface), same as
    // it will be every time the compositor's FBO is touched later.
    let compositor = Compositor::new(&gl)?;
    let blit_control_timer = PassTimer::new(&gl).map_err(|e| format!("blit_control_timer: {e}"))?;
    let blit_output_timer = PassTimer::new(&gl).map_err(|e| format!("blit_output_timer: {e}"))?;

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

    Ok(AppState {
        display,
        main_ctx,
        control,
        output,
        decks,
        compositor,
        gl,
        egui_glow,
        refresh_interval,
        next_frame_at: Instant::now(),
        audio: opendrop_audio::spawn_capture(),
        deck_next_render_at: [Instant::now(); deck::DECK_COUNT],
        show: Show::default(),
        registry: create_default_registry(),
        keymap: keymap::default_keymap(),
        blit_control_timer,
        blit_output_timer,
        last_output_swap_at: None,
        perf_tick: 0,
        preflight_tx,
        preflight_rx,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--preflight-check" {
        preflight::run_preflight_check(Path::new(&args[2]));
    }

    let event_loop = EventLoop::new().expect("failed to create winit event loop");
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("event loop exited with an error");
}
