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
use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::num::NonZeroU32;
use std::path::Path;

use crate::ffi;
use crate::gl_debug;
use crate::gl_state;
use crate::preset_patch;
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
    /// What this instance's `fps` held before `set_param` ever hijacked it.
    /// Captured at construction because `projectm_get_fps` stops being able to
    /// answer the question the moment the side channel is in use.
    default_fps: i32,
    /// Where projectM's preset-switch-failed callback deposits its message.
    /// `Box`ed so the address handed to C as `user_data` survives this
    /// `Deck` being moved into the deck `Vec`; a `RefCell` rather than an
    /// atomic because the callback is synchronous (measured) and fires on
    /// the same thread as the load that triggered it.
    ///
    /// Without this, a load that projectM *rejects* is invisible: the FFI
    /// call returns nothing, and `core.h` specifies that when a preset can't
    /// be loaded "no switch takes place": so the deck silently keeps
    /// rendering its previous preset while the app believes the new one is
    /// up. That failure mode matters more since every load goes through
    /// `load_preset_patched`, which can be rejected where the original file
    /// would not have been.
    load_failure: Box<RefCell<Option<String>>>,
    render_timer: PassTimer,
    copy_timer: PassTimer,
}

/// projectM's preset-switch-failed callback. `user_data` is the address of
/// the owning deck's `load_failure` cell.
///
/// # Safety
/// Called by libprojectM with a NUL-terminated `message` (or null), and the
/// `user_data` pointer registered in `create_one_deck_context`: which points
/// at a `RefCell<Option<String>>` owned by a live `Deck`, since the callback
/// is unregistered by `projectm_destroy` before that cell is dropped.
unsafe extern "C" fn on_preset_load_failed(_filename: *const c_char, message: *const c_char, user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }
    let cell = unsafe { &*(user_data as *const RefCell<Option<String>>) };
    let msg = if message.is_null() {
        "projectM rejected the preset (no message)".to_string()
    } else {
        unsafe { CStr::from_ptr(message) }.to_string_lossy().into_owned()
    };
    // `try_borrow_mut` rather than `borrow_mut`: panicking across an FFI
    // boundary is undefined behaviour, and dropping one message is a far
    // better outcome than that if projectM ever re-enters here.
    if let Ok(mut slot) = cell.try_borrow_mut() {
        *slot = Some(msg);
    }
}

impl Deck {
    /// Loads a preset straight from its file, unpatched.
    ///
    /// **This has no callers in the app any more**: since Step 8, every live
    /// load goes through [`Deck::load_preset_patched`], because the
    /// `set_param` side channel is sticky instance state that corrupts any
    /// unpatched preset loaded onto a deck that has ever been modulated (see
    /// that method). Kept as the unpatched entry point for a future caller
    /// that has first called [`Deck::reset_param_channel`]. Must be called
    /// while this deck's context is current.
    pub fn load_preset(&self, path: &Path, smooth_transition: bool) -> Result<(), String> {
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("preset path {} is not a valid C string: {e}", path.display()))?;
        self.clear_load_failure();
        unsafe { ffi::projectm_load_preset_file(self.handle, c_path.as_ptr(), smooth_transition) };
        self.take_load_failure()
    }

    /// Drops any message left over from an earlier load, so
    /// [`Deck::take_load_failure`] can only ever report this load's own.
    fn clear_load_failure(&self) {
        self.load_failure.borrow_mut().take();
    }

    /// `Err` if projectM's failure callback fired during the load just made.
    /// Sound because that callback is synchronous with the load call
    /// (measured against libprojectM 4.1.6: the message is already deposited
    /// by the time `projectm_load_preset_data` returns).
    fn take_load_failure(&self) -> Result<(), String> {
        match self.load_failure.borrow_mut().take() {
            Some(msg) => Err(format!("projectM rejected the preset: {msg}")),
            None => Ok(()),
        }
    }

    /// Loads preset text held in memory instead of from a path: the patched
    /// output of `preset_patch::patch_preset`. Same context rule as
    /// `load_preset`, this deck's context must be current.
    ///
    /// **This bypasses Phase 4's preflight validation.** `load_preset` is the
    /// single passage point for *files*, and preflight validates a file in an
    /// isolated child process before it can reach a live deck. Text arriving
    /// here has been through `patch_preset` since, so what actually gets
    /// compiled was never seen by preflight. Callers must still preflight the
    /// pre-patch file; the patch step itself is treated as trusted because it
    /// is this crate's own deterministic, unit-tested transform, not
    /// user-supplied content.
    pub fn load_preset_data(&self, text: &str, smooth_transition: bool) -> Result<(), String> {
        let c_text = CString::new(text)
            .map_err(|e| format!("patched preset text is not a valid C string: {e}"))?;
        self.clear_load_failure();
        unsafe { ffi::projectm_load_preset_data(self.handle, c_text.as_ptr(), smooth_transition) };
        self.take_load_failure()
    }

    /// Loads `path` with the host-to-preset side channel patched in: reads
    /// the file, runs it through `preset_patch::patch_preset` with `targets`,
    /// and hands the result to [`Deck::load_preset_data`]. The single
    /// passage point for loading a preset that [`Deck::set_param`] will
    /// modulate.
    ///
    /// **Why every load on a modulated deck must come through here** (spike
    /// report §5.2): the word written by `set_param` is projectM *instance*
    /// state that survives preset loads, so an unpatched preset loaded onto
    /// a deck that has ever been modulated reads a ~10^7 code word as its
    /// own `fps` and its framerate-dependent physics is destroyed, with
    /// nothing to restore it. Routing every load through this function makes
    /// that unrepresentable rather than merely documented.
    ///
    /// Preflight (Phase 4) still validates the file, unpatched, exactly as
    /// before: see [`Deck::load_preset_data`] on why the patch step itself
    /// is treated as trusted.
    ///
    /// Plenty of `.milk` files in the wild are CP1252 rather than UTF-8, so
    /// the bytes are read and lossily decoded rather than read as a `String`:
    /// a replacement character inside a preset comment is harmless, a hard
    /// error on a preset the user picked is not.
    pub fn load_preset_patched(
        &self,
        path: &Path,
        targets: &[preset_patch::PatchTarget],
        substituted_fps: i32,
        smooth_transition: bool,
    ) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("failed to read preset {}: {e}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let patched = preset_patch::patch_preset(&text, targets, substituted_fps);
        self.load_preset_data(&patched, smooth_transition)
    }

    /// Writes one `(index, value)` pair into this deck's running preset: the
    /// host to preset side channel Time and Qvar modulate through. Cheap
    /// enough to call every frame; see `engine::preset_patch` for why it goes
    /// through `projectm_set_fps` and what the preset must be patched with
    /// first.
    ///
    /// **Sticky.** The written word is instance state that outlives preset
    /// loads, so from the first call onwards every preset loaded on this deck
    /// must be a patched one (`load_preset_data`), or its own `fps` reads see
    /// the raw code word. Call [`Deck::reset_param_channel`] before going back
    /// to plain `load_preset`.
    pub fn set_param(&self, index: u16, value: f64) {
        unsafe { ffi::projectm_set_fps(self.handle, preset_patch::encode_param(index, value)) };
    }

    /// Puts `fps` back to the value this instance started with, undoing
    /// `set_param`'s hijack. Required before loading an unpatched preset on a
    /// deck that has been modulated, otherwise that preset's own
    /// framerate-dependent physics reads a ~10^7 code word.
    pub fn reset_param_channel(&self) {
        unsafe { ffi::projectm_set_fps(self.handle, self.default_fps) };
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
    // `Gles(None)` lets glutin/ANGLE negotiate down to GLES 3.0, which is
    // below projectM's GladLoader minimum (patched to 3.1 to match this
    // vendored ANGLE build's ceiling on its D3D11 backend, see
    // packaging/windows/overlay-ports/projectm/gles31-min-version.patch):
    // ask for exactly what's needed instead of leaving it to negotiation.
    #[cfg(target_os = "windows")]
    let context_api = ContextApi::Gles(Some(Version::new(3, 1)));
    #[cfg(not(target_os = "windows"))]
    let context_api = ContextApi::OpenGl(Some(Version::new(3, 3)));

    let deck_ctx_attrs = ContextAttributesBuilder::new()
        .with_debug(cfg!(debug_assertions))
        .with_profile(GlProfile::Core)
        .with_context_api(context_api)
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

    let default_fps = unsafe { ffi::projectm_get_fps(handle) };

    // Same callback `app::preflight` and `app::thumbnail_child` already
    // register in their own child processes: live decks went without it,
    // which made a rejected load indistinguishable from a successful one.
    // Registered against the `Box`'s heap address, which survives the `Deck`
    // being moved into the deck `Vec`.
    let load_failure: Box<RefCell<Option<String>>> = Box::new(RefCell::new(None));
    unsafe {
        ffi::projectm_set_preset_switch_failed_event_callback(
            handle,
            Some(on_preset_load_failed),
            std::ptr::from_ref::<RefCell<Option<String>>>(&load_failure).cast::<c_void>().cast_mut(),
        )
    };

    Ok(Deck {
        context,
        surface,
        gl,
        texture,
        width: w,
        height: h,
        handle,
        default_fps,
        load_failure,
        render_timer,
        copy_timer,
    })
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
