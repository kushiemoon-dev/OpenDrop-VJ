use glow::HasContext;
use glutin::config::{Api, Config, ConfigSurfaceTypes, ConfigTemplateBuilder};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, PossiblyCurrentContext, Version};
use glutin::display::{Display, GetGlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{ApiPreference, DisplayBuilder};
use opendrop_core::commands::create_default_registry;
use opendrop_core::show::Show;
use opendrop_engine::compositor::{Compositor, COMP_H, COMP_W};
use opendrop_engine::deck::{self};
use opendrop_engine::readback::FrameReadback;
use opendrop_engine::timing::PassTimer;
use raw_window_handle::HasWindowHandle;
use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};
use crate::deck_video::create_empty_video_texture;
use crate::paths::{pick_distinct_presets, preset_dir, preset_display_name, thumbnail_cache_dir, walk_milk_files};
use crate::preset_load::{channel_values, load_preset_onto_deck};
use crate::{config, keymap, theme, ui, video_clips, AppState, FALLBACK_REFRESH_MILLIHERTZ};

pub(crate) struct WindowSlot {
    pub(crate) window: Window,
    pub(crate) surface: Surface<WindowSurface>,
    pub(crate) size: (u32, u32),
    pub(crate) occluded: bool,
}

impl WindowSlot {
    /// Makes `ctx` current against this slot's surface and resets the
    /// viewport; glViewport does not re-derive from the surface on its own,
    /// so every switch between the two windows' surfaces must redo this.
    pub(crate) fn make_current_and_size_viewport(&self, ctx: &PossiblyCurrentContext, gl: &glow::Context) -> Result<(), String> {
        ctx.make_current(&self.surface).map_err(|e| format!("make_current failed: {e}"))?;
        unsafe { gl.viewport(0, 0, self.size.0 as i32, self.size.1 as i32) };
        Ok(())
    }

    pub(crate) fn render_and_swap(
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
    pub(crate) fn render_and_swap_with_egui(
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

/// Builds one window + the EGL Display/Config it negotiated, via
/// glutin-winit's DisplayBuilder (the only way to get the first window and
/// the Config in one negotiation).
pub(crate) fn bootstrap_display(event_loop: &ActiveEventLoop, attrs: WindowAttributes) -> Result<(Window, Config), String> {
    // ANGLE's EGL implementation on Windows only ever advertises GLES
    // renderable-type bits (never desktop GL), so requesting Api::OPENGL
    // there yields zero matching configs. Everywhere else glutin's EGL
    // backend is fronting real desktop GL drivers, so keep requesting that.
    #[cfg(target_os = "windows")]
    let api = Api::GLES2;
    #[cfg(not(target_os = "windows"))]
    let api = Api::OPENGL;

    let template = ConfigTemplateBuilder::new()
        .with_api(api)
        .with_surface_type(ConfigSurfaceTypes::WINDOW | ConfigSurfaceTypes::PBUFFER)
        .with_alpha_size(8)
        .with_depth_size(0)
        .with_stencil_size(0);

    let (window, gl_config) = DisplayBuilder::new()
        .with_preference(ApiPreference::PreferEgl)
        .with_window_attributes(Some(attrs))
        .build(event_loop, template, |mut configs| {
            // DisplayBuilder's picker callback must return a Config, not a
            // Result: an empty match here means the template's constraints
            // (see above) can't be satisfied on this driver at all.
            configs.next().unwrap_or_else(|| {
                panic!("EGL returned zero configs matching the WINDOW|PBUFFER template (requested api: {api:?}, alpha8/depth0/stencil0)")
            })
        })
        .map_err(|e| format!("failed to bootstrap EGL display/config: {e}"))?;
    let window = window.ok_or_else(|| "DisplayBuilder did not create the requested window".to_string())?;
    Ok((window, gl_config))
}

pub(crate) fn create_window_slot(display: &Display, gl_config: &Config, window: Window) -> Result<WindowSlot, String> {
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

/// Two windows sharing one GL context, paced explicitly
/// instead of relying on vsync: a Wayland surface that stops being visible
/// stops receiving frame callbacks, so `SwapInterval::Wait` on it would
/// block `swap_buffers` and freeze the whole single-threaded render loop,
/// output window included.
pub(crate) fn bootstrap(event_loop: &ActiveEventLoop) -> Result<AppState, String> {
    let control_attrs = Window::default_attributes()
        .with_title("OpenDrop: Control")
        .with_transparent(false);
    let (control_window, gl_config) = bootstrap_display(event_loop, control_attrs)?;
    let display = gl_config.display();

    let output_attrs = Window::default_attributes()
        .with_title("OpenDrop: Output")
        .with_transparent(false);
    let output_window = glutin_winit::finalize_window(event_loop, output_attrs, &gl_config)
        .map_err(|e| format!("failed to create output window: {e}"))?;

    let raw_window_handle = control_window
        .window_handle()
        .map_err(|e| format!("control window has no raw handle: {e}"))?
        .as_raw();
    // See engine/src/deck.rs's anchor-context creation for why this asks
    // for GLES 3.1 explicitly instead of leaving it to `Gles(None)`.
    #[cfg(target_os = "windows")]
    let context_api = ContextApi::Gles(Some(Version::new(3, 1)));
    #[cfg(not(target_os = "windows"))]
    let context_api = ContextApi::OpenGl(Some(Version::new(3, 3)));

    let ctx_attrs = ContextAttributesBuilder::new()
        .with_debug(cfg!(debug_assertions))
        .with_profile(GlProfile::Core)
        .with_context_api(context_api)
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
    // Every deck starts on neutral Time params and no q-var watches
    // (`Show::default`), and the 4 bootstrap presets go through the same
    // patched load path as every later one; see `load_preset_onto_deck` on
    // why that is not optional.
    let neutral_time_params = opendrop_core::time_params::DeckTimeParams::default();
    let unwatched_q_vars = opendrop_core::q_vars::default_q_var_params();
    let mut param_last_sent = [channel_values(&neutral_time_params, &unwatched_q_vars); deck::DECK_COUNT];
    let mut deck_q_var_watches = [unwatched_q_vars.enabled; deck::DECK_COUNT];
    // Seeds `AppState::preset_errors` below. A bootstrap preset projectM
    // rejects must not abort startup: that deck just comes up empty, exactly
    // as it did before `Deck` reported load failures at all. But it must not
    // be invisible either: without this the deck would show the idle logo for
    // the whole session with nothing anywhere saying why.
    let mut preset_errors: HashMap<usize, String> = HashMap::new();
    for (i, dk) in decks.iter().enumerate() {
        dk.context.make_current(&dk.surface).map_err(|e| format!("make_current(deck {i}) failed: {e}"))?;
        match load_preset_onto_deck(
            dk,
            &presets[i],
            &neutral_time_params,
            &unwatched_q_vars,
            &mut param_last_sent[i],
            &mut deck_q_var_watches[i],
            false,
        ) {
            Ok(()) => println!("[app] deck {i} preset: {}", presets[i].display()),
            Err(e) => {
                eprintln!("[app] deck {i} bootstrap preset {} failed to load: {e}", presets[i].display());
                preset_errors.insert(i, e);
            }
        }
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
    let monitor_refresh_interval = Duration::from_secs_f64(1000.0 / refresh_millihertz as f64);

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
    // shared ownership. The 4 Deck::gl contexts stay owned, unshared; see
    // engine/src/deck.rs.
    let gl = Arc::new(gl);
    let version = unsafe { gl.get_parameter_string(glow::VERSION) };
    println!("[app] main context: GL {version}");

    // shader_version=None (auto-detect), native_pixels_per_point=None (no
    // forced ratio), dithering=true, same as egui_glow's own example
    // (examples/pure_glow.rs:188).
    let mut egui_glow = egui_glow::EguiGlow::new(event_loop, Arc::clone(&gl), None, None, true);

    // Load the persisted `ui.json` once, here, before the theme, active panel
    // and stage mode it carries are wired onto the Context and `AppState`
    // below. The runtime theme-switch handler saves through
    // `config::save_config`, so without this load that preference would be
    // written and then ignored on every restart.
    let ui_config = config::config_file_path().map(|p| config::load_config(&p)).unwrap_or_default();
    let fps_override = ui_config.fps_override.map(config::sanitize_fps);
    let refresh_interval = match fps_override {
        Some(fps) => Duration::from_secs_f64(1.0 / f64::from(fps)),
        None => monitor_refresh_interval,
    };
    let ui_scale = config::sanitize_ui_scale(ui_config.ui_scale);
    egui_glow.egui_ctx.set_zoom_factor(ui_scale);

    // Wire the persisted (or default)
    // theme + fonts onto the bootstrap Context, once. `set_fonts` is only
    // ever called here: a runtime theme change re-applies
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
    // for `widgets.rs` and the runtime theme switch, which resolve theme
    // tokens for custom-painted widgets not driven by egui's `Style` directly.
    egui_glow.egui_ctx.data_mut(|d| d.insert_temp(egui::Id::new(theme::THEME_ID_KEY), default_theme_id));

    // Register each deck's live GPU texture with egui's painter once, here
    // at bootstrap; never per-frame, which would leak a new texture handle
    // into the painter every tick. `register_native_texture` touches no GL
    // state and is safe to call at any time (egui_glow 0.36.1
    // src/painter.rs:649-655). `glow::Texture` is a type alias that
    // resolves to `glow::NativeTexture` for a native (non-wasm) `glow::
    // Context` (glow 0.17 src/native.rs:205), the same type as
    // `Deck::texture`, so it passes through directly with no `.0`.
    let deck_tex_ids: [egui::TextureId; 4] =
        std::array::from_fn(|i| egui_glow.painter.register_native_texture(decks[i].texture));

    let deck_video_texture: [glow::NativeTexture; 4] = std::array::from_fn(|_| {
        create_empty_video_texture(&gl, opendrop_io::video_capture::CAPTURE_W, opendrop_io::video_capture::CAPTURE_H)
    });
    let deck_video_tex_ids: [egui::TextureId; 4] =
        std::array::from_fn(|i| egui_glow.painter.register_native_texture(deck_video_texture[i]));

    // Compositor FBO/texture belong to whichever context is current at
    // creation: main_ctx is current here (on control's surface), same as
    // it will be every time the compositor's FBO is touched later.
    let compositor = Compositor::new(&gl)?;
    let blit_control_timer = PassTimer::new(&gl).map_err(|e| format!("blit_control_timer: {e}"))?;
    let blit_output_timer = PassTimer::new(&gl).map_err(|e| format!("blit_output_timer: {e}"))?;

    // One FrameReadback per shared texture (compositor + 4 decks).
    // Built here, main_ctx still current on control's surface; FrameReadback::
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
    // (see `v4l2_frame_tx`'s doc comment on `AppState`).
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
    // context; that's the whole point of sharing. Checked against deck
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
    // (`main.cjs:425-429`; see `opendrop_io::chat`'s module doc comment).
    // `chat_tx` is cloned once per platform thread below; `chat_events` is
    // the receiving end, stored on `AppState`.
    let (chat_tx, chat_events) = mpsc::channel();

    let audio = opendrop_audio::spawn_capture();
    let audio_gain_db = ui_config.audio_gain_db;
    let audio_agc = ui_config.audio_agc;
    let beat_sensitivity = ui_config.beat_sensitivity;
    audio.set_gain_db(audio_gain_db);
    audio.set_agc(audio_agc);

    let mut show = Show::default();
    show.beat_detector.set_sensitivity(beat_sensitivity);
    // `core` is zero-I/O and has no clock of
    // its own, so its playlist engines start from a fixed, hardcoded RNG
    // seed; shuffle mode would otherwise replay the exact same sequence
    // every single app launch. Real per-launch entropy is supplied here,
    // the one place in the codebase allowed to touch a wall clock for this.
    let bootstrap_rng_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    show.reseed_rng(bootstrap_rng_seed);
    show.preset_catalog = catalog;

    // Enumerated once, like `input_devices` above: a sysfs scan
    // per frame would be pointless, and the camera list doesn't change
    // mid-session any more than the audio device list does. The first
    // camera is pre-selected so the panel's "Use camera" button works in
    // one click on the common single-webcam machine.
    let video_cameras = opendrop_io::video_capture::list_cameras();
    let video_camera_device = video_cameras.first().map(|c| c.id.clone()).unwrap_or_default();

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
        // Receivers moved into the NDI thread here, once; they are no
        // longer needed as `AppState` fields once `spawn` takes ownership.
        // Discovery used to start unconditionally here, right after
        // spawn, so the source dropdown would "just work" whenever the NDI
        // panel was opened. In practice that runs `in_::find`'s SDK call
        // every ~5ms for the entire session, on every machine, whether or
        // not NDI is ever used (see `ndi::in_::find`'s doc comment). Now
        // started/stopped from the `about_to_wait` gate near the thumbnail
        // pump instead, keyed on `Panel::NdiIn` being on screen.
        ndi: opendrop_io::ndi::spawn(compositor_frame_rx, deck_frame_rx),
        ndi_composite_active: false,
        ndi_deck_active: [false; deck::DECK_COUNT],
        spout: opendrop_io::spout::spawn(),
        spout_composite_active: false,
        spout_deck_active: [false; deck::DECK_COUNT],
        ndi_in_texture: None,
        ndi_in_selected_source: None,
        spout_in: opendrop_io::spout_in::spawn(),
        spout_in_texture: None,
        spout_in_selected: None,
        ndi_discovery_active: false,
        overlay_assets: HashMap::new(),
        next_overlay_id: 0,
        overlay_textures: HashMap::new(),
        last_beat_at: None,
        v4l2_active: false,
        v4l2: opendrop_io::v4l2loopback::spawn(v4l2_frame_rx),
        v4l2_device: None,
        video: opendrop_io::video_capture::spawn(),
        video_input: None,
        video_texture: None,
        video_frame_seq: 0,
        deck_video_capture: std::array::from_fn(|_| opendrop_io::video_capture::spawn()),
        deck_video_input: [const { None }; 4],
        deck_video_frame_seq: [0; 4],
        deck_video_texture,
        deck_video_tex_ids,
        // Scanned once here (see the field's doc comment); the panel's
        // Rescan button is the only thing that re-reads either directory.
        video_clips: video_clips::scan_clips(),
        video_cameras,
        video_camera_device,
        video_local_error: None,
        video_panel_target: ui::video::VideoPanelTarget::Global,
        cloud_presets: opendrop_io::cloud_presets::spawn(),
        cloud_presets_api_url: ui_config.cloud_presets_api_url.clone().unwrap_or_default(),
        cloud_presets_token_input: String::new(),
        cloud_presets_secret_error: None,
        cloud_presets_rename: None,
        cloud_video: opendrop_io::cloud_video::spawn(),
        cloud_video_api_url: ui_config.cloud_video_api_url.clone().unwrap_or_default(),
        gl,
        egui_glow,
        refresh_interval,
        fps_override,
        last_session_save: Instant::now(),
        ui_scale,
        next_frame_at: Instant::now(),
        t0: Instant::now(),
        audio,
        // Enumerated once here at bootstrap and cached; the Audio
        // panel never calls `list_input_devices()` itself: the list
        // doesn't change mid-session.
        input_devices: opendrop_audio::list_input_devices(),
        // Panel settings restored from the same `ui_config` loaded above,
        // not just navigation state.
        selected_input_device: ui_config.audio_input_device.clone(),
        audio_gain_db,
        audio_agc,
        beat_sensitivity,
        last_vu_level: 0.0,
        deck_next_render_at: [Instant::now(); deck::DECK_COUNT],
        invisible_mode: ui_config.invisible_mode,
        pending_mesh_size: [None; deck::DECK_COUNT],
        param_last_sent,
        param_cursor: [0; deck::DECK_COUNT],
        deck_q_var_watches,
        show,
        registry: create_default_registry(),
        // Empty `ui_config.keymap` means "no persisted remapping yet" (first
        // launch, or a `ui.json` predating this step). Falls back to the
        // hardcoded defaults rather than starting with no bindings at all.
        // A non-empty one fully replaces the defaults: `exiting` persists
        // `state.keymap` in its entirety (see that fn), so whatever's saved
        // already reflects every Learn/Clear the user has ever done, not a
        // diff against `default_keymap()`.
        keymap: if ui_config.keymap.is_empty() { keymap::default_keymap() } else { keymap::keymap_from_wire(&ui_config.keymap) },
        keymap_learning: None,
        blit_control_timer,
        blit_output_timer,
        last_output_swap_at: None,
        last_wall_ms: None,
        perf_tick: 0,
        preflight_tx,
        preflight_rx,
        path_by_name,
        pending_validations: HashSet::new(),
        preset_errors,
        // Seeded from the 4 presets the bootstrap loop above actually
        // loaded: leaving these empty until the first UI-driven load meant
        // every deck card started blank even though a preset was running
        // on it.
        deck_preset_names: std::array::from_fn(|i| preset_display_name(&preset_root, &presets[i])),
        transition_seconds: 0.0,
        share_set_name: String::new(),
        deck_tex_ids,
        // Restored from
        // the same `ui_config` loaded above, instead of always starting on
        // the hardcoded defaults.
        active_panel: ui_config.active_panel.into(),
        stage_mode: ui_config.stage_mode,
        presets_drawer_open: false,
        preset_search_query: String::new(),
        preset_search_cache: ui::preset_browser::SearchCache::default(),
        favorite_presets: ui_config.favorite_presets.clone(),
        favorites_only: false,
        scan: crate::preset_scan::ScanHandle::new(
            crate::preset_scan::scan_file_path().map(|p| crate::preset_scan::load_table(&p)).unwrap_or_default(),
        ),
        hide_flagged: ui_config.hide_flagged_presets,
        autopilot: {
            let mut pilot = opendrop_core::autopilot::Autopilot::default();
            pilot.reseed(bootstrap_rng_seed);
            pilot
        },
        energy_tracker: Default::default(),
        preset_annotations: opendrop_core::preset_annotations::PresetAnnotations::from_parts(
            ui_config.preset_tags.clone(),
            ui_config.preset_notes.clone(),
        ),
        preset_history: opendrop_core::preset_annotations::PresetHistory::from_names(ui_config.preset_history.clone()),
        annotation_edit: Default::default(),
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
        osc_out: None,
        osc_out_host: ui_config.osc_out_host.clone(),
        osc_out_port: ui_config.osc_out_port,
        osc_out_error: None,
        rkbx_link: opendrop_io::rkbx_link::spawn(),
        rkbx_link_port: ui_config.rkbx_link_port,
        rkbx_mapping_error: None,
        rkbx_sync: [None, None, None, None],
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
