//! Step 4's compositor: pure `glBlitFramebuffer` plumbing, no shader: just
//! proves deck texture → composite FBO → window works end to end before
//! step 6 replaces the quadrant blits below with the real 14-uniform
//! shader (opacity, blend modes, keying, color correction).

use glow::HasContext;

use crate::deck;

pub const COMP_W: u32 = 1920;
pub const COMP_H: u32 = 1080;

pub struct Compositor {
    pub fbo: glow::NativeFramebuffer,
    #[allow(dead_code)] // not sampled directly until step 6's shader needs it
    pub color_tex: glow::NativeTexture,
    /// Scratch FBO used only to bind a deck texture as a blit *source*:
    /// `glBlitFramebuffer` needs an FBO-bound source, textures alone won't
    /// do. Re-attached to a different deck texture on every quadrant blit.
    read_scratch_fbo: glow::NativeFramebuffer,
}

impl Compositor {
    /// Must run while the main context is current. Unlike the deck
    /// textures, the FBO objects created here are NOT shared across the GL
    /// share group (only textures and buffers are): they belong
    /// exclusively to whichever context is current at creation time, same
    /// as the spike's compositor VAO.
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let color_tex = gl.create_texture().map_err(|e| format!("create_texture (composite) failed: {e}"))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(color_tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                COMP_W as i32,
                COMP_H as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);

            let fbo = gl.create_framebuffer().map_err(|e| format!("create_framebuffer (composite) failed: {e}"))?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color_tex), 0);
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(format!("composite FBO incomplete: status 0x{status:x}"));
            }

            let read_scratch_fbo = gl.create_framebuffer().map_err(|e| format!("create_framebuffer (scratch) failed: {e}"))?;

            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Ok(Self { fbo, color_tex, read_scratch_fbo })
        }
    }

    /// Blits a full deck texture into quadrant `index` (0=top-left,
    /// 1=top-right, 2=bottom-left, 3=bottom-right) of the composite FBO.
    pub fn blit_deck_into_quadrant(&self, gl: &glow::Context, deck_tex: glow::NativeTexture, index: usize) {
        let (qw, qh) = (COMP_W / 2, COMP_H / 2);
        let col = (index % 2) as u32;
        let row = (index / 2) as u32; // 0 = visually top, 1 = visually bottom
        let dst_x0 = col * qw;
        let dst_y0 = (1 - row) * qh; // GL's y origin is the bottom, so "top" is the upper half
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.read_scratch_fbo));
            gl.framebuffer_texture_2d(glow::READ_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(deck_tex), 0);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.fbo));
            gl.blit_framebuffer(
                0,
                0,
                deck::DECK_W as i32,
                deck::DECK_H as i32,
                dst_x0 as i32,
                dst_y0 as i32,
                (dst_x0 + qw) as i32,
                (dst_y0 + qh) as i32,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
        }
    }

    /// Blits the composite into whichever window surface's default
    /// framebuffer (FBO 0) is currently bound. Call once per window,
    /// between `make_current` and `swap_buffers`.
    pub fn blit_to_current_window(&self, gl: &glow::Context, window_w: i32, window_h: i32) {
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.blit_framebuffer(0, 0, COMP_W as i32, COMP_H as i32, 0, 0, window_w, window_h, glow::COLOR_BUFFER_BIT, glow::LINEAR);
        }
    }
}
