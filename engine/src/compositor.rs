//! Native port of OpenDrop-VJ `src/lib/engine/compositor.ts`: the deck-slot
//! blend/keying/color-correction shader (`#version 330 core` here vs
//! `#version 300 es` there). Two deltas from the source, both required by
//! the native pipeline:
//!  - the vertex shader's `vUV.y = 1.0 - vUV.y` is dropped: it only existed
//!    to cancel a `<canvas>` upload's top-left origin, and projectM's
//!    FBO-0 → texture copy (`deck::copy_fbo0_to_shared_texture`) is already
//!    in GL's bottom-left convention. Nothing in this pipeline flips rows.
//!  - 14 uniforms, not 13 as first counted.
//!
//! compositor.ts's 5th layer, the video background, was added after
//! the deck layers as [`Compositor::composite_video_layer`],
//! which reuses the deck shader above rather than adding a program of its
//! own, exactly as the TS source does. Its frames come from a CPU-side
//! decoder, so they *would* need the dropped row flip; that flip happens
//! once in ffmpeg's own filter chain instead (`opendrop_io::video_capture
//! ::output_args`), which is why nothing in this file has to know about it.

use glow::HasContext;
use opendrop_core::blend::{blend_state_for, BlendMode, ColorParams, GlBlend, SlotComposite, DEFAULT_SLOT_COMPOSITE};

use crate::timing::PassTimer;

pub const COMP_W: u32 = 1920;
pub const COMP_H: u32 = 1080;

const VERTEX_SRC: &str = r#"#version 300 es
const vec2 verts[6] = vec2[6](vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(-1.0,1.0), vec2(-1.0,1.0), vec2(1.0,-1.0), vec2(1.0,1.0));
out vec2 vUV;
void main() {
	vec2 p = verts[gl_VertexID];
	gl_Position = vec4(p, 0.0, 1.0);
	vUV = p * 0.5 + 0.5;
}
"#;

const FRAGMENT_SRC: &str = r#"#version 300 es
precision highp float;
precision highp sampler2D;
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
/// real app comes from the crossfader.
///
/// Deck slots only: an overlay sprite is a different primitive with
/// different semantics (arbitrary position/rotation/size, its own blend
/// vocabulary) and has its own [`OverlayLayerInput`] rather than extra
/// `Option` fields here.
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
    /// Strobe flash pass: its own tiny program (solid color * alpha, no
    /// texture sampling) rather than reusing `program`/`uniforms` above,
    /// which is shaped around sampling a deck texture. Shares `empty_vao`:
    /// same "fullscreen triangle from
    /// `gl_VertexID`, no vertex attributes" trick, no per-draw geometry to
    /// own.
    strobe_program: glow::NativeProgram,
    strobe_uniforms: StrobeUniforms,
    /// Overlay sprite/text pass, again its own program: unlike `program`
    /// (fullscreen, no geometry uniforms) and `strobe_program` (fullscreen,
    /// no texture), this one
    /// builds a positioned/rotated quad in its vertex stage. Shares
    /// `empty_vao` with both, same `gl_VertexID`-only trick.
    overlay_program: glow::NativeProgram,
    overlay_uniforms: OverlayUniforms,
    /// Scratch full-frame copy of `color_tex`, for the two overlay blend
    /// modes whose math reads the destination (`overlay`, `hard-light`).
    /// Created on first use, not in `new`: it costs `COMP_W*COMP_H*4` =
    /// 8 MB of VRAM, and every overlay a session ever draws may well use
    /// one of the four fixed-function modes instead, in which case this
    /// stays `None` for the whole run.
    backdrop_tex: Option<glow::NativeTexture>,
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

            let strobe_program = build_program_from(gl, STROBE_VERTEX_SRC, STROBE_FRAGMENT_SRC)?;
            let strobe_uniforms = StrobeUniforms {
                u_color: gl.get_uniform_location(strobe_program, "uColor"),
                u_intensity: gl.get_uniform_location(strobe_program, "uIntensity"),
            };

            // Left enabled for the lifetime of this context: every layer
            // draw sets its own blendFuncSeparate/blendEquation before
            // drawing, same as the WebGL2 source enabling it once in its
            // constructor and never touching GL_BLEND's enable bit again.
            gl.enable(glow::BLEND);

            let overlay_program = build_program_from(gl, OVERLAY_VERTEX_SRC, OVERLAY_FRAGMENT_SRC)?;
            let overlay_uniforms = OverlayUniforms {
                u_center_px: gl.get_uniform_location(overlay_program, "uCenterPx"),
                u_half_px: gl.get_uniform_location(overlay_program, "uHalfPx"),
                u_rot_rad: gl.get_uniform_location(overlay_program, "uRotRad"),
                u_viewport_px: gl.get_uniform_location(overlay_program, "uViewportPx"),
                u_tex: gl.get_uniform_location(overlay_program, "uTex"),
                u_backdrop: gl.get_uniform_location(overlay_program, "uBackdrop"),
                u_opacity: gl.get_uniform_location(overlay_program, "uOpacity"),
                u_mode: gl.get_uniform_location(overlay_program, "uMode"),
            };

            let composite_timer = PassTimer::new(gl).map_err(|e| format!("composite_timer: {e}"))?;

            Ok(Self {
                fbo,
                color_tex,
                program,
                uniforms,
                empty_vao,
                composite_timer,
                strobe_program,
                strobe_uniforms,
                overlay_program,
                overlay_uniforms,
                backdrop_tex: None,
            })
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

    /// Ends the "composite" pass's timer. Call once per frame, after the
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
            // sat/bright/contrast, same mapping color_params_to_filter uses.
            gl.uniform_1_f32(self.uniforms.u_hue_rotate_deg.as_ref(), (input.color.hue_rotate * 360.0) as f32);
            gl.uniform_1_f32(self.uniforms.u_saturate_mul.as_ref(), (input.color.saturate * 2.0) as f32);
            gl.uniform_1_f32(self.uniforms.u_brightness_mul.as_ref(), (input.color.brightness * 2.0) as f32);
            gl.uniform_1_f32(self.uniforms.u_contrast_mul.as_ref(), (input.color.contrast * 2.0) as f32);
            gl.uniform_1_f32(self.uniforms.u_invert_amount.as_ref(), input.color.invert as f32);

            gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }
    }

    /// Draws the video background layer: one full-screen quad sampling a
    /// decoded clip / camera / capture frame (`opendrop_io::video_capture`),
    /// always in normal (alpha-over) blend at its own opacity, with `color`
    /// carrying the beat-reactive flash/hue (`core::video::VideoState::
    /// layer_color_params`).
    ///
    /// **Reuses the deck shader verbatim** rather than adding a fifth
    /// program: `compositor.ts`'s own video layer does exactly this,
    /// driving the same `uOpacity`/`uHueRotateDeg`/`uBrightnessMul`
    /// uniforms on the same program with keying switched off (see that
    /// file's `videoActive` block). Everything this layer needs the deck
    /// shader already does, so this method is a named, documented entry
    /// point onto [`Compositor::composite_layer`], not a copy of it. That
    /// is also why there is no `uVideo*` anything: the "hue-rotate on the
    /// beat" toggle is the deck shader's existing hue-rotate path, fed by
    /// the beat detector instead of the Color panel's slider.
    ///
    /// **Position in the frame: on top of the 4 decks, under the NDI-in
    /// layer, the strobe flash and the overlays**, not behind the decks.
    /// It might seem more natural to draw it behind the decks; the
    /// OpenDrop-VJ compositor this ports says the opposite, in a class
    /// header that records *why*: drawing the video first made it
    /// disappear the moment any deck slot reached full opacity ("confirmed
    /// live"), which is the default state at either end of the crossfader. Behind-the-decks
    /// would therefore ship a layer that is invisible in the app's normal
    /// configuration. Drawing it last also keeps
    /// `should_force_normal_for_lowest_slot` correct as-is (the reference
    /// makes the same point in that function's own doc comment): the deck
    /// stack still starts against a transparent framebuffer.
    ///
    /// Skipped below the same 0.001 opacity floor every other pass uses,
    /// so a fully faded-out layer costs nothing.
    pub fn composite_video_layer(&self, gl: &glow::Context, video_tex: glow::NativeTexture, opacity: f32, color: ColorParams) {
        let input = LayerInput { opacity, composite: DEFAULT_SLOT_COMPOSITE, color };
        // `force_normal: false`. There is nothing to override:
        // `DEFAULT_SLOT_COMPOSITE` already carries `BlendMode::Normal`, which
        // is what `force_normal` would coerce it to anyway (same reasoning as
        // the NDI-in layer's own call site in `app`).
        self.composite_layer(gl, video_tex, &input, false);
    }

    /// Draws the BPM-synced strobe flash as a fullscreen quad,
    /// additive-blended into the composite FBO
    /// on top of everything already drawn there this frame. Call once per
    /// frame, after the deck, video ([`Compositor::composite_video_layer`])
    /// and NDI-in `composite_layer` calls and before
    /// `blit_to_current_window`/the compositor readback (`FrameReadback`),
    /// so the flash shows up in the control preview, the output window,
    /// and NDI/v4l2 alike: all four read `color_tex` through this same
    /// FBO. A no-op below/at 0 intensity: skip the whole draw rather than
    /// blending in a fully transparent quad, same "opacity floor" idiom
    /// `composite_layer` uses for a slot at 0 opacity.
    pub fn render_strobe_flash(&self, gl: &glow::Context, color: [f32; 3], intensity: f32) {
        if intensity <= 0.0 {
            return;
        }
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, COMP_W as i32, COMP_H as i32);
            // Additive: dst += src.rgb * src.a, dst alpha left alone (ZERO
            // src-alpha factor into the dst-alpha accumulator): the flash
            // brightens the frame without depressing the alpha channel
            // `composite_layer`'s own blending already built up.
            gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE, glow::ZERO, glow::ONE);
            gl.blend_equation(glow::FUNC_ADD);

            gl.use_program(Some(self.strobe_program));
            gl.bind_vertex_array(Some(self.empty_vao));

            gl.uniform_3_f32(self.strobe_uniforms.u_color.as_ref(), color[0], color[1], color[2]);
            gl.uniform_1_f32(self.strobe_uniforms.u_intensity.as_ref(), intensity);

            gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }
    }

    /// Draws one overlay sprite (an image, or a rasterized string; see
    /// `overlay_texture`) as a positioned, rotated, scaled quad in the
    /// composite FBO. Call once per *visible* overlay
    /// (`core::overlay::visible_overlay_ids` decides which those are),
    /// inside the same `begin_frame`/`end_frame` bracket as every other
    /// pass, so the result lands in `color_tex` for the readback and for
    /// each window's `blit_to_current_window`: the control preview, the
    /// output window, NDI and v4l2 all read that one texture, so this pass
    /// needs no per-consumer wiring.
    ///
    /// Position in the frame: after the deck/NDI-in `composite_layer`
    /// calls (an overlay is over the visuals, never under them) and after
    /// `render_strobe_flash`. Nothing dictates the relative order of
    /// strobe and overlay; strobe-first is chosen so an overlay stays
    /// readable through a flash instead of being washed out by it: the
    /// flash is an effect on the visuals, the overlay is content on top of
    /// the result.
    ///
    /// Skipped entirely below the 0.001 opacity floor (same idiom as
    /// `composite_layer`/`render_strobe_flash`), at a non-positive scale,
    /// or for a degenerate texture.
    pub fn composite_overlay(&mut self, gl: &glow::Context, input: &OverlayLayerInput) {
        if input.tex_w == 0 || input.tex_h == 0 {
            return;
        }
        if !input.opacity.is_finite() || input.opacity <= 0.001 {
            return;
        }
        if !input.scale.is_finite() || input.scale <= 0.0 {
            return;
        }
        let (half_w, half_h) = overlay_quad_half_size_px(input.tex_w, input.tex_h, input.scale);
        let (center_x, center_y) = overlay_center_px(input.x, input.y);
        // A NaN slider value (or a NaN drift/spin accumulation) would draw
        // a degenerate quad rather than nothing; reject the whole draw.
        if !(half_w > 0.0 && half_h > 0.0 && center_x.is_finite() && center_y.is_finite() && input.rotation_deg.is_finite())
        {
            return;
        }

        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, COMP_W as i32, COMP_H as i32);
        }

        // Snapshot the destination first, for the two modes that need it.
        // `copy_tex_sub_image_2d` reads the READ_FRAMEBUFFER binding,
        // which the `bind_framebuffer(FRAMEBUFFER, ..)` above just set to
        // our own FBO, and it writes into a texture that is NOT one of
        // that FBO's attachments, so there is no feedback loop.
        let mode = if input.blend_mode.needs_backdrop() {
            match self.capture_backdrop(gl) {
                Some(_) => input.blend_mode,
                // Backdrop allocation failed (out of VRAM, driver
                // refusal): degrade to `Normal` rather than dropping the
                // overlay: a slightly wrong blend beats an invisible
                // sprite mid-set.
                None => OverlayBlendMode::Normal,
            }
        } else {
            // No copy at all for the four fixed-function modes: this is a
            // full 1920x1080 texture copy per call, and most overlays
            // never need it.
            input.blend_mode
        };
        let (src_rgb, dst_rgb, src_a, dst_a) = mode.blend_state();

        unsafe {
            gl.blend_func_separate(src_rgb, dst_rgb, src_a, dst_a);
            gl.blend_equation(glow::FUNC_ADD);

            gl.use_program(Some(self.overlay_program));
            gl.bind_vertex_array(Some(self.empty_vao));

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(input.texture));
            gl.uniform_1_i32(self.overlay_uniforms.u_tex.as_ref(), 0);
            // Always bound, even for a mode that ignores it: an ES
            // fragment shader with an unbound sampler is undefined
            // behavior on some drivers even along a branch it never takes.
            // Falls back to the sprite's own texture when no backdrop
            // exists, which is harmless. Nothing samples it then.
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.backdrop_tex.unwrap_or(input.texture)));
            gl.uniform_1_i32(self.overlay_uniforms.u_backdrop.as_ref(), 1);

            gl.uniform_2_f32(self.overlay_uniforms.u_center_px.as_ref(), center_x, center_y);
            gl.uniform_2_f32(self.overlay_uniforms.u_half_px.as_ref(), half_w, half_h);
            // Negated: CSS `rotate(+deg)` turns clockwise on screen, and
            // this quad is built in GL's y-up pixel space where a positive
            // angle turns counter-clockwise.
            gl.uniform_1_f32(self.overlay_uniforms.u_rot_rad.as_ref(), -input.rotation_deg.to_radians());
            gl.uniform_2_f32(self.overlay_uniforms.u_viewport_px.as_ref(), COMP_W as f32, COMP_H as f32);
            gl.uniform_1_f32(self.overlay_uniforms.u_opacity.as_ref(), input.opacity.clamp(0.0, 1.0));
            gl.uniform_1_i32(self.overlay_uniforms.u_mode.as_ref(), mode.shader_mode());

            gl.draw_arrays(glow::TRIANGLES, 0, 6);

            // Leave the active texture unit where every other pass in this
            // file (and egui_glow, and `deck`'s upload paths) assumes it:
            // unit 0. This is the only pass that ever moves it.
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
        }
    }

    /// Copies the composite FBO's current contents into `backdrop_tex`,
    /// allocating it on first use. `None` means the texture could not be
    /// created. See `composite_overlay`'s fallback.
    fn capture_backdrop(&mut self, gl: &glow::Context) -> Option<glow::NativeTexture> {
        // Pin the unit first: everything below binds a texture, and the
        // lazy-allocation branch would otherwise do so on whichever unit
        // the previous pass happened to leave active.
        unsafe { gl.active_texture(glow::TEXTURE0) };
        let tex = match self.backdrop_tex {
            Some(tex) => tex,
            None => unsafe {
                let tex = match gl.create_texture() {
                    Ok(tex) => tex,
                    Err(e) => {
                        eprintln!("[engine] overlay backdrop texture creation failed: {e}");
                        return None;
                    }
                };
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
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
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                self.backdrop_tex = Some(tex);
                tex
            },
        };
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, COMP_W as i32, COMP_H as i32);
        }
        Some(tex)
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
    build_program_from(gl, VERTEX_SRC, FRAGMENT_SRC)
}

/// Compiles+links one vertex/fragment pair into a program. Factored out of
/// `build_program` so the strobe pass's own tiny program can
/// share the same compile/link/error-handling path instead of duplicating
/// it.
fn build_program_from(gl: &glow::Context, vertex_src: &str, fragment_src: &str) -> Result<glow::NativeProgram, String> {
    unsafe {
        let vs = compile_shader(gl, glow::VERTEX_SHADER, vertex_src)?;
        let fs = compile_shader(gl, glow::FRAGMENT_SHADER, fragment_src)?;
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

const STROBE_VERTEX_SRC: &str = r#"#version 300 es
const vec2 verts[6] = vec2[6](vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(-1.0,1.0), vec2(-1.0,1.0), vec2(1.0,-1.0), vec2(1.0,1.0));
void main() {
	gl_Position = vec4(verts[gl_VertexID], 0.0, 1.0);
}
"#;

const STROBE_FRAGMENT_SRC: &str = r#"#version 300 es
precision highp float;
uniform vec3 uColor;
uniform float uIntensity;
out vec4 fragColor;
void main() {
	fragColor = vec4(uColor, uIntensity);
}
"#;

struct StrobeUniforms {
    u_color: Option<glow::NativeUniformLocation>,
    u_intensity: Option<glow::NativeUniformLocation>,
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

mod overlay;
pub use overlay::{overlay_center_px, overlay_quad_half_size_px, OverlayBlendMode, OverlayLayerInput};
use overlay::{OverlayUniforms, OVERLAY_FRAGMENT_SRC, OVERLAY_VERTEX_SRC};

#[cfg(test)]
mod tests;
