//! Native port of OpenDrop-VJ `src/lib/engine/compositor.ts`: the deck-slot
//! blend/keying/color-correction shader (`#version 330 core` here vs
//! `#version 300 es` there). Two deltas from the source, both required by
//! the native pipeline (see the plan's step-1 review):
//!  - the vertex shader's `vUV.y = 1.0 - vUV.y` is dropped: it only existed
//!    to cancel a `<canvas>` upload's top-left origin, and projectM's
//!    FBO-0 → texture copy (`deck::copy_fbo0_to_shared_texture`) is already
//!    in GL's bottom-left convention: nothing in this pipeline flips rows.
//!  - 14 uniforms, not 13 (PLAN.md's count; corrected in step 10).
//!    The video layer (compositor.ts's 5th layer) is out of scope for Phase 2
//!   : no video decode crate exists yet (see PLAN.md § Hors).

use glow::HasContext;
use opendrop_core::blend::{blend_state_for, BlendMode, ColorParams, GlBlend, SlotComposite};

use crate::timing::PassTimer;

pub const COMP_W: u32 = 1920;
pub const COMP_H: u32 = 1080;

const VERTEX_SRC: &str = r#"#version 330 core
const vec2 verts[6] = vec2[6](vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(-1.0,1.0), vec2(-1.0,1.0), vec2(1.0,-1.0), vec2(1.0,1.0));
out vec2 vUV;
void main() {
	vec2 p = verts[gl_VertexID];
	gl_Position = vec4(p, 0.0, 1.0);
	vUV = p * 0.5 + 0.5;
}
"#;

const FRAGMENT_SRC: &str = r#"#version 330 core
uniform sampler2D uTex;
uniform bool uMultiply;
uniform float uOpacity;
uniform bool uLumaOn;
uniform float uLumaBlack;
uniform float uLumaWhite;
uniform bool uColorOn;
uniform float uKeyHue;
uniform float uKeyTol;
uniform float uHueRotateDeg;
uniform float uSaturateMul;
uniform float uBrightnessMul;
uniform float uContrastMul;
uniform float uInvertAmount;
in vec2 vUV;
out vec4 fragColor;

float luma(vec3 c) { return dot(c, vec3(0.299, 0.587, 0.114)); }

float rgb2hue(vec3 c) {
	float maxc = max(c.r, max(c.g, c.b));
	float minc = min(c.r, min(c.g, c.b));
	float delta = maxc - minc;
	if (delta < 1e-5) return 0.0;
	float h;
	if (maxc == c.r) h = mod((c.g - c.b) / delta, 6.0);
	else if (maxc == c.g) h = (c.b - c.r) / delta + 2.0;
	else h = (c.r - c.g) / delta + 4.0;
	return h / 6.0;
}

float hueDist(float a, float b) {
	float d = abs(a - b);
	return min(d, 1.0 - d);
}

vec3 rgb2hsv(vec3 c) {
	vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
	vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
	vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
	float d = q.x - min(q.w, q.y);
	float e = 1.0e-10;
	return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

vec3 hsv2rgb(vec3 c) {
	vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
	vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
	return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

vec3 applyColorParams(vec3 c) {
	vec3 hsv = rgb2hsv(c);
	hsv.x = fract(hsv.x + uHueRotateDeg / 360.0);
	c = hsv2rgb(hsv);
	c = mix(vec3(luma(c)), c, uSaturateMul);
	c = c * uBrightnessMul;
	c = (c - 0.5) * uContrastMul + 0.5;
	c = mix(c, 1.0 - c, uInvertAmount);
	return c;
}

void main() {
	vec3 raw = texture(uTex, vUV).rgb;

	float mask = 1.0;
	if (uLumaOn) {
		float l = luma(raw);
		mask *= smoothstep(uLumaBlack - 0.02, uLumaBlack + 0.02, l);
		mask *= 1.0 - smoothstep(uLumaWhite - 0.02, uLumaWhite + 0.02, l);
	}
	if (uColorOn) {
		float dh = hueDist(rgb2hue(raw), uKeyHue);
		mask *= smoothstep(uKeyTol, uKeyTol + 0.05, dh);
	}

	vec3 C = clamp(applyColorParams(raw), 0.0, 1.0);
	float A = clamp(uOpacity * mask, 0.0, 1.0);
	vec3 outRGB = uMultiply ? mix(vec3(1.0), C, A) : C * A;
	fragColor = vec4(outRGB, A);
}
"#;

struct Uniforms {
    u_tex: Option<glow::NativeUniformLocation>,
    u_multiply: Option<glow::NativeUniformLocation>,
    u_opacity: Option<glow::NativeUniformLocation>,
    u_luma_on: Option<glow::NativeUniformLocation>,
    u_luma_black: Option<glow::NativeUniformLocation>,
    u_luma_white: Option<glow::NativeUniformLocation>,
    u_color_on: Option<glow::NativeUniformLocation>,
    u_key_hue: Option<glow::NativeUniformLocation>,
    u_key_tol: Option<glow::NativeUniformLocation>,
    u_hue_rotate_deg: Option<glow::NativeUniformLocation>,
    u_saturate_mul: Option<glow::NativeUniformLocation>,
    u_brightness_mul: Option<glow::NativeUniformLocation>,
    u_contrast_mul: Option<glow::NativeUniformLocation>,
    u_invert_amount: Option<glow::NativeUniformLocation>,
}

/// One deck slot's compositing input for one frame: same `SlotComposite`/
/// `ColorParams` `core::blend` already models, plus the opacity that in the
/// real app comes from the crossfader (step 7).
#[derive(Clone, Copy)]
pub struct LayerInput {
    pub opacity: f32,
    pub composite: SlotComposite,
    pub color: ColorParams,
}

pub struct Compositor {
    pub fbo: glow::NativeFramebuffer,
    #[allow(dead_code)] // not sampled directly; render_frame reads the FBO's attachment
    pub color_tex: glow::NativeTexture,
    program: glow::NativeProgram,
    uniforms: Uniforms,
    /// GLSL 3.3 core requires a VAO bound at draw time even for a shader
    /// with no vertex attributes (this one builds its fullscreen triangle
    /// from `gl_VertexID` alone): ES/compatibility profiles don't need
    /// this, which is why the WebGL2 source never had one.
    empty_vao: glow::NativeVertexArray,
    composite_timer: PassTimer,
}

impl Compositor {
    /// Must run while the main context is current: the FBO/program/VAO
    /// created here are NOT shared across the GL share group (only textures
    /// and buffers are); they belong exclusively to whichever context is
    /// current at creation time.
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
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            let program = build_program(gl)?;
            let uniforms = locate_uniforms(gl, program);
            let empty_vao = gl.create_vertex_array().map_err(|e| format!("create_vertex_array failed: {e}"))?;

            // Left enabled for the lifetime of this context: every layer
            // draw sets its own blendFuncSeparate/blendEquation before
            // drawing, same as the WebGL2 source enabling it once in its
            // constructor and never touching GL_BLEND's enable bit again.
            gl.enable(glow::BLEND);

            let composite_timer = PassTimer::new(gl).map_err(|e| format!("composite_timer: {e}"))?;

            Ok(Self { fbo, color_tex, program, uniforms, empty_vao, composite_timer })
        }
    }

    /// Clears the composite FBO to transparent and starts the "composite"
    /// pass's timer. Call once per frame, before any `composite_layer`
    /// calls; pair with `end_frame` after the last one.
    pub fn begin_frame(&mut self, gl: &glow::Context) {
        self.composite_timer.begin(gl);
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, COMP_W as i32, COMP_H as i32);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    /// Ends the "composite" pass's timer: call once per frame, after the
    /// last `composite_layer` call.
    pub fn end_frame(&mut self, gl: &glow::Context) {
        self.composite_timer.end(gl);
    }

    pub fn composite_ms(&self) -> Option<f64> {
        self.composite_timer.last_ms()
    }

    /// Draws one deck's texture into the composite FBO with its blend mode,
    /// keying, and color correction. Skips slots at or below the 0.001
    /// opacity floor. `force_normal` overrides `composite.blend`: used for
    /// the lowest active slot, since multiply/screen/additive against a
    /// still-transparent framebuffer reads wrong (e.g. multiply → black).
    pub fn composite_layer(&self, gl: &glow::Context, deck_tex: glow::NativeTexture, input: &LayerInput, force_normal: bool) {
        if input.opacity <= 0.001 {
            return;
        }
        let mode = if force_normal { BlendMode::Normal } else { input.composite.blend };
        let bs = blend_state_for(mode);
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, COMP_W as i32, COMP_H as i32);
            gl.blend_func_separate(gl_factor(bs.src_rgb), gl_factor(bs.dst_rgb), gl_factor(bs.src_a), gl_factor(bs.dst_a));
            gl.blend_equation(glow::FUNC_ADD);

            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.empty_vao));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(deck_tex));

            gl.uniform_1_i32(self.uniforms.u_tex.as_ref(), 0);
            gl.uniform_1_i32(self.uniforms.u_multiply.as_ref(), (mode == BlendMode::Multiply) as i32);
            gl.uniform_1_f32(self.uniforms.u_opacity.as_ref(), input.opacity);
            gl.uniform_1_i32(self.uniforms.u_luma_on.as_ref(), input.composite.luma_key as i32);
            gl.uniform_1_f32(self.uniforms.u_luma_black.as_ref(), input.composite.luma_black as f32);
            gl.uniform_1_f32(self.uniforms.u_luma_white.as_ref(), input.composite.luma_white as f32);
            gl.uniform_1_i32(self.uniforms.u_color_on.as_ref(), input.composite.color_key as i32);
            gl.uniform_1_f32(self.uniforms.u_key_hue.as_ref(), input.composite.color_hue as f32);
            gl.uniform_1_f32(self.uniforms.u_key_tol.as_ref(), input.composite.color_tol as f32);
            // ColorParams fields are 0..1 with 0.5 = neutral (100%) for
            // sat/bright/contrast: same mapping color_params_to_filter uses.
            gl.uniform_1_f32(self.uniforms.u_hue_rotate_deg.as_ref(), (input.color.hue_rotate * 360.0) as f32);
            gl.uniform_1_f32(self.uniforms.u_saturate_mul.as_ref(), (input.color.saturate * 2.0) as f32);
            gl.uniform_1_f32(self.uniforms.u_brightness_mul.as_ref(), (input.color.brightness * 2.0) as f32);
            gl.uniform_1_f32(self.uniforms.u_contrast_mul.as_ref(), (input.color.contrast * 2.0) as f32);
            gl.uniform_1_f32(self.uniforms.u_invert_amount.as_ref(), input.color.invert as f32);

            gl.draw_arrays(glow::TRIANGLES, 0, 6);
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

fn gl_factor(f: GlBlend) -> u32 {
    match f {
        GlBlend::Zero => glow::ZERO,
        GlBlend::One => glow::ONE,
        GlBlend::SrcColor => glow::SRC_COLOR,
        GlBlend::OneMinusSrcColor => glow::ONE_MINUS_SRC_COLOR,
        GlBlend::SrcAlpha => glow::SRC_ALPHA,
        GlBlend::OneMinusSrcAlpha => glow::ONE_MINUS_SRC_ALPHA,
    }
}

fn compile_shader(gl: &glow::Context, kind: u32, src: &str) -> Result<glow::NativeShader, String> {
    unsafe {
        let shader = gl.create_shader(kind).map_err(|e| format!("create_shader failed: {e}"))?;
        gl.shader_source(shader, src);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let info = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(format!("shader compile failed: {info}"));
        }
        Ok(shader)
    }
}

fn build_program(gl: &glow::Context) -> Result<glow::NativeProgram, String> {
    unsafe {
        let vs = compile_shader(gl, glow::VERTEX_SHADER, VERTEX_SRC)?;
        let fs = compile_shader(gl, glow::FRAGMENT_SHADER, FRAGMENT_SRC)?;
        let program = gl.create_program().map_err(|e| format!("create_program failed: {e}"))?;
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        if !gl.get_program_link_status(program) {
            let info = gl.get_program_info_log(program);
            return Err(format!("program link failed: {info}"));
        }
        Ok(program)
    }
}

fn locate_uniforms(gl: &glow::Context, program: glow::NativeProgram) -> Uniforms {
    unsafe {
        Uniforms {
            u_tex: gl.get_uniform_location(program, "uTex"),
            u_multiply: gl.get_uniform_location(program, "uMultiply"),
            u_opacity: gl.get_uniform_location(program, "uOpacity"),
            u_luma_on: gl.get_uniform_location(program, "uLumaOn"),
            u_luma_black: gl.get_uniform_location(program, "uLumaBlack"),
            u_luma_white: gl.get_uniform_location(program, "uLumaWhite"),
            u_color_on: gl.get_uniform_location(program, "uColorOn"),
            u_key_hue: gl.get_uniform_location(program, "uKeyHue"),
            u_key_tol: gl.get_uniform_location(program, "uKeyTol"),
            u_hue_rotate_deg: gl.get_uniform_location(program, "uHueRotateDeg"),
            u_saturate_mul: gl.get_uniform_location(program, "uSaturateMul"),
            u_brightness_mul: gl.get_uniform_location(program, "uBrightnessMul"),
            u_contrast_mul: gl.get_uniform_location(program, "uContrastMul"),
            u_invert_amount: gl.get_uniform_location(program, "uInvertAmount"),
        }
    }
}
