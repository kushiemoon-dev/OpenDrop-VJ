use glutin::prelude::*;
use std::num::NonZeroU32;
use std::path::Path;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

mod bootstrap;
mod config;
mod egl_headless;
mod frame;
mod deck_video;
mod keymap;
mod overlays;
mod panel;
mod paths;
mod preset_load;
mod state;
mod preflight;
mod thumbnail_child;
mod ui_root;
mod thumbnails;
mod theme;
mod ui;
mod video_clips;

pub(crate) use panel::Panel;
use bootstrap::{bootstrap, WindowSlot};
use state::{App, AppState, InvisibleMode};
use frame::background::service_background_work;
use frame::composite::composite_frame;
use frame::deck_video_sync::sync_deck_video;
use frame::decks::render_decks;
use frame::drain::drain_inputs;
use frame::outputs::feed_outputs;
use frame::present::present_frame;
use frame::tick::{tick_show, TickOutput};
use frame::sources::upload_source_frames;
use frame::ui_pass::run_ui_pass;

/// ponytail: paced off the control window's monitor only, read once at
/// bootstrap. A VJ setup can have control and output on different-refresh
/// monitors; revisit if that ever causes visible judder on the output side.
const FALLBACK_REFRESH_MILLIHERTZ: u32 = 60_000;

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

        // egui first, control window only; output never carries UI.
        if window_id == state.control.window.id() {
            let _egui_response = state.egui_glow.on_window_event(&state.control.window, &event);
        }

        // Handled regardless of which window has focus; both windows show
        // the same show state, so the keymap isn't per-window. Gated on
        // egui_wants_keyboard_input() (not EventResponse.consumed, which is
        // also true for e.g. a mouse click on a button) so debug shortcuts
        // keep working except while an egui text widget (e.g. the preset
        // browser search) actually has focus.
        if let WindowEvent::KeyboardInput { event: key_event, .. } = &event {
            // `!key_event.repeat` excludes
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
                // The Keymap panel's
                // Learn button only records which command is waiting
                // (`keymap_learning`); the key that commits the binding is
                // captured right here, on the very next accepted press,
                // intercepted ahead of normal dispatch (including the F11
                // Stage-mode shortcut below) rather than also firing
                // whatever that key already did/does.
                if let Some(cmd_id) = state.keymap_learning.take() {
                    state.keymap.insert(key_event.logical_key.clone(), cmd_id);
                } else {
                    if let Some(&cmd_id) = state.keymap.get(&key_event.logical_key) {
                        state.registry.dispatch(cmd_id, 1.0, &mut state.show);
                    }

                    // Stage mode is
                    // a transient UI bool on `AppState`, not a `CommandId`:
                    // matched directly against the logical key here rather
                    // than routed through `state.keymap`/`state.registry.
                    // dispatch`, under the same guard as the dispatch above.
                    if key_event.logical_key == Key::Named(NamedKey::F11) {
                        state.stage_mode = !state.stage_mode;
                    }
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

        drain_inputs(state);

        let now = Instant::now();
        // Wayland can wake this loop for reasons unrelated to pacing (e.g.
        // buffer-release protocol traffic generated by our own previous
        // swap); about_to_wait fires far more often than the WaitUntil
        // deadline requests. Gating the render on next_frame_at, instead of
        // rendering on every call, is what keeps that from turning into a
        // self-sustaining busy loop (measured: ~10 kHz without this gate).
        if now >= state.next_frame_at {
            let TickOutput { mut layer_inputs, audio, now_ms, ndi_active } = tick_show(state, now);

            render_decks(state, now, &audio, &layer_inputs);

            service_background_work(state);

            upload_source_frames(state, ndi_active);

            sync_deck_video(state, now);

            let lowest_active = composite_frame(state, now, now_ms, &mut layer_inputs);

            feed_outputs(state);

            run_ui_pass(state, event_loop);

            present_frame(state, now, lowest_active);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(state.next_frame_at));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &mut self.state {
            // Persist
            // `active_panel`/`stage_mode` on exit, matching Step 7's
            // original "How" text ("write on exiting ... and
            // immediately on every runtime theme change"). Same
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
            ui_config.rkbx_link_port = state.rkbx_link_port;
            ui_config.obs_host = state.obs_host.clone();
            ui_config.obs_port = state.obs_port;
            ui_config.twitch_channel = state.twitch_channel.clone();
            ui_config.kick_channel = state.kick_channel.clone();
            ui_config.cloud_presets_api_url =
                if state.cloud_presets_api_url.trim().is_empty() { None } else { Some(state.cloud_presets_api_url.clone()) };
            ui_config.cloud_video_api_url =
                if state.cloud_video_api_url.trim().is_empty() { None } else { Some(state.cloud_video_api_url.clone()) };
            ui_config.invisible_mode = state.invisible_mode;
            ui_config.keymap = keymap::keymap_to_wire(&state.keymap);
            config::save_config(config_path.as_deref(), &ui_config);

            state.egui_glow.destroy();
        }
    }
}

// projectM only ships `projectm_set_log_callback` from 4.2.0 onward (the
// pinned Windows/GLES overlay port); the Arch `projectm-4.pc` package this
// links against on Linux is still 4.1.6, so this diagnostic hook is
// Windows-only. Without it, every internal GLResolver/GladLoader failure
// reason (e.g. "no current GL context") is silently discarded, and
// `projectm_create()` returning NULL carries no explanation at all.
#[cfg(target_os = "windows")]
unsafe extern "C" fn log_projectm_message(
    message: *const std::os::raw::c_char,
    log_level: opendrop_engine::ffi::projectm_log_level,
    _user_data: *mut std::os::raw::c_void,
) {
    let msg = unsafe { std::ffi::CStr::from_ptr(message) }.to_string_lossy();
    eprintln!("[projectM level={log_level}] {msg}");
}

fn main() {
    #[cfg(target_os = "windows")]
    unsafe {
        opendrop_engine::ffi::projectm_set_log_callback(Some(log_projectm_message), false, std::ptr::null_mut());
    }

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
    use opendrop_engine::compositor::{LayerInput, COMP_H, COMP_W};

    /// Step 10 (Strobe panel): headless GL smoke test for
    /// `Compositor::render_strobe_flash`. This codebase has no automated
    /// UI/GPU test harness (plan Convention D, manual live-app
    /// verification is the norm), but `egl_headless` (already used by
    /// `--preflight-check`/`--render-thumbnail`) gives a real, windowless
    /// GL context to any test binary, which `cargo build`/`clippy` can't
    /// substitute for: a GLSL compile/link error is a runtime-only
    /// failure. Each test builds its own context inline rather than
    /// through a shared helper returning just `gl`: `khronos_egl`'s
    /// wrapper types have no `Drop` of their own, but the `DynamicInstance`
    /// (`egl_inst`) owns the dlopen'd `libEGL.so.1` handle and drops it
    /// (dlclose) when it goes out of scope; dropping it before `gl`'s
    /// loaded function pointers are done being used would be undefined
    /// behavior, so `egl_inst`/`display`/`ctx`/`pb` all stay bound for the
    /// whole test.
    mod compositor_strobe_flash_smoke_test {
        use super::*;
        use glow::HasContext;
        use opendrop_engine::compositor::Compositor;

        #[test]
        fn compositor_builds_and_renders_the_strobe_flash_without_a_gl_error() {
            let (egl_inst, display, config) = egl_headless::init_egl();
            let ctx = egl_headless::create_context(&egl_inst, display, config);
            let pb = egl_headless::create_pbuffer(&egl_inst, display, config, 64, 64);
            egl_inst.make_current(display, Some(pb), Some(pb), Some(ctx)).expect("eglMakeCurrent failed");
            let gl = egl_headless::make_gl(&egl_inst);

            let mut compositor =
                Compositor::new(&gl).expect("Compositor::new (including the strobe shader) should build on a real driver");
            compositor.begin_frame(&gl);
            // Drains one pre-existing, unrelated GL error this test
            // uncovered: `PassTimer::poll` (`engine::timing`, not touched
            // by this step) calls `glGetQueryObjectuiv` on a query object
            // that has never yet had a `glBeginQuery`/`glEndQuery` pair,
            // disallowed by the GL spec, which every `PassTimer`'s very
            // first `begin()` call hits once. Nothing before this step ran
            // `Compositor` against a real GL context to surface it; see
            // this step's report for the finding. Draining it here keeps
            // this test's assertion scoped to what it actually exercises:
            // `render_strobe_flash`, not `timing.rs`'s pre-existing bug.
            unsafe { gl.get_error() };
            compositor.render_strobe_flash(&gl, [1.0, 0.5, 0.25], 0.75);
            compositor.end_frame(&gl);

            let err = unsafe { gl.get_error() };
            assert_eq!(err, glow::NO_ERROR, "GL error after render_strobe_flash: 0x{err:x}");
        }

        #[test]
        fn zero_intensity_is_a_no_op_and_leaves_no_gl_error() {
            let (egl_inst, display, config) = egl_headless::init_egl();
            let ctx = egl_headless::create_context(&egl_inst, display, config);
            let pb = egl_headless::create_pbuffer(&egl_inst, display, config, 64, 64);
            egl_inst.make_current(display, Some(pb), Some(pb), Some(ctx)).expect("eglMakeCurrent failed");
            let gl = egl_headless::make_gl(&egl_inst);

            let mut compositor = Compositor::new(&gl).expect("Compositor::new should build");
            compositor.begin_frame(&gl);
            // See the sibling test's comment: drains the pre-existing
            // `PassTimer` first-use GL error, unrelated to this test.
            unsafe { gl.get_error() };
            compositor.render_strobe_flash(&gl, [1.0, 1.0, 1.0], 0.0);
            compositor.end_frame(&gl);

            let err = unsafe { gl.get_error() };
            assert_eq!(err, glow::NO_ERROR, "GL error after a 0-intensity render_strobe_flash: 0x{err:x}");
        }
    }

    /// Step 12 (Overlays panel): headless GL coverage for the overlay
    /// compositing primitive: `overlay_texture::upload_rgba` plus
    /// `Compositor::composite_overlay`. Unlike the strobe smoke tests
    /// above, these read pixels back out of the composite FBO and assert
    /// on their values: the whole point of this pass is *where* a sprite
    /// lands and *how* it blends, neither of which a "no GL error" check
    /// can see, and neither of which `cargo build` can see at all (the
    /// shaders are compiled at runtime, and the geometry math only exists
    /// once a driver has run it).
    ///
    /// Same context-lifetime constraint as the strobe module above: the
    /// `DynamicInstance` owns the dlopen'd `libEGL.so.1` and dlcloses it
    /// on drop, so it must outlive every use of `gl`'s loaded function
    /// pointers, hence [`HeadlessGl`], whose field order makes that
    /// ordering explicit instead of relying on each test binding four
    /// locals in the right order.
    mod compositor_overlay_gl_tests {
        use super::*;
        use glow::HasContext;
        use khronos_egl as egl;
        use opendrop_engine::compositor::{
            overlay_center_px, overlay_quad_half_size_px, Compositor, OverlayBlendMode, OverlayLayerInput,
        };
        use opendrop_engine::overlay_texture::{self, RgbaImage};

        /// A windowless GL context plus everything that must stay alive
        /// for it to keep working. Fields drop in declaration order, so
        /// `gl` goes before `_egl_inst` unloads `libEGL`.
        struct HeadlessGl {
            gl: glow::Context,
            _pb: egl::Surface,
            _ctx: egl::Context,
            _display: egl::Display,
            _egl_inst: egl_headless::Egl,
        }

        fn headless_gl() -> HeadlessGl {
            let (egl_inst, display, config) = egl_headless::init_egl();
            let ctx = egl_headless::create_context(&egl_inst, display, config);
            let pb = egl_headless::create_pbuffer(&egl_inst, display, config, 64, 64);
            egl_inst.make_current(display, Some(pb), Some(pb), Some(ctx)).expect("eglMakeCurrent failed");
            let gl = egl_headless::make_gl(&egl_inst);
            HeadlessGl { gl, _pb: pb, _ctx: ctx, _display: display, _egl_inst: egl_inst }
        }

        /// `begin_frame` + the one-time drain of `PassTimer::poll`'s
        /// pre-existing first-use `GL_INVALID_OPERATION` (documented by
        /// Step 10's own tests, in `engine::timing`, untouched here).
        fn begin_clean_frame(gl: &glow::Context, compositor: &mut Compositor) {
            compositor.begin_frame(gl);
            unsafe { gl.get_error() };
        }

        /// One RGBA pixel of the composite FBO, in GL's own coordinates
        /// (origin bottom-left).
        fn read_composite_pixel(gl: &glow::Context, compositor: &Compositor, x: i32, y: i32) -> [u8; 4] {
            let mut px = [0u8; 4];
            unsafe {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(compositor.fbo));
                gl.read_pixels(
                    x,
                    y,
                    1,
                    1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut px)),
                );
            }
            px
        }

        fn solid_rgba(width: u32, height: u32, rgba: [u8; 4]) -> RgbaImage {
            RgbaImage {
                width,
                height,
                pixels: rgba.iter().copied().cycle().take(width as usize * height as usize * 4).collect(),
            }
        }

        fn input(texture: glow::NativeTexture, tex_w: u32, tex_h: u32) -> OverlayLayerInput {
            OverlayLayerInput {
                texture,
                tex_w,
                tex_h,
                x: 0.5,
                y: 0.5,
                scale: 1.0,
                rotation_deg: 0.0,
                opacity: 1.0,
                blend_mode: OverlayBlendMode::Normal,
            }
        }

        fn assert_channel_near(actual: [u8; 4], expected_rgb: [u8; 3], mode: OverlayBlendMode) {
            for (i, expected) in expected_rgb.iter().enumerate() {
                let delta = actual[i] as i32 - *expected as i32;
                assert!(
                    delta.abs() <= 3,
                    "{:?}: channel {i} was {}, expected ~{expected} (whole pixel {actual:?})",
                    mode,
                    actual[i]
                );
            }
        }

        /// Fills the whole composite with an opaque flat color, by drawing
        /// a 1x1 sprite blown up well past the frame: the simplest way to
        /// establish a known backdrop for the blend-mode assertions,
        /// through the very pass under test (so a bug in it would be
        /// caught by the "backdrop is what we asked for" check below
        /// rather than silently skewing every expectation).
        fn fill_composite(gl: &glow::Context, compositor: &mut Compositor, rgb: [u8; 3]) {
            let img = solid_rgba(1, 1, [rgb[0], rgb[1], rgb[2], 255]);
            let tex = overlay_texture::upload_rgba(gl, &img).expect("1x1 upload should work");
            let mut layer = input(tex, 1, 1);
            layer.scale = 4000.0;
            compositor.composite_overlay(gl, &layer);
            unsafe { gl.delete_texture(tex) };
        }

        #[test]
        fn an_opaque_sprite_lands_at_its_normalized_position_and_nowhere_else() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build every program");
            begin_clean_frame(gl, &mut compositor);

            // 64x64 white sprite at (0.25, 0.25), upper-LEFT quadrant in
            // `Overlay`'s CSS convention, which is high y in GL's.
            let img = solid_rgba(64, 64, [255, 255, 255, 255]);
            let tex = overlay_texture::upload_rgba(gl, &img).expect("upload should work");
            let mut layer = input(tex, 64, 64);
            layer.x = 0.25;
            layer.y = 0.25;
            compositor.composite_overlay(gl, &layer);
            compositor.end_frame(gl);

            let (cx, cy) = overlay_center_px(0.25, 0.25);
            assert_eq!((cx, cy), (480.0, 810.0));
            assert_eq!(read_composite_pixel(gl, &compositor, cx as i32, cy as i32), [255, 255, 255, 255]);
            // Just inside the 32 px half-extent, and just outside it.
            assert_eq!(read_composite_pixel(gl, &compositor, cx as i32 + 30, cy as i32), [255, 255, 255, 255]);
            assert_eq!(read_composite_pixel(gl, &compositor, cx as i32 + 40, cy as i32), [0, 0, 0, 0]);
            // The mirror position (0.75, 0.75) must be untouched; this is
            // what catches a y-flip or an x/y swap.
            assert_eq!(read_composite_pixel(gl, &compositor, 1440, 270), [0, 0, 0, 0]);

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        #[test]
        fn uploaded_pixels_reach_the_composite_in_the_right_orientation() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");
            begin_clean_frame(gl, &mut compositor);

            // 2x2, row 0 = TOP: red top-left, green top-right,
            // blue bottom-left, white bottom-right.
            let img = RgbaImage {
                width: 2,
                height: 2,
                pixels: vec![
                    255, 0, 0, 255, // (0,0) top-left
                    0, 255, 0, 255, // (1,0) top-right
                    0, 0, 255, 255, // (0,1) bottom-left
                    255, 255, 255, 255, // (1,1) bottom-right
                ],
            };
            let tex = overlay_texture::upload_rgba(gl, &img).expect("upload should work");
            let mut layer = input(tex, 2, 2);
            // half-extent = tex_w * scale * 0.5 = scale for a 2x2, so
            // scale 400 gives an 800x800 quad centered on the frame:
            // x in [560, 1360], y in [140, 940].
            layer.scale = 400.0;
            compositor.composite_overlay(gl, &layer);
            compositor.end_frame(gl);

            assert_eq!(overlay_quad_half_size_px(2, 2, 400.0), (400.0, 400.0));
            // Sampled 10% in from each corner: with a 2x2 texture,
            // CLAMP_TO_EDGE + LINEAR make anything outside the texel
            // centers (uv 0.25/0.75) exactly that corner texel.
            // GL reads bottom-up, so high y is the image's TOP row.
            assert_eq!(read_composite_pixel(gl, &compositor, 640, 860), [255, 0, 0, 255], "top-left should be red");
            assert_eq!(read_composite_pixel(gl, &compositor, 1280, 860), [0, 255, 0, 255], "top-right should be green");
            assert_eq!(read_composite_pixel(gl, &compositor, 640, 220), [0, 0, 255, 255], "bottom-left should be blue");
            assert_eq!(
                read_composite_pixel(gl, &compositor, 1280, 220),
                [255, 255, 255, 255],
                "bottom-right should be white"
            );

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        #[test]
        fn rotation_turns_the_quad_about_its_own_center() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");

            // 64x8 sprite: 64 px wide, 8 px tall, centered. Unrotated it
            // covers (960±32, 540±4); at 90 degrees it covers
            // (960±4, 540±32).
            let img = solid_rgba(64, 8, [255, 255, 255, 255]);
            let tex = overlay_texture::upload_rgba(gl, &img).expect("upload should work");

            begin_clean_frame(gl, &mut compositor);
            compositor.composite_overlay(gl, &input(tex, 64, 8));
            compositor.end_frame(gl);
            assert_eq!(read_composite_pixel(gl, &compositor, 985, 540)[3], 255, "unrotated: wide");
            assert_eq!(read_composite_pixel(gl, &compositor, 960, 565)[3], 0, "unrotated: not tall");

            begin_clean_frame(gl, &mut compositor);
            let mut rotated = input(tex, 64, 8);
            rotated.rotation_deg = 90.0;
            compositor.composite_overlay(gl, &rotated);
            compositor.end_frame(gl);
            assert_eq!(read_composite_pixel(gl, &compositor, 985, 540)[3], 0, "rotated: no longer wide");
            assert_eq!(read_composite_pixel(gl, &compositor, 960, 565)[3], 255, "rotated: now tall");

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        #[test]
        fn every_blend_mode_produces_its_documented_result_over_a_known_backdrop() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");

            // Backdrop 64/255 = 0.25098, sprite 192/255 = 0.75294;
            // deliberately asymmetric so `overlay` and `hard-light`
            // (which are the same function with its arguments swapped)
            // land on visibly different values.
            let img = solid_rgba(8, 8, [192, 192, 192, 255]);
            let tex = overlay_texture::upload_rgba(gl, &img).expect("upload should work");

            // b = 0.25098, s = 0.75294:
            //   normal      = s                        -> 192
            //   screen      = s + b*(1-s)              -> 208
            //   plus-lighter= b + s (clamped)          -> 255
            //   multiply    = b*s                      -> 48
            //   hard-light  = 1 - 2*(1-b)*(1-s)        -> 161
            //   overlay     = 2*s*b   (b<=0.5 branch,
            //                          arguments swapped) ->  96
            let expected: [(OverlayBlendMode, [u8; 3]); 6] = [
                (OverlayBlendMode::Normal, [192, 192, 192]),
                (OverlayBlendMode::Screen, [208, 208, 208]),
                (OverlayBlendMode::PlusLighter, [255, 255, 255]),
                (OverlayBlendMode::Multiply, [48, 48, 48]),
                (OverlayBlendMode::HardLight, [161, 161, 161]),
                (OverlayBlendMode::Overlay, [96, 96, 96]),
            ];

            for (mode, want) in expected {
                begin_clean_frame(gl, &mut compositor);
                fill_composite(gl, &mut compositor, [64, 64, 64]);
                // The backdrop itself must be what we asked for, or every
                // expectation below is measured against the wrong base.
                assert_eq!(
                    read_composite_pixel(gl, &compositor, 960, 540),
                    [64, 64, 64, 255],
                    "backdrop fill went wrong before testing {mode:?}"
                );

                let mut layer = input(tex, 8, 8);
                layer.blend_mode = mode;
                compositor.composite_overlay(gl, &layer);
                compositor.end_frame(gl);

                assert_channel_near(read_composite_pixel(gl, &compositor, 960, 540), want, mode);
                assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR, "GL error after {mode:?}");
            }

            unsafe { gl.delete_texture(tex) };
        }

        #[test]
        fn opacity_scales_a_normal_blend_toward_the_backdrop() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");
            begin_clean_frame(gl, &mut compositor);
            fill_composite(gl, &mut compositor, [0, 0, 0]);

            let img = solid_rgba(8, 8, [255, 255, 255, 255]);
            let tex = overlay_texture::upload_rgba(gl, &img).expect("upload should work");
            let mut layer = input(tex, 8, 8);
            layer.opacity = 0.5;
            compositor.composite_overlay(gl, &layer);
            compositor.end_frame(gl);

            // 1.0*0.5 + 0.0*0.5 = 0.5 -> 128
            assert_channel_near(read_composite_pixel(gl, &compositor, 960, 540), [128, 128, 128], layer.blend_mode);
            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        #[test]
        fn a_sub_floor_opacity_or_a_zero_scale_draws_nothing_at_all() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");
            let img = solid_rgba(64, 64, [255, 255, 255, 255]);
            let tex = overlay_texture::upload_rgba(gl, &img).expect("upload should work");

            for mutate in [
                (|l: &mut OverlayLayerInput| l.opacity = 0.0) as fn(&mut OverlayLayerInput),
                |l: &mut OverlayLayerInput| l.opacity = 0.0005,
                |l: &mut OverlayLayerInput| l.scale = 0.0,
                |l: &mut OverlayLayerInput| l.scale = f32::NAN,
                |l: &mut OverlayLayerInput| l.x = f32::NAN,
                |l: &mut OverlayLayerInput| l.rotation_deg = f32::NAN,
                |l: &mut OverlayLayerInput| l.tex_w = 0,
            ] {
                begin_clean_frame(gl, &mut compositor);
                let mut layer = input(tex, 64, 64);
                mutate(&mut layer);
                compositor.composite_overlay(gl, &layer);
                compositor.end_frame(gl);
                assert_eq!(read_composite_pixel(gl, &compositor, 960, 540), [0, 0, 0, 0]);
                assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
            }

            unsafe { gl.delete_texture(tex) };
        }

        #[test]
        fn the_pass_leaves_texture_unit_0_active_for_whatever_draws_next() {
            // GL state hygiene: this is the only pass in the compositor
            // that ever touches `glActiveTexture`, and everything after it
            // in the frame (`egui_glow`'s own draw, the deck upload paths,
            // the next frame's `composite_layer`) assumes unit 0.
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");
            begin_clean_frame(gl, &mut compositor);

            let img = solid_rgba(8, 8, [255, 255, 255, 255]);
            let tex = overlay_texture::upload_rgba(gl, &img).expect("upload should work");
            let mut layer = input(tex, 8, 8);
            // The backdrop path binds unit 1, so exercise that one.
            layer.blend_mode = OverlayBlendMode::HardLight;
            compositor.composite_overlay(gl, &layer);
            compositor.end_frame(gl);

            let active = unsafe { gl.get_parameter_i32(glow::ACTIVE_TEXTURE) };
            assert_eq!(active as u32, glow::TEXTURE0, "composite_overlay left texture unit {active:#x} active");

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        #[test]
        fn a_rasterized_string_composites_as_a_sprite() {
            // End-to-end for the text half: rasterize -> upload ->
            // composite, and assert ink actually reached the composite.
            // No pixel-exact expectation (that would pin the shape of a
            // specific font's glyphs), just that the pipeline carries the
            // requested color through.
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");
            begin_clean_frame(gl, &mut compositor);

            let img = overlay_texture::rasterize_text(theme::fonts::INTER_VARIABLE, "OD", 200.0, [255, 0, 128])
                .expect("rasterizing should work");
            let (w, h_px) = (img.width, img.height);
            let tex = overlay_texture::upload_rgba(gl, &img).expect("upload should work");
            compositor.composite_overlay(gl, &input(tex, w, h_px));
            compositor.end_frame(gl);

            let (half_w, half_h) = overlay_quad_half_size_px(w, h_px, 1.0);
            let (cx, cy) = overlay_center_px(0.5, 0.5);
            let mut inked = 0;
            for dy in -(half_h as i32)..(half_h as i32) {
                for dx in -(half_w as i32)..(half_w as i32) {
                    let px = read_composite_pixel(gl, &compositor, cx as i32 + dx, cy as i32 + dy);
                    if px[3] > 200 {
                        inked += 1;
                        assert!(px[0] > 200 && px[1] < 60 && px[2] > 100, "unexpected ink color {px:?}");
                    }
                }
            }
            assert!(inked > 100, "'OD' at 200px should leave far more than {inked} solid pixels");

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }
    }

    /// Step 14 (Video panel): headless GL coverage for the video
    /// background layer: `Compositor::composite_video_layer`. Same
    /// rationale as the overlay module above: this pass's whole contract is
    /// *where in the stack* it lands and *what the beat-reactive color
    /// params do to it*, and both are runtime-only facts (the shader is
    /// compiled by the driver, and the compositing order only exists once
    /// something has actually drawn).
    ///
    /// The load-bearing test here is
    /// `the_video_layer_draws_over_a_full_opacity_deck_not_under_it`: it
    /// pins the one place this deliberately deviates from drawing the
    /// video layer before the decks (see `composite_video_layer`'s doc
    /// comment).
    mod compositor_video_layer_gl_tests {
        use super::*;
        use glow::HasContext;
        use khronos_egl as egl;
        use opendrop_core::blend::{ColorParams, DEFAULT_COLOR_PARAMS, DEFAULT_SLOT_COMPOSITE};
        use opendrop_core::video::VideoState;
        use opendrop_engine::compositor::Compositor;
        use opendrop_engine::overlay_texture::{self, RgbaImage};

        /// Same field-order-is-drop-order constraint as the overlay
        /// module's own helper; see its doc comment.
        struct HeadlessGl {
            gl: glow::Context,
            _pb: egl::Surface,
            _ctx: egl::Context,
            _display: egl::Display,
            _egl_inst: egl_headless::Egl,
        }

        fn headless_gl() -> HeadlessGl {
            let (egl_inst, display, config) = egl_headless::init_egl();
            let ctx = egl_headless::create_context(&egl_inst, display, config);
            let pb = egl_headless::create_pbuffer(&egl_inst, display, config, 64, 64);
            egl_inst.make_current(display, Some(pb), Some(pb), Some(ctx)).expect("eglMakeCurrent failed");
            let gl = egl_headless::make_gl(&egl_inst);
            HeadlessGl { gl, _pb: pb, _ctx: ctx, _display: display, _egl_inst: egl_inst }
        }

        /// `begin_frame` plus the one-time drain of `PassTimer::poll`'s
        /// pre-existing first-use `GL_INVALID_OPERATION` (documented by
        /// Step 10's tests, in `engine::timing`, untouched here).
        fn begin_clean_frame(gl: &glow::Context, compositor: &mut Compositor) {
            compositor.begin_frame(gl);
            unsafe { gl.get_error() };
        }

        fn read_center(gl: &glow::Context, compositor: &Compositor) -> [u8; 4] {
            let mut px = [0u8; 4];
            unsafe {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(compositor.fbo));
                gl.read_pixels(
                    COMP_W as i32 / 2,
                    COMP_H as i32 / 2,
                    1,
                    1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut px)),
                );
            }
            px
        }

        /// A 1x1 opaque texture: the layer is full-screen, so one texel is
        /// all any of these assertions needs.
        fn solid_texture(gl: &glow::Context, rgb: [u8; 3]) -> glow::NativeTexture {
            let img = RgbaImage { width: 1, height: 1, pixels: vec![rgb[0], rgb[1], rgb[2], 255] };
            overlay_texture::upload_rgba(gl, &img).expect("1x1 upload should work")
        }

        fn assert_near(actual: [u8; 4], expected: [u8; 4], what: &str) {
            for (i, expected) in expected.iter().enumerate() {
                let delta = actual[i] as i32 - *expected as i32;
                assert!(delta.abs() <= 2, "{what}: channel {i} was {} , expected ~{expected} (pixel {actual:?})", actual[i]);
            }
        }

        #[test]
        fn a_full_opacity_video_layer_fills_the_whole_frame_with_its_own_pixels() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build every program");
            begin_clean_frame(gl, &mut compositor);

            let tex = solid_texture(gl, [20, 200, 90]);
            compositor.composite_video_layer(gl, tex, 1.0, DEFAULT_COLOR_PARAMS);
            compositor.end_frame(gl);

            assert_near(read_center(gl, &compositor), [20, 200, 90, 255], "center");
            // Full-screen: the corners must be covered too, unlike a sprite.
            let mut corner = [0u8; 4];
            unsafe {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(compositor.fbo));
                gl.read_pixels(1, 1, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut corner)));
            }
            assert_near(corner, [20, 200, 90, 255], "corner");

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        #[test]
        fn opacity_is_the_layers_sole_strength_control() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");
            begin_clean_frame(gl, &mut compositor);

            let tex = solid_texture(gl, [255, 255, 255]);
            compositor.composite_video_layer(gl, tex, 0.5, DEFAULT_COLOR_PARAMS);
            compositor.end_frame(gl);

            // Alpha-over onto a cleared (transparent) FBO: S*a + D*(1-a).
            assert_near(read_center(gl, &compositor), [128, 128, 128, 128], "half opacity");

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        #[test]
        fn a_zero_opacity_layer_is_skipped_entirely() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");
            begin_clean_frame(gl, &mut compositor);

            let tex = solid_texture(gl, [255, 255, 255]);
            compositor.composite_video_layer(gl, tex, 0.0, DEFAULT_COLOR_PARAMS);
            compositor.end_frame(gl);

            assert_eq!(read_center(gl, &compositor), [0, 0, 0, 0]);

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        /// The ordering decision this step makes, pinned as behavior.
        ///
        /// A deck slot at full opacity covers the frame; if the video layer
        /// were composited *before* the decks, it would be completely
        /// hidden here, and "a deck at
        /// opacity 1" is the default state at either end of the crossfader,
        /// so the layer would be invisible in normal use. Drawing it after
        /// the decks is what makes its own opacity slider the sole control
        /// of how much of it shows, exactly as `compositor.ts` documents.
        #[test]
        fn the_video_layer_draws_over_a_full_opacity_deck_not_under_it() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");
            begin_clean_frame(gl, &mut compositor);

            let deck_tex = solid_texture(gl, [255, 0, 0]);
            let video_tex = solid_texture(gl, [0, 0, 255]);
            // A deck slot at full opacity, drawn first, exactly as
            // `about_to_wait` draws them.
            let deck = LayerInput { opacity: 1.0, composite: DEFAULT_SLOT_COMPOSITE, color: DEFAULT_COLOR_PARAMS };
            compositor.composite_layer(gl, deck_tex, &deck, true);
            assert_near(read_center(gl, &compositor), [255, 0, 0, 255], "the deck alone");

            compositor.composite_video_layer(gl, video_tex, 1.0, DEFAULT_COLOR_PARAMS);
            compositor.end_frame(gl);

            assert_near(read_center(gl, &compositor), [0, 0, 255, 255], "the video layer over the deck");

            unsafe {
                gl.delete_texture(deck_tex);
                gl.delete_texture(video_tex);
            }
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        /// The half of the layer that makes "reuse the Color shader path"
        /// a real claim rather than an intention: the exact `ColorParams`
        /// `core::video` produces on a beat drive the deck shader's own
        /// `uBrightnessMul`/`uHueRotateDeg`.
        #[test]
        fn the_beat_flash_brightens_the_layer_by_the_webs_factor() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");

            let tex = solid_texture(gl, [100, 100, 100]);
            let mut state = VideoState::default();
            state.react_flash = true;
            state.react_hue = false;

            begin_clean_frame(gl, &mut compositor);
            compositor.composite_video_layer(gl, tex, 1.0, state.layer_color_params(false));
            compositor.end_frame(gl);
            assert_near(read_center(gl, &compositor), [100, 100, 100, 255], "off the beat");

            begin_clean_frame(gl, &mut compositor);
            compositor.composite_video_layer(gl, tex, 1.0, state.layer_color_params(true));
            compositor.end_frame(gl);
            // 100/255 * 1.4 = 0.549 -> 140.
            assert_near(read_center(gl, &compositor), [140, 140, 140, 255], "on the beat");

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        #[test]
        fn the_beat_hue_rotation_actually_moves_the_hue() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");

            let tex = solid_texture(gl, [255, 0, 0]);
            let mut state = VideoState::default();
            state.react_flash = false;
            state.react_hue = true;

            begin_clean_frame(gl, &mut compositor);
            compositor.composite_video_layer(gl, tex, 1.0, state.layer_color_params(false));
            compositor.end_frame(gl);
            assert_near(read_center(gl, &compositor), [255, 0, 0, 255], "off the beat: untouched red");

            begin_clean_frame(gl, &mut compositor);
            compositor.composite_video_layer(gl, tex, 1.0, state.layer_color_params(true));
            compositor.end_frame(gl);
            // +35 degrees from pure red, at full saturation/value: red
            // stays pinned and green climbs to 35/60 of full.
            let px = read_center(gl, &compositor);
            assert_eq!(px[0], 255, "red channel should stay saturated: {px:?}");
            assert!((140..=160).contains(&px[1]), "green should be ~149 after +35deg, got {px:?}");
            assert_eq!(px[2], 0, "blue should stay at 0: {px:?}");

            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }

        /// GL-state hygiene, the same check the overlay pass carries: this
        /// pass must leave texture unit 0 active, since every other pass in
        /// the frame (and egui_glow, and the deck upload paths) assumes it.
        #[test]
        fn the_pass_leaves_texture_unit_zero_active_and_no_gl_error() {
            let h = headless_gl();
            let gl = &h.gl;
            let mut compositor = Compositor::new(gl).expect("Compositor::new should build");
            begin_clean_frame(gl, &mut compositor);

            let tex = solid_texture(gl, [10, 20, 30]);
            compositor.composite_video_layer(gl, tex, 1.0, ColorParams { hue_rotate: 0.25, ..DEFAULT_COLOR_PARAMS });
            compositor.end_frame(gl);

            let active = unsafe { gl.get_parameter_i32(glow::ACTIVE_TEXTURE) };
            assert_eq!(active as u32, glow::TEXTURE0, "composite_video_layer left texture unit {active:#x} active");
            unsafe { gl.delete_texture(tex) };
            assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
        }
    }
}
