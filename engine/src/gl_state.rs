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

/// The full GL state `render_frame` (step 5 on) saves and restores around
/// `projectm_opengl_render_frame`: a superset of what libprojectM 4.1.6
/// actually touches, since that varies per preset. Restoring in *absolute*
/// terms (back to exactly this snapshot), not incrementally.
pub struct GlState {
    program: Option<glow::NativeProgram>,
    vertex_array: Option<glow::NativeVertexArray>,
    array_buffer: Option<glow::NativeBuffer>,
    draw_framebuffer: Option<glow::NativeFramebuffer>,
    read_framebuffer: Option<glow::NativeFramebuffer>,
    read_buffer: i32,
    draw_buffer: i32,
    active_texture: i32,
    texture_binding_2d: Option<glow::NativeTexture>,
    blend_enabled: bool,
    blend_src_rgb: i32,
    blend_dst_rgb: i32,
    blend_src_alpha: i32,
    blend_dst_alpha: i32,
    blend_eq_rgb: i32,
    blend_eq_alpha: i32,
    viewport: [i32; 4],
    scissor_test_enabled: bool,
    scissor_box: [i32; 4],
    color_writemask: [bool; 4],
    unpack_alignment: i32,
}

pub fn save(gl: &glow::Context) -> GlState {
    unsafe {
        let mut viewport = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
        let mut scissor_box = [0i32; 4];
        gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor_box);
        GlState {
            program: gl.get_parameter_program(glow::CURRENT_PROGRAM),
            vertex_array: gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING),
            array_buffer: gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING),
            draw_framebuffer: gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING),
            read_framebuffer: gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING),
            read_buffer: gl.get_parameter_i32(glow::READ_BUFFER),
            draw_buffer: gl.get_parameter_i32(glow::DRAW_BUFFER),
            active_texture: gl.get_parameter_i32(glow::ACTIVE_TEXTURE),
            texture_binding_2d: gl.get_parameter_texture(glow::TEXTURE_BINDING_2D),
            blend_enabled: gl.is_enabled(glow::BLEND),
            blend_src_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
            blend_dst_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
            blend_src_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
            blend_dst_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
            blend_eq_rgb: gl.get_parameter_i32(glow::BLEND_EQUATION_RGB),
            blend_eq_alpha: gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA),
            viewport,
            scissor_test_enabled: gl.is_enabled(glow::SCISSOR_TEST),
            scissor_box,
            color_writemask: gl.get_parameter_bool_array::<4>(glow::COLOR_WRITEMASK),
            unpack_alignment: gl.get_parameter_i32(glow::UNPACK_ALIGNMENT),
        }
    }
}

pub fn restore(gl: &glow::Context, s: &GlState) {
    unsafe {
        gl.use_program(s.program);
        gl.bind_vertex_array(s.vertex_array);
        gl.bind_buffer(glow::ARRAY_BUFFER, s.array_buffer);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, s.draw_framebuffer);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, s.read_framebuffer);
        gl.read_buffer(s.read_buffer as u32);
        gl.draw_buffers(&[s.draw_buffer as u32]);
        gl.active_texture(s.active_texture as u32);
        gl.bind_texture(glow::TEXTURE_2D, s.texture_binding_2d);
        if s.blend_enabled {
            gl.enable(glow::BLEND);
        } else {
            gl.disable(glow::BLEND);
        }
        gl.blend_func_separate(s.blend_src_rgb as u32, s.blend_dst_rgb as u32, s.blend_src_alpha as u32, s.blend_dst_alpha as u32);
        gl.blend_equation_separate(s.blend_eq_rgb as u32, s.blend_eq_alpha as u32);
        gl.viewport(s.viewport[0], s.viewport[1], s.viewport[2], s.viewport[3]);
        if s.scissor_test_enabled {
            gl.enable(glow::SCISSOR_TEST);
        } else {
            gl.disable(glow::SCISSOR_TEST);
        }
        gl.scissor(s.scissor_box[0], s.scissor_box[1], s.scissor_box[2], s.scissor_box[3]);
        gl.color_mask(s.color_writemask[0], s.color_writemask[1], s.color_writemask[2], s.color_writemask[3]);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, s.unpack_alignment);
    }
}
