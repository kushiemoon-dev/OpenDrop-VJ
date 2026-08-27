use glow::HasContext;
use glutin::config::{Api, ConfigSurfaceTypes, ConfigTemplateBuilder};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use opendrop_engine::deck::{self, DeckStack};
use raw_window_handle::HasWindowHandle;
use std::num::NonZeroU32;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

#[allow(dead_code)] // both fields kept alive for their Drop; used fully from step 3 onward
struct AppState {
    window: Window,
    deck_stack: DeckStack,
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
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
        }
    }
}

/// Step 2 of Phase 2: proves the 5-context share group (1 main + 4 deck) and
/// pbuffer setup works on this driver before any real rendering is built on
/// top of it. See `piped-rolling-sunrise.md` step 2.
fn bootstrap(event_loop: &ActiveEventLoop) -> Result<AppState, String> {
    let window_attrs = Window::default_attributes()
        .with_title("OpenDrop: control")
        .with_transparent(false);

    // WINDOW | PBUFFER: this config backs both the window surface below and
    // the 4 deck pbuffer surfaces engine::deck creates from it. with_api
    // must be explicit: on EGL, ConfigTemplateBuilder defaults to
    // requesting GLES2, not desktop OpenGL.
    let template = ConfigTemplateBuilder::new()
        .with_api(Api::OPENGL)
        .with_surface_type(ConfigSurfaceTypes::WINDOW | ConfigSurfaceTypes::PBUFFER)
        .with_alpha_size(8)
        .with_depth_size(0)
        .with_stencil_size(0);

    let (window, gl_config) = DisplayBuilder::new()
        .with_window_attributes(Some(window_attrs))
        .build(event_loop, template, |mut configs| {
            // DisplayBuilder's picker callback must return a Config, not a
            // Result: an empty match here means the template's constraints
            // (see above) can't be satisfied on this driver at all.
            configs.next().expect("EGL returned zero configs matching the WINDOW|PBUFFER/OpenGL/alpha8/depth0/stencil0 template")
        })
        .map_err(|e| format!("failed to bootstrap EGL display/config: {e}"))?;
    let window = window.ok_or_else(|| "DisplayBuilder did not create the requested window".to_string())?;
    let display = gl_config.display();

    let raw_window_handle = window
        .window_handle()
        .map_err(|e| format!("window has no raw handle: {e}"))?
        .as_raw();

    let ctx_attrs = ContextAttributesBuilder::new()
        .with_debug(cfg!(debug_assertions))
        .with_profile(GlProfile::Core)
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(raw_window_handle));

    // Anchor context: created here, converted to PossiblyCurrent, but not
    // actually made current yet. engine::deck::create_deck_stack creates its
    // 4 contexts sharing this anchor's namespace before any of the 5 total
    // contexts is made current: see gl_state correctness notes in the plan.
    let not_current_main = unsafe { display.create_context(&gl_config, &ctx_attrs) }
        .map_err(|e| format!("failed to create main GL context: {e}"))?;
    let main_ctx = not_current_main.treat_as_possibly_current();

    let deck_stack = deck::create_deck_stack(&display, &gl_config, &main_ctx)?;

    let size = window.inner_size();
    let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(size.width.max(1)).expect("width.max(1) is nonzero"),
        NonZeroU32::new(size.height.max(1)).expect("height.max(1) is nonzero"),
    );
    let main_surface = unsafe { display.create_window_surface(&gl_config, &surface_attrs) }
        .map_err(|e| format!("failed to create main window surface: {e}"))?;
    main_ctx
        .make_current(&main_surface)
        .map_err(|e| format!("failed to make main context current: {e}"))?;

    let mut gl = unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };
    if cfg!(debug_assertions) {
        opendrop_engine::gl_debug::install(&mut gl, "main");
    }
    let version = unsafe { gl.get_parameter_string(glow::VERSION) };
    println!("[app] main context: GL {version}");

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
    let visible_from_deck3 = unsafe { deck_stack.gl[3].is_texture(probe_tex) };
    println!("[app] texture created in main context, visible from deck context 4: {visible_from_deck3}");
    if !visible_from_deck3 {
        return Err("share group broken: texture created in the main context is not visible from deck context 4".to_string());
    }

    Ok(AppState { window, deck_stack })
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create winit event loop");
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("event loop exited with an error");
}
