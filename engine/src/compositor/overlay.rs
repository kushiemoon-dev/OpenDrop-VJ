//! Overlay layer types and shaders for the compositor: the blend modes, the
//! per-frame input, the sprite sizing helpers and the overlay program sources.

use super::*;

/// The 6 `mix-blend-mode` values an overlay can carry
/// (`SidebarOverlays.svelte:32`'s `BLEND_MODES`). Deliberately NOT
/// `core::blend::BlendMode` (4 values, deck-slot compositing): the two
/// lists only partially overlap, mean different things to the user, and
/// are set from different panels.
///
/// Four of these are plain fixed-function GL blend states. `Overlay` and
/// `HardLight` are not expressible that way: they need to read the
/// destination, and go through a backdrop copy instead; see
/// [`Compositor::composite_overlay`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayBlendMode {
    /// `Overlay::default().blend_mode`, hence the `Default` here.
    #[default]
    Screen,
    Normal,
    PlusLighter,
    Multiply,
    Overlay,
    HardLight,
}

impl OverlayBlendMode {
    /// In `SidebarOverlays.svelte`'s dropdown order, so the native panel
    /// lists them the same way.
    pub const ALL: [OverlayBlendMode; 6] = [
        OverlayBlendMode::Screen,
        OverlayBlendMode::Normal,
        OverlayBlendMode::PlusLighter,
        OverlayBlendMode::Multiply,
        OverlayBlendMode::Overlay,
        OverlayBlendMode::HardLight,
    ];

    /// The CSS keyword, which is what `core::overlay::Overlay::blend_mode`
    /// stores (a `String`, faithful to the TS type it was ported from).
    pub fn as_css(self) -> &'static str {
        match self {
            OverlayBlendMode::Screen => "screen",
            OverlayBlendMode::Normal => "normal",
            OverlayBlendMode::PlusLighter => "plus-lighter",
            OverlayBlendMode::Multiply => "multiply",
            OverlayBlendMode::Overlay => "overlay",
            OverlayBlendMode::HardLight => "hard-light",
        }
    }

    /// Inverse of [`Self::as_css`]. An unrecognized keyword falls back to
    /// the default (`screen`) rather than failing a frame: the field is a
    /// free-form `String` on the `core` side and can hold anything a
    /// future import path puts there.
    pub fn from_css(css: &str) -> Self {
        Self::ALL.into_iter().find(|m| m.as_css() == css).unwrap_or_default()
    }

    /// Whether this mode's math needs the pixels already in the composite
    /// FBO as a shader input (and therefore a copy of them: you cannot
    /// sample the framebuffer you are drawing into).
    pub(super) fn needs_backdrop(self) -> bool {
        matches!(self, OverlayBlendMode::Overlay | OverlayBlendMode::HardLight)
    }

    /// Value of the fragment shader's `uMode`.
    pub(super) fn shader_mode(self) -> i32 {
        match self {
            OverlayBlendMode::Normal => 0,
            OverlayBlendMode::Screen => 1,
            OverlayBlendMode::PlusLighter => 2,
            OverlayBlendMode::Multiply => 3,
            OverlayBlendMode::Overlay => 4,
            OverlayBlendMode::HardLight => 5,
        }
    }

    /// `(srcRGB, dstRGB, srcAlpha, dstAlpha)` for `glBlendFuncSeparate`.
    ///
    /// The fragment shader pre-shapes its RGB output per mode so these
    /// stay plain fixed-function states, with `a` the sprite's effective
    /// alpha (texel alpha × the overlay's opacity) and `S` its color:
    /// - Normal: `fragColor.rgb = S*a`, `(ONE, ONE_MINUS_SRC_ALPHA)`
    ///   → `S*a + D*(1-a)`, classic "over".
    /// - PlusLighter: same output, `(ONE, ONE)` → `D + S*a`.
    /// - Screen: same output, `(ONE, ONE_MINUS_SRC_COLOR)`
    ///   → `S*a + D*(1 - S*a)`, exactly `screen` weighted by `a`.
    /// - Multiply: `fragColor.rgb = mix(1, S, a)`, `(ZERO, SRC_COLOR)`
    ///   → `D * mix(1, S, a)`, exactly `multiply` weighted by `a`, the
    ///   same trick `composite_layer`'s `uMultiply` uniform uses.
    /// - Overlay/HardLight: the shader has already produced the final
    ///   pixel (it sampled the backdrop itself), so `(ONE, ZERO)` writes
    ///   it through untouched.
    ///
    /// The alpha channel is the standard "over" accumulation
    /// (`a + D_a*(1-a)`) for every fixed-function mode: these modes change
    /// how colors combine, not how coverage does. Keeping it right matters
    /// because the NDI/v4l2 readback ships this FBO's alpha downstream.
    pub(super) fn blend_state(self) -> (u32, u32, u32, u32) {
        match self {
            OverlayBlendMode::Normal => (glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA),
            OverlayBlendMode::PlusLighter => (glow::ONE, glow::ONE, glow::ONE, glow::ONE_MINUS_SRC_ALPHA),
            OverlayBlendMode::Screen => (glow::ONE, glow::ONE_MINUS_SRC_COLOR, glow::ONE, glow::ONE_MINUS_SRC_ALPHA),
            OverlayBlendMode::Multiply => (glow::ZERO, glow::SRC_COLOR, glow::ONE, glow::ONE_MINUS_SRC_ALPHA),
            OverlayBlendMode::Overlay | OverlayBlendMode::HardLight => {
                (glow::ONE, glow::ZERO, glow::ONE, glow::ZERO)
            }
        }
    }
}

/// One overlay sprite's compositing input for one frame: the native
/// replacement for the DOM element `OverlayLayer.svelte` positioned over
/// the visualizer.
///
/// Distinct from [`LayerInput`] on purpose: that one is a full-screen deck
/// slot with keying and color correction, this one is a positioned,
/// rotated, scaled quad.
///
/// `x`/`y` are normalized 0-1 with the CSS convention `Overlay` uses:
/// origin top-left, `y` growing downward. `scale` multiplies the sprite's
/// fitted natural size (see [`overlay_quad_half_size_px`]). `rotation_deg`
/// is clockwise on screen, like CSS `rotate()`.
///
/// `tex_w`/`tex_h` are stored here rather than derived: aspect-correct
/// sizing needs the texture's own dimensions, and GLES 3.0
/// has no `glGetTexLevelParameter` to query them back from the handle.
/// They come free. Every caller has just decoded or rasterized the image.
#[derive(Clone, Copy)]
pub struct OverlayLayerInput {
    pub texture: glow::NativeTexture,
    pub tex_w: u32,
    pub tex_h: u32,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotation_deg: f32,
    pub opacity: f32,
    pub blend_mode: OverlayBlendMode,
}

/// Largest fraction of the frame an unscaled (`scale == 1`) sprite may
/// occupy: port of `OverlayLayer.svelte`'s `max-width: 80vw;
/// max-height: 80vh` on `.overlay-media`.
pub(super) const OVERLAY_MAX_FRACTION: f32 = 0.8;

/// Half-extents, in composite pixels, of the quad an overlay sprite draws
/// into: its intrinsic pixel size shrunk to fit inside
/// `OVERLAY_MAX_FRACTION` of the frame (never enlarged: `max-width`
/// only shrinks), then multiplied by the overlay's `scale`.
///
/// Half-extents rather than full size because the quad is centered on
/// `x`/`y` and rotated about that center, which is exactly what the web's
/// `transform: translate(-50%, -50%) rotate(...)` does.
///
/// Free function, and public, so the sizing rule is unit-testable without
/// a GL context.
pub fn overlay_quad_half_size_px(tex_w: u32, tex_h: u32, scale: f32) -> (f32, f32) {
    let (w, h) = (tex_w as f32, tex_h as f32);
    if w <= 0.0 || h <= 0.0 {
        return (0.0, 0.0);
    }
    let fit = ((COMP_W as f32 * OVERLAY_MAX_FRACTION) / w)
        .min((COMP_H as f32 * OVERLAY_MAX_FRACTION) / h)
        .min(1.0);
    (w * fit * scale * 0.5, h * fit * scale * 0.5)
}

/// An overlay's normalized center, converted to composite pixels in GL's
/// own coordinate system: origin bottom-left, y growing upward. `y` is
/// flipped because `Overlay::y` is CSS-style top-down (it feeds
/// `style="top: {y*100}%"` in the web).
pub fn overlay_center_px(x: f32, y: f32) -> (f32, f32) {
    (x * COMP_W as f32, (1.0 - y) * COMP_H as f32)
}

/// Overlay sprite quad. Unlike the two fullscreen passes above, this
/// vertex stage actually places geometry: it takes a unit quad from
/// `gl_VertexID`, scales it to the sprite's half-extents in composite
/// pixels, rotates it about its own center, translates it to the overlay's
/// center, and only then converts to NDC. Rotating in pixel space rather
/// than NDC is what keeps a rotated sprite from shearing on a non-square
/// frame (1920x1080 here).
///
/// `vUV` flips V (`-c.y`): row 0 of an uploaded image is its TOP row,
/// while GL's texture origin is bottom-left. This is the mirror of the
/// note at the top of this file about the deck shader NOT needing a flip:
/// a deck's texture comes from a GL FBO copy (already bottom-left), an
/// overlay's from a CPU-side decoder (top-left).
pub(super) const OVERLAY_VERTEX_SRC: &str = r#"#version 300 es
const vec2 corners[6] = vec2[6](vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(-1.0,1.0), vec2(-1.0,1.0), vec2(1.0,-1.0), vec2(1.0,1.0));
uniform vec2 uCenterPx;
uniform vec2 uHalfPx;
uniform float uRotRad;
uniform vec2 uViewportPx;
out vec2 vUV;
void main() {
	vec2 c = corners[gl_VertexID];
	vUV = vec2(c.x, -c.y) * 0.5 + 0.5;
	vec2 local = c * uHalfPx;
	float s = sin(uRotRad);
	float co = cos(uRotRad);
	vec2 rotated = vec2(local.x * co - local.y * s, local.x * s + local.y * co);
	gl_Position = vec4((uCenterPx + rotated) / uViewportPx * 2.0 - 1.0, 0.0, 1.0);
}
"#;

/// Companion fragment stage. Four of the six modes only pre-shape the
/// output for a fixed-function blend state (see
/// `OverlayBlendMode::blend_state`); `overlay`/`hard-light` need the
/// destination, so they sample `uBackdrop`, a copy of the composite taken
/// just before this draw, and produce the finished pixel themselves.
pub(super) const OVERLAY_FRAGMENT_SRC: &str = r#"#version 300 es
precision highp float;
precision highp sampler2D;
uniform sampler2D uTex;
uniform sampler2D uBackdrop;
uniform vec2 uViewportPx;
uniform float uOpacity;
uniform int uMode;
in vec2 vUV;
out vec4 fragColor;

// W3C compositing-1 hard-light(backdrop b, source s), component-wise.
vec3 hardLight(vec3 b, vec3 s) {
	return mix(2.0 * b * s, 1.0 - 2.0 * (1.0 - b) * (1.0 - s), step(vec3(0.5), s));
}

void main() {
	vec4 src = texture(uTex, vUV);
	float a = clamp(src.a * uOpacity, 0.0, 1.0);
	if (uMode == 3) {
		// multiply: D * mix(1, S, a), with dstRGB = SRC_COLOR
		fragColor = vec4(mix(vec3(1.0), src.rgb, a), a);
	} else if (uMode == 4 || uMode == 5) {
		vec4 bd = texture(uBackdrop, gl_FragCoord.xy / uViewportPx);
		// overlay(b, s) == hard-light(s, b), the same function with its
		// two arguments swapped (W3C compositing-1).
		vec3 blended = (uMode == 5) ? hardLight(bd.rgb, src.rgb) : hardLight(src.rgb, bd.rgb);
		fragColor = vec4(mix(bd.rgb, blended, a), a + bd.a * (1.0 - a));
	} else {
		// normal / screen / plus-lighter: premultiplied source, the blend
		// state does the rest.
		fragColor = vec4(src.rgb * a, a);
	}
}
"#;

pub(super) struct OverlayUniforms {
    pub(super) u_center_px: Option<glow::NativeUniformLocation>,
    pub(super) u_half_px: Option<glow::NativeUniformLocation>,
    pub(super) u_rot_rad: Option<glow::NativeUniformLocation>,
    pub(super) u_viewport_px: Option<glow::NativeUniformLocation>,
    pub(super) u_tex: Option<glow::NativeUniformLocation>,
    pub(super) u_backdrop: Option<glow::NativeUniformLocation>,
    pub(super) u_opacity: Option<glow::NativeUniformLocation>,
    pub(super) u_mode: Option<glow::NativeUniformLocation>,
}
