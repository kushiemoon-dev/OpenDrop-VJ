//! GL state hygiene around code we don't control.
//!
//! `reset_read_framebuffer_to_fbo0` is the fix from the plan's step-1
//! review: `projectm_opengl_render_frame()` (from step 5 on) leaves
//! `READ_FRAMEBUFFER_BINDING` pointed at one of projectM's own internal
//! FBOs and `READ_BUFFER` on `GL_COLOR_ATTACHMENT0`. `glCopyTexSubImage2D`
//! reads from the *read* framebuffer, so without resetting both to an
//! absolute known state first, the copy silently grabs projectM's
//! intermediate buffer instead of the frame actually rendered to this
//! context's own pbuffer (FBO 0): measured on 6 real presets in the Phase
//! 0 spike: up to 100% of pixels different, sometimes a different image
//! entirely.
//!
//! The full save/restore around `render_frame` itself (the wider field set
//! projectM can touch: program, VAO, blend state, viewport, scissor, …)
//! lands in step 5, once there's an actual `render_frame` call to wrap.

use glow::HasContext;

/// Absolute reset (not a restore-to-previous): call this immediately
/// before every `glCopyTexSubImage2D` that reads from a deck's own pbuffer.
///
/// The `glReadBuffer(GL_BACK)` also works around a Mesa quirk: on a
/// pbuffer, `GL_DOUBLEBUFFER` is 0 but `DRAW_BUFFER` is `BACK` and
/// `READ_BUFFER` is `FRONT`: front and back alias the same storage here,
/// but that won't hold on a driver where the pbuffer is genuinely
/// double-buffered.
pub fn reset_read_framebuffer_to_fbo0(gl: &glow::Context) {
    unsafe {
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        gl.read_buffer(glow::BACK);
    }
}
