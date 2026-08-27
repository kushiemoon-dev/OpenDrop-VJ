//! The 4 deck GL contexts. Each shares the object namespace of the app's
//! main window context (so textures created here are visible from it, and
//! vice versa) and renders off-screen into its own pbuffer.
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
use std::num::NonZeroU32;

use crate::gl_debug;
use crate::gl_state;

pub const DECK_W: u32 = 1280;
pub const DECK_H: u32 = 720;
pub const DECK_COUNT: usize = 4;

/// The 4 deck contexts, their pbuffer surfaces, one `glow::Context` per
/// context, and one shared RGBA8 texture per deck already allocated in each
/// context's namespace: the copy target for `glCopyTexSubImage2D` from
/// Phase 2 step 4 onward.
pub struct DeckStack {
    pub contexts: Vec<PossiblyCurrentContext>,
    pub surfaces: Vec<Surface<PbufferSurface>>,
    pub gl: Vec<glow::Context>,
    pub textures: Vec<glow::NativeTexture>,
}

pub fn create_deck_stack(
    display: &Display,
    config: &Config,
    anchor: &PossiblyCurrentContext,
) -> Result<DeckStack, String> {
    let deck_ctx_attrs = ContextAttributesBuilder::new()
        .with_debug(cfg!(debug_assertions))
        .with_profile(GlProfile::Core)
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .with_sharing(anchor)
        .build(None);

    // All 4 created: none made current yet: before any of the 5 total
    // contexts (this anchor included) is ever made current.
    let mut not_current = Vec::with_capacity(DECK_COUNT);
    for i in 0..DECK_COUNT {
        let ctx = unsafe { display.create_context(config, &deck_ctx_attrs) }
            .map_err(|e| format!("failed to create deck {i} GL context: {e}"))?;
        not_current.push(ctx);
    }
    let contexts: Vec<PossiblyCurrentContext> =
        not_current.into_iter().map(|c| c.treat_as_possibly_current()).collect();

    let mut surfaces = Vec::with_capacity(DECK_COUNT);
    for i in 0..DECK_COUNT {
        let attrs = SurfaceAttributesBuilder::<PbufferSurface>::new().build(
            NonZeroU32::new(DECK_W).expect("DECK_W is nonzero"),
            NonZeroU32::new(DECK_H).expect("DECK_H is nonzero"),
        );
        let surface = unsafe { display.create_pbuffer_surface(config, &attrs) }
            .map_err(|e| format!("failed to create deck {i} pbuffer surface: {e}"))?;
        surfaces.push(surface);
    }

    let mut gl = Vec::with_capacity(DECK_COUNT);
    let mut textures = Vec::with_capacity(DECK_COUNT);
    for i in 0..DECK_COUNT {
        contexts[i]
            .make_current(&surfaces[i])
            .map_err(|e| format!("failed to make deck {i} context current: {e}"))?;

        let mut ctx_gl = unsafe {
            glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s))
        };
        if cfg!(debug_assertions) {
            let label: &'static str = match i {
                0 => "deck0",
                1 => "deck1",
                2 => "deck2",
                _ => "deck3",
            };
            gl_debug::install(&mut ctx_gl, label);
        }

        let version = unsafe { ctx_gl.get_parameter_string(glow::VERSION) };
        println!("[engine] deck context {i}: GL {version}");

        let tex = create_shared_deck_texture(&ctx_gl);
        textures.push(tex);
        gl.push(ctx_gl);
    }

    Ok(DeckStack { contexts, surfaces, gl, textures })
}

/// Step 4 diagnostic test pattern, no projectM involved yet: a solid `color`
/// fill with a `band_color` strip along the bottom of the frame, then
/// copied into the deck's shared texture. The bottom-band placement is the
/// end-to-end orientation test: far more reliable than a near-symmetric
/// MilkDrop preset would be, and it stays "bottom" all the way through
/// (texture, composite, window) specifically because nothing along that
/// path flips rows. Assumes deck context `i`'s pbuffer is already current.
pub fn render_test_pattern(gl: &glow::Context, tex: glow::NativeTexture, color: (f32, f32, f32), band_color: (f32, f32, f32)) {
    unsafe {
        gl.disable(glow::SCISSOR_TEST);
        gl.clear_color(color.0, color.1, color.2, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);

        let band_h = (DECK_H / 8).max(1) as i32; // GL row 0 is the bottom of the frame
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(0, 0, DECK_W as i32, band_h);
        gl.clear_color(band_color.0, band_color.1, band_color.2, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.disable(glow::SCISSOR_TEST);
    }
    copy_fbo0_to_shared_texture(gl, tex);
}

/// Copies this context's own pbuffer (FBO 0) into its shared deck texture.
/// The exclusive `glCopyTexSubImage2D` in this whole pipeline: GPU-to-GPU,
/// no `glReadPixels` anywhere near the render path.
pub fn copy_fbo0_to_shared_texture(gl: &glow::Context, tex: glow::NativeTexture) {
    gl_state::reset_read_framebuffer_to_fbo0(gl);
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, DECK_W as i32, DECK_H as i32);
    }
}

fn create_shared_deck_texture(gl: &glow::Context) -> glow::NativeTexture {
    unsafe {
        let tex = gl.create_texture().expect("glGenTextures failed");
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            DECK_W as i32,
            DECK_H as i32,
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
