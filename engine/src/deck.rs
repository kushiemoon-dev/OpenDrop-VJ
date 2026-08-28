//! The 4 deck GL contexts. Each shares the object namespace of the app's
//! main window context (so textures created here are visible from it, and
//! vice versa), owns one projectM instance, and renders off-screen into its
//! own pbuffer.
//!
//! Deck resolution is capped hard at 4096: `EGL_MAX_PBUFFER_WIDTH/HEIGHT`.
//! `with_pbuffer_sizes()` on the config template is ignored by EGL, so that
//! cap isn't requestable; it's just a fact to respect when picking sizes.

use glow::HasContext;
use glutin::config::Config;
use glutin::context::{
    ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext,
    PossiblyCurrentGlContext, Version,
};
use glutin::display::{Display, GlDisplay};
use glutin::surface::{PbufferSurface, Surface, SurfaceAttributesBuilder};
use std::ffi::CString;
use std::num::NonZeroU32;
use std::path::Path;

use crate::ffi;
use crate::gl_debug;
use crate::gl_state;
use crate::timing::PassTimer;

pub const DECK_W: u32 = 1280;
pub const DECK_H: u32 = 720;
pub const DECK_COUNT: usize = 4;

/// One deck: its own GL context (sharing the main context's object
/// namespace), pbuffer surface, `glow::Context`, shared output texture, and
/// projectM instance: all created together, all belonging to this one
/// context.
pub struct Deck {
    pub context: PossiblyCurrentContext,
    pub surface: Surface<PbufferSurface>,
    pub gl: glow::Context,
    pub texture: glow::NativeTexture,
    /// This deck's own render size: `DECK_W`/`DECK_H` for the 4 live
    /// decks, `THUMB_W`/`THUMB_H` for the thumbnail context. Kept so
    /// `render_frame`'s copy sizes itself off the real surface instead of
    /// the module-level live-deck constants.
    width: u32,
    height: u32,
    handle: ffi::projectm_handle,
    render_timer: PassTimer,
    copy_timer: PassTimer,
}

impl Deck {
    /// The single passage point for loading a preset into this deck.
    /// Phase 4's per-preset pre-flight validation (a subprocess that loads
    /// the file first, so a bad preset can't take down a running deck)
    /// hooks in here without touching how contexts are structured. Must be
    /// called while this deck's context is current.
    pub fn load_preset(&self, path: &Path, smooth_transition: bool) -> Result<(), String> {
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("preset path {} is not a valid C string: {e}", path.display()))?;
        unsafe { ffi::projectm_load_preset_file(self.handle, c_path.as_ptr(), smooth_transition) };
        Ok(())
    }

    /// Sets the soft-cut transition duration in seconds.
    pub fn set_soft_cut_duration(&self, seconds: f64) {
        unsafe { ffi::projectm_set_soft_cut_duration(self.handle, seconds) };
    }

    /// Sets the mesh size (width and height) for the visualization.
    pub fn set_mesh_size(&self, width: usize, height: usize) {
        unsafe { ffi::projectm_set_mesh_size(self.handle, width, height) };
    }

    /// Injects one chunk of PCM, renders one projectM frame: GL state
    /// saved/restored in absolute terms around the opaque render call (see
    /// `gl_state`, and the plan's step-1 review: without this, the
    /// subsequent copy can read garbage left behind by whatever the preset
    /// did to blend/framebuffer/viewport state): then copies the result
    /// into this deck's shared texture. Must be called while this deck's
    /// context is current.
    ///
    /// Render and copy are timed as two sequential `GL_TIME_ELAPSED`
    /// queries (never nested: see `timing::PassTimer`), each read back
    /// non-blockingly and inline, so timing this costs nothing beyond the
    /// query calls themselves: no extra context switch.
    pub fn render_frame(&mut self, pcm: &[f32]) {
        self.render_timer.begin(&self.gl);
        unsafe {
            ffi::projectm_pcm_add_float(
                self.handle,
                pcm.as_ptr(),
                (pcm.len() / 2) as u32,
                ffi::projectm_channels_PROJECTM_STEREO,
            );
            let before = gl_state::save(&self.gl);
            ffi::projectm_opengl_render_frame(self.handle);
            gl_state::restore(&self.gl, &before);
        }
        self.render_timer.end(&self.gl);

        self.copy_timer.begin(&self.gl);
        copy_fbo0_to_shared_texture(&self.gl, self.texture, self.width as i32, self.height as i32);
        self.copy_timer.end(&self.gl);
    }

    pub fn render_ms(&self) -> Option<f64> {
        self.render_timer.last_ms()
    }

    pub fn copy_ms(&self) -> Option<f64> {
        self.copy_timer.last_ms()
    }
}

impl Drop for Deck {
    fn drop(&mut self) {
        // projectm_destroy needs this deck's context current to free its GL
        // resources. Fine today: the whole Vec<Deck> (contexts included)
        // is torn down together at process exit; revisit if a Deck is ever
        // dropped individually while its siblings keep running.
        unsafe { ffi::projectm_destroy(self.handle) };
    }
}

pub fn create_decks(display: &Display, config: &Config, anchor: &PossiblyCurrentContext) -> Result<Vec<Deck>, String> {
    let mut decks = Vec::with_capacity(DECK_COUNT);
    for i in 0..DECK_COUNT {
        let attrs = SurfaceAttributesBuilder::<PbufferSurface>::new().build(
            NonZeroU32::new(DECK_W).expect("DECK_W is nonzero"),
            NonZeroU32::new(DECK_H).expect("DECK_H is nonzero"),
        );
        let surface = unsafe { display.create_pbuffer_surface(config, &attrs) }
            .map_err(|e| format!("failed to create deck {i} pbuffer surface: {e}"))?;

        let label: &'static str = match i {
            0 => "deck0",
            1 => "deck1",
            2 => "deck2",
            _ => "deck3",
        };
        decks.push(create_one_deck_context(display, config, anchor, surface, DECK_W, DECK_H, label)?);
    }
    Ok(decks)
}

/// Builds one GL context sharing `anchor`'s object namespace, made current
/// against the given (already-created) pbuffer `surface`, with its own
/// `glow::Context` and projectM instance: the extracted per-deck body of
/// `create_decks`.
///
/// `w`/`h` are parameters rather than `DECK_W`/`DECK_H` because a second
/// caller used to build a smaller, 6th context here for preset thumbnails.
/// That renderer now runs in a separate process (`app::thumbnail_child`),
/// so `create_decks` is the only caller left; the sizes stay explicit
/// because everything downstream of them: the shared texture's
/// allocation, `copy_fbo0_to_shared_texture`'s copy region: has to agree
/// with the surface actually created, not with a module constant.
///
/// Note: this used to be split across 3 loops in `create_decks` so that all
/// 4 deck contexts were created before any of them (or `anchor`) was made
/// current. That ordering isn't an EGL requirement: a share-group context
/// can be created at any time regardless of which sibling is current.
pub fn create_one_deck_context(
    display: &Display,
    config: &Config,
    anchor: &PossiblyCurrentContext,
    surface: Surface<PbufferSurface>,
    w: u32,
    h: u32,
    debug_label: &'static str,
) -> Result<Deck, String> {
    let deck_ctx_attrs = ContextAttributesBuilder::new()
        .with_debug(cfg!(debug_assertions))
        .with_profile(GlProfile::Core)
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .with_sharing(anchor)
        .build(None);
    let not_current = unsafe { display.create_context(config, &deck_ctx_attrs) }
        .map_err(|e| format!("failed to create {debug_label} GL context: {e}"))?;
    let context = not_current.treat_as_possibly_current();
    context
        .make_current(&surface)
        .map_err(|e| format!("failed to make {debug_label} context current: {e}"))?;

    let mut gl = unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };
    if cfg!(debug_assertions) {
        gl_debug::install(&mut gl, debug_label);
    }

    let version = unsafe { gl.get_parameter_string(glow::VERSION) };
    println!("[engine] {debug_label} context: GL {version}");

    let texture = create_shared_deck_texture(&gl, w, h);

    // projectm_create() allocates GL resources immediately, so it needs
    // this deck's context current (it is, from make_current above):
    // those resources end up private to this context, same as the FBO.
    let handle = unsafe { ffi::projectm_create() };
    if handle.is_null() {
        return Err(format!("projectm_create() returned NULL in {debug_label} context"));
    }
    unsafe { ffi::projectm_set_window_size(handle, w as usize, h as usize) };

    let render_timer = PassTimer::new(&gl).map_err(|e| format!("{debug_label} render_timer: {e}"))?;
    let copy_timer = PassTimer::new(&gl).map_err(|e| format!("{debug_label} copy_timer: {e}"))?;

    Ok(Deck { context, surface, gl, texture, width: w, height: h, handle, render_timer, copy_timer })
}

/// Copies a `w` x `h` region of this context's own pbuffer (FBO 0) into its
/// shared deck texture. The exclusive `glCopyTexSubImage2D` in this whole
/// pipeline: GPU-to-GPU, no `glReadPixels` anywhere near the render path.
///
/// `w`/`h` are the caller's real surface size, not `DECK_W`/`DECK_H`: see
/// `create_one_deck_context` on why that distinction is kept even now that
/// every live caller passes the deck constants.
pub fn copy_fbo0_to_shared_texture(gl: &glow::Context, tex: glow::NativeTexture, w: i32, h: i32) {
    gl_state::reset_read_framebuffer_to_fbo0(gl);
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, w, h);
    }
}

/// Allocates the context's shared output texture at its own `w` x `h`: see
/// `copy_fbo0_to_shared_texture` on why this is not `DECK_W`/`DECK_H`.
fn create_shared_deck_texture(gl: &glow::Context, w: u32, h: u32) -> glow::NativeTexture {
    unsafe {
        let tex = gl.create_texture().expect("glGenTextures failed");
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w as i32,
            h as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        tex
    }
}
