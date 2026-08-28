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

/// samples/channel per injected PCM chunk, ~10ms @48kHz: matches the
/// spike's own chunking, kept only because `synth_audio_chunk` is temporary.
pub const AUDIO_CHUNK: usize = 480;
const SAMPLE_RATE: u32 = 48_000;

/// One deck: its own GL context (sharing the main context's object
/// namespace), pbuffer surface, `glow::Context`, shared output texture, and
/// projectM instance: all created together, all belonging to this one
/// context.
pub struct Deck {
    pub context: PossiblyCurrentContext,
    pub surface: Surface<PbufferSurface>,
    pub gl: glow::Context,
    pub texture: glow::NativeTexture,
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
    pub fn load_preset(&self, path: &Path) -> Result<(), String> {
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("preset path {} is not a valid C string: {e}", path.display()))?;
        unsafe { ffi::projectm_load_preset_file(self.handle, c_path.as_ptr(), false) };
        Ok(())
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
        copy_fbo0_to_shared_texture(&self.gl, self.texture);
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
    let mut contexts = not_current.into_iter().map(|c| c.treat_as_possibly_current());

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
    let mut surfaces = surfaces.into_iter();

    let mut decks = Vec::with_capacity(DECK_COUNT);
    for i in 0..DECK_COUNT {
        let context = contexts.next().expect("exactly DECK_COUNT contexts were created above");
        let surface = surfaces.next().expect("exactly DECK_COUNT surfaces were created above");
        context.make_current(&surface).map_err(|e| format!("failed to make deck {i} context current: {e}"))?;

        let mut gl = unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };
        if cfg!(debug_assertions) {
            let label: &'static str = match i {
                0 => "deck0",
                1 => "deck1",
                2 => "deck2",
                _ => "deck3",
            };
            gl_debug::install(&mut gl, label);
        }

        let version = unsafe { gl.get_parameter_string(glow::VERSION) };
        println!("[engine] deck context {i}: GL {version}");

        let texture = create_shared_deck_texture(&gl);

        // projectm_create() allocates GL resources immediately, so it needs
        // this deck's context current (it is, from make_current above):
        // those resources end up private to this context, same as the FBO.
        let handle = unsafe { ffi::projectm_create() };
        if handle.is_null() {
            return Err(format!("projectm_create() returned NULL in deck {i} context"));
        }
        unsafe { ffi::projectm_set_window_size(handle, DECK_W as usize, DECK_H as usize) };

        let render_timer = PassTimer::new(&gl).map_err(|e| format!("deck {i} render_timer: {e}"))?;
        let copy_timer = PassTimer::new(&gl).map_err(|e| format!("deck {i} copy_timer: {e}"))?;

        decks.push(Deck { context, surface, gl, texture, handle, render_timer, copy_timer });
    }

    Ok(decks)
}

/// TEMPORARY (Phase 2 only: real audio capture lands in Phase 3): synthesizes
/// one PCM chunk so presets have something to react to. Ported from the
/// earlier prototype's `synth_audio_chunk`, one call per deck per frame, sharing
/// `sample_pos` across decks so their tones/kicks stay in phase.
pub fn synth_audio_chunk(sample_pos: u64, deck_index: usize) -> Vec<f32> {
    let mut buf = Vec::with_capacity(AUDIO_CHUNK * 2);
    let base_freq = 220.0 + deck_index as f32 * 55.0;
    for n in 0..AUDIO_CHUNK {
        let t = (sample_pos + n as u64) as f32 / SAMPLE_RATE as f32;
        let tone = (t * base_freq * std::f32::consts::TAU).sin() * 0.3;
        let beat_phase = (t * 2.0) % 1.0; // ~2 Hz synthetic kick
        let kick = if beat_phase < 0.02 { (1.0 - beat_phase / 0.02) * 0.6 } else { 0.0 };
        let s = (tone + kick).clamp(-1.0, 1.0);
        buf.push(s);
        buf.push(s);
    }
    buf
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
