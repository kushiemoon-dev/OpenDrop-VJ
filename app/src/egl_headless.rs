//! Minimal headless EGL bootstrap for this binary's child-process
//! subcommands: an EGL display, a core-profile OpenGL 3.3 context, and a
//! pbuffer to render into, with no window and no winit/glutin involvement.
//!
//! Two callers, both of them child processes re-invoked from the parent
//! app: `preflight::run_preflight_check` (`--preflight-check`) and
//! `thumbnail_child::run_render_thumbnail` (`--render-thumbnail`). The main
//! process never uses any of this; it goes through glutin, which needs the
//! window handles this module deliberately does without.
//!
//! Ported from an earlier prototype (a pattern already proven there),
//! with `expect` rather than `Result` throughout on purpose: the only
//! callers are child processes whose entire contract with the parent is
//! their exit code, so a panic here is already the right failure signal.

use khronos_egl as egl;

pub type Egl = egl::DynamicInstance<egl::Latest>;

pub fn init_egl() -> (Egl, egl::Display, egl::Config) {
    let inst = unsafe { egl::DynamicInstance::<egl::Latest>::load_required() }.expect("failed to load libEGL.so.1");
    let display = unsafe { inst.get_display(egl::DEFAULT_DISPLAY) }.expect("eglGetDisplay failed");
    inst.initialize(display).expect("eglInitialize failed");
    inst.bind_api(egl::OPENGL_API).expect("eglBindAPI(OPENGL_API) failed");

    let config_attribs = [
        egl::SURFACE_TYPE,
        egl::PBUFFER_BIT,
        egl::RENDERABLE_TYPE,
        egl::OPENGL_BIT,
        egl::RED_SIZE,
        8,
        egl::GREEN_SIZE,
        8,
        egl::BLUE_SIZE,
        8,
        egl::ALPHA_SIZE,
        8,
        egl::NONE,
    ];
    let config = inst
        .choose_first_config(display, &config_attribs)
        .expect("eglChooseConfig failed")
        .expect("no matching EGL config for pbuffer+OpenGL");
    (inst, display, config)
}

// No `share` context param here (unlike the spike's general-purpose
// version): neither child process ever has more than this one context.
pub fn create_context(inst: &Egl, display: egl::Display, config: egl::Config) -> egl::Context {
    let attribs = [
        egl::CONTEXT_MAJOR_VERSION,
        3,
        egl::CONTEXT_MINOR_VERSION,
        3,
        egl::CONTEXT_OPENGL_PROFILE_MASK,
        egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
        egl::NONE,
    ];
    inst.create_context(display, config, None, &attribs).expect("eglCreateContext failed")
}

pub fn create_pbuffer(inst: &Egl, display: egl::Display, config: egl::Config, w: i32, h: i32) -> egl::Surface {
    let attribs = [egl::WIDTH, w, egl::HEIGHT, h, egl::NONE];
    inst.create_pbuffer_surface(display, config, &attribs).expect("eglCreatePbufferSurface failed")
}
