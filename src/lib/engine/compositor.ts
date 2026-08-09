/**
 * Compositor — per-deck WebGL blend/keying compositor.
 *
 * Owns one visible WebGL2 canvas. Each Deck/Butterchurn instance keeps
 * rendering into its own canvas exactly as before — this class uploads
 * that canvas as a texture every frame (from inside its own rAF loop,
 * which is required: reading a live WebGL canvas outside an rAF callback
 * can see an already-cleared buffer, confirmed by a feasibility spike)
 * and composites the 4 layers with native GPU blend equations, no
 * framebuffer ping-pong.
 *
 * A 5th layer — the video-loop <video> element — draws LAST, on top of the 4
 * deck layers, always in 'normal' (alpha-over) blend using its own opacity.
 * This makes the video's own opacity slider the sole control of how much of
 * it shows — independent of deck opacity/crossfader — since a deck slot at
 * full opacity would otherwise fully occlude anything drawn underneath it
 * (confirmed live: drawing video first/bottom made it disappear whenever a
 * deck's own opacity reached 1, e.g. crossfader at either extreme). Replaces
 * the old approach of compositing the <video> as a separate DOM layer behind
 * this canvas via CSS `mix-blend-mode: screen`, found unreliable across two
 * independently GPU-composited surfaces on some Chromium/Mesa/Wayland
 * stacks (see 2026-07-20-video-compositor-integration-design.md). Same
 * rAF-upload discipline applies: a <video> is a valid TexImageSource
 * too, uploaded from inside drawFrame() like the deck canvases.
 */

import type { BlendMode, SlotComposite, ColorParams } from './sync.js';
import { DEFAULT_SLOT_COMPOSITE, DEFAULT_COLOR_PARAMS } from './sync.js';

export type { BlendMode };

/**
 * Symbolic blend factors — NOT real WebGL enum values. The GL-facing
 * Compositor maps these to gl.ONE / gl.SRC_COLOR / etc at draw time, so this
 * module stays testable in plain Node without a WebGL context.
 */
export const GLBlend = {
	ZERO: 'ZERO',
	ONE: 'ONE',
	SRC_COLOR: 'SRC_COLOR',
	ONE_MINUS_SRC_COLOR: 'ONE_MINUS_SRC_COLOR',
	SRC_ALPHA: 'SRC_ALPHA',
	ONE_MINUS_SRC_ALPHA: 'ONE_MINUS_SRC_ALPHA',
} as const;

type GLBlendFactor = (typeof GLBlend)[keyof typeof GLBlend];

export interface BlendState {
	srcRGB: GLBlendFactor;
	dstRGB: GLBlendFactor;
	srcA: GLBlendFactor;
	dstA: GLBlendFactor;
}

const BLEND_MODES: readonly BlendMode[] = ['normal', 'additive', 'screen', 'multiply'];

/**
 * GPU blend-equation factors for each mode. Alpha coverage is constant
 * across all modes so keyed-out / transparent regions still reveal
 * whatever is behind the compositor canvas (video layer, background).
 */
export function blendStateFor(mode: BlendMode): BlendState {
	const alpha = { srcA: GLBlend.ONE, dstA: GLBlend.ONE_MINUS_SRC_ALPHA } as const;
	switch (mode) {
		case 'normal':
			return { srcRGB: GLBlend.ONE, dstRGB: GLBlend.ONE_MINUS_SRC_ALPHA, ...alpha };
		case 'additive':
			return { srcRGB: GLBlend.ONE, dstRGB: GLBlend.ONE, ...alpha };
		case 'screen':
			return { srcRGB: GLBlend.ONE, dstRGB: GLBlend.ONE_MINUS_SRC_COLOR, ...alpha };
		case 'multiply':
			return { srcRGB: GLBlend.ZERO, dstRGB: GLBlend.SRC_COLOR, ...alpha };
	}
}

/**
 * Decode a MIDI/keyboard range value (0..1) into one of the 4 blend modes.
 * 4 equal buckets: [0,.25)→normal [.25,.5)→additive [.5,.75)→screen [.75,1]→multiply.
 */
export function blendModeFromValue01(v: number): BlendMode {
	const idx = Math.min(BLEND_MODES.length - 1, Math.max(0, Math.floor(v * BLEND_MODES.length)));
	return BLEND_MODES[idx];
}

/**
 * Inverse of blendModeFromValue01 — the bucket center for a mode, used so
 * MIDI soft-takeover has a "current value" to compare an incoming CC against.
 */
export function blendModeToValue01(mode: BlendMode): number {
	const idx = BLEND_MODES.indexOf(mode);
	return (idx + 0.5) / BLEND_MODES.length;
}

/**
 * One-shot migration from the old global CSS `mix-blend-mode` string
 * (od-blendmode) to the new BlendMode enum. Modes with no equivalent
 * collapse to 'normal'.
 */
export function migrateBlendModeString(old: string): BlendMode {
	if (old === 'screen') return 'screen';
	if (old === 'multiply') return 'multiply';
	if (old === 'plus-lighter') return 'additive';
	return 'normal';
}

/** Per-slot compositing config for the 4 decks, indexed 0-3. */
export type SlotComposites = [SlotComposite, SlotComposite, SlotComposite, SlotComposite];

/** Merge a patch into one slot's SlotComposite. Pure — returns a new tuple. */
export function withSlotComposite(composites: SlotComposites, slot: number, patch: Partial<SlotComposite>): SlotComposites {
	const next = [...composites] as SlotComposites;
	next[slot] = { ...next[slot], ...patch };
	return next;
}

/**
 * Whether the video layer should draw this frame — opacity above the same 0.001
 * floor the deck slots use, plus a minimum readyState (HAVE_CURRENT_DATA = 2) so a
 * <video> with no decoded frame yet doesn't upload garbage or count as "already
 * opaque" for shouldForceNormalForLowestSlot below.
 */
export function isVideoLayerActive(hasSource: boolean, opacity: number, readyState: number): boolean {
	return hasSource && opacity > 0.001 && readyState >= 2;
}

/**
 * Whether the lowest active deck slot should be forced to 'normal' blend — multiply/
 * screen/additive against a still-transparent framebuffer reads wrong (e.g. multiply →
 * black). Independent of the video layer, which now draws last, on top of the deck
 * stack, not underneath it.
 */
export function shouldForceNormalForLowestSlot(slot: number, lowestActive: number): boolean {
	return slot === lowestActive;
}

function glFactor(gl: WebGL2RenderingContext, f: GLBlendFactor): number {
	switch (f) {
		case 'ZERO': return gl.ZERO;
		case 'ONE': return gl.ONE;
		case 'SRC_COLOR': return gl.SRC_COLOR;
		case 'ONE_MINUS_SRC_COLOR': return gl.ONE_MINUS_SRC_COLOR;
		case 'SRC_ALPHA': return gl.SRC_ALPHA;
		case 'ONE_MINUS_SRC_ALPHA': return gl.ONE_MINUS_SRC_ALPHA;
	}
}

// Fullscreen triangle-pair via gl_VertexID — no vertex buffer needed.
const VERTEX_SRC = `#version 300 es
const vec2 verts[6] = vec2[6](vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(-1.0,1.0), vec2(-1.0,1.0), vec2(1.0,-1.0), vec2(1.0,1.0));
out vec2 vUV;
void main() {
	vec2 p = verts[gl_VertexID];
	gl_Position = vec4(p, 0.0, 1.0);
	vUV = p * 0.5 + 0.5;
	vUV.y = 1.0 - vUV.y;
}`;

// LumaKey (black/white threshold smoothstep) + ColorKey (hue distance + tolerance)
// compute an alpha mask from the RAW deck pixel — keying always targets what's
// actually in the preset, unaffected by the color-correction below (otherwise a
// chroma key + a hue-rotate would fight each other in a confusing way). Color
// params (hue/sat/bright/contrast/invert) are the same 5 ops the old CSS `filter`
// applied, in the same order, folded here so they still work once the source
// canvases are hidden behind the compositor. Multiply mode needs a different RGB
// formula than the other three (which share the same premultiplied C*A output —
// only their GPU blendFunc differs, set by the caller via blendStateFor).
const FRAGMENT_SRC = `#version 300 es
precision highp float;
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
}`;

interface Uniforms {
	uTex: WebGLUniformLocation | null;
	uMultiply: WebGLUniformLocation | null;
	uOpacity: WebGLUniformLocation | null;
	uLumaOn: WebGLUniformLocation | null;
	uLumaBlack: WebGLUniformLocation | null;
	uLumaWhite: WebGLUniformLocation | null;
	uColorOn: WebGLUniformLocation | null;
	uKeyHue: WebGLUniformLocation | null;
	uKeyTol: WebGLUniformLocation | null;
	uHueRotateDeg: WebGLUniformLocation | null;
	uSaturateMul: WebGLUniformLocation | null;
	uBrightnessMul: WebGLUniformLocation | null;
	uContrastMul: WebGLUniformLocation | null;
	uInvertAmount: WebGLUniformLocation | null;
}

interface Layer {
	source: HTMLCanvasElement | null;
	opacity: number;
	config: SlotComposite;
	color: ColorParams;
}

const SLOT_COUNT = 4;

export class Compositor {
	private readonly canvas: HTMLCanvasElement;
	private gl: WebGL2RenderingContext;
	private program!: WebGLProgram;
	private uniforms!: Uniforms;
	private textures: WebGLTexture[] = [];
	private layers: Layer[] = Array.from({ length: SLOT_COUNT }, () => ({
		source: null,
		opacity: 0,
		config: DEFAULT_SLOT_COMPOSITE,
		color: DEFAULT_COLOR_PARAMS,
	}));
	private videoTexture: WebGLTexture | null = null;
	private videoSource: HTMLVideoElement | null = null;
	private videoOpacity = 0;
	private videoBrightness = 1;
	private videoHueRotateDeg = 0;
	private rafId: number | null = null;

	constructor(canvas: HTMLCanvasElement) {
		this.canvas = canvas;
		// preserveDrawingBuffer: true — without it the browser is free to clear
		// this buffer immediately after each frame is presented on-screen (a
		// perf optimization). On-screen scanout is unaffected, but any
		// asynchronous readback of this canvas — Electron's capturePage()
		// (NDI/v4l2/Spout output all route through it), canvas.toBlob()
		// snapshots, etc. — can land after that clear and read back solid
		// black even though the window visibly shows the real frame.
		//
		// alpha: false — this canvas is the entire visible output (video is
		// drawn INTO it as a texture layer via attachVideoSource, not
		// DOM-composited behind it), so it never needs real transparency.
		// With alpha:true + premultipliedAlpha:true (WebGL's default), any
		// partially-transparent blend layer gets its RGB scaled down toward
		// black at the drawing-buffer level — invisible on-screen (the OS
		// compositor blends it against whatever's behind the window), but a
		// flat capturePage()/toBitmap() readback sees those scaled-down,
		// near-black values directly. Forcing an opaque backbuffer sidesteps
		// that entirely.
		const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, preserveDrawingBuffer: true });
		if (!gl) throw new Error('Compositor: WebGL2 unavailable on the compositor canvas.');
		this.gl = gl;
		this.setup();
		gl.enable(gl.BLEND);
		canvas.addEventListener('webglcontextlost', this.onContextLost);
		canvas.addEventListener('webglcontextrestored', this.onContextRestored);
	}

	/** Bind a deck's raw canvas as the texture source for a slot (0-3). */
	attachSource(slot: number, source: HTMLCanvasElement): void {
		this.layers[slot].source = source;
	}

	/** Update a slot's opacity + composite config. Called reactively, not per-frame by the caller. */
	setLayer(slot: number, opacity: number, config: SlotComposite): void {
		this.layers[slot].opacity = opacity;
		this.layers[slot].config = config;
	}

	/** Update a slot's color params (hue/sat/bright/contrast/invert) — same 5 ops the old CSS filter applied. */
	setColor(slot: number, color: ColorParams): void {
		this.layers[slot].color = color;
	}

	/** Bind the DOM <video> element as the texture source for the bottom video layer (null = none/disabled). */
	attachVideoSource(source: HTMLVideoElement | null): void {
		this.videoSource = source;
	}

	/** Update the video layer's opacity + beat-reactive brightness/hue. Same uOpacity/
	 * uBrightnessMul/uHueRotateDeg uniforms the 4 deck slots already use — no shader change. */
	setVideoLayer(opacity: number, brightness: number, hueRotateDeg: number): void {
		this.videoOpacity = opacity;
		this.videoBrightness = brightness;
		this.videoHueRotateDeg = hueRotateDeg;
	}

	/** Resize the drawing buffer. CSS sizing (100% via stylesheet) handles display scaling. */
	resize(w: number, h: number, pixelRatio = 1): void {
		this.canvas.width = Math.max(1, Math.round(w * pixelRatio));
		this.canvas.height = Math.max(1, Math.round(h * pixelRatio));
	}

	start(): void {
		if (this.rafId !== null) return;
		const tick = () => {
			// A thrown drawFrame() (e.g. a tainted cross-origin video texture) would
			// otherwise never reach the requestAnimationFrame(tick) below, permanently
			// killing the render loop for the rest of the session — caught here so one
			// bad frame is skipped instead of freezing everything.
			try {
				this.drawFrame();
			} catch (e) {
				console.error('Compositor: drawFrame failed, skipping this frame', e);
			}
			this.rafId = requestAnimationFrame(tick);
		};
		this.rafId = requestAnimationFrame(tick);
	}

	stop(): void {
		if (this.rafId !== null) {
			cancelAnimationFrame(this.rafId);
			this.rafId = null;
		}
	}

	destroy(): void {
		this.stop();
		this.canvas.removeEventListener('webglcontextlost', this.onContextLost);
		this.canvas.removeEventListener('webglcontextrestored', this.onContextRestored);
		const gl = this.gl;
		for (const tex of this.textures) gl.deleteTexture(tex);
		this.textures = [];
		if (this.videoTexture) gl.deleteTexture(this.videoTexture);
		this.videoTexture = null;
		gl.deleteProgram(this.program);
	}

	private setup(): void {
		this.program = this.buildProgram();
		this.uniforms = this.locateUniforms();
		this.textures = this.createTextures();
		this.videoTexture = this.createTextures(1)[0];
	}

	private buildProgram(): WebGLProgram {
		const gl = this.gl;
		const compile = (type: number, src: string): WebGLShader => {
			const shader = gl.createShader(type)!;
			gl.shaderSource(shader, src);
			gl.compileShader(shader);
			if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
				const info = gl.getShaderInfoLog(shader);
				gl.deleteShader(shader);
				throw new Error(`Compositor: shader compile failed — ${info}`);
			}
			return shader;
		};
		const program = gl.createProgram()!;
		gl.attachShader(program, compile(gl.VERTEX_SHADER, VERTEX_SRC));
		gl.attachShader(program, compile(gl.FRAGMENT_SHADER, FRAGMENT_SRC));
		gl.linkProgram(program);
		if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
			const info = gl.getProgramInfoLog(program);
			throw new Error(`Compositor: program link failed — ${info}`);
		}
		return program;
	}

	private locateUniforms(): Uniforms {
		const gl = this.gl;
		const loc = (name: string) => gl.getUniformLocation(this.program, name);
		return {
			uTex: loc('uTex'),
			uMultiply: loc('uMultiply'),
			uOpacity: loc('uOpacity'),
			uLumaOn: loc('uLumaOn'),
			uLumaBlack: loc('uLumaBlack'),
			uLumaWhite: loc('uLumaWhite'),
			uColorOn: loc('uColorOn'),
			uKeyHue: loc('uKeyHue'),
			uKeyTol: loc('uKeyTol'),
			uHueRotateDeg: loc('uHueRotateDeg'),
			uSaturateMul: loc('uSaturateMul'),
			uBrightnessMul: loc('uBrightnessMul'),
			uContrastMul: loc('uContrastMul'),
			uInvertAmount: loc('uInvertAmount'),
		};
	}

	private createTextures(count: number = SLOT_COUNT): WebGLTexture[] {
		const gl = this.gl;
		const textures: WebGLTexture[] = [];
		for (let i = 0; i < count; i++) {
			const tex = gl.createTexture()!;
			gl.bindTexture(gl.TEXTURE_2D, tex);
			gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
			gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
			gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
			gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
			textures.push(tex);
		}
		return textures;
	}

	private drawFrame(): void {
		const gl = this.gl;
		gl.viewport(0, 0, this.canvas.width, this.canvas.height);
		gl.clearColor(0, 0, 0, 0);
		gl.clear(gl.COLOR_BUFFER_BIT);
		gl.useProgram(this.program);

		// Deck slots first — composited among themselves exactly as before the video
		// layer existed, independent of it (video draws last, on top — see below).
		const lowestActive = this.layers.findIndex((l) => l.source && l.opacity > 0.001);

		for (let slot = 0; slot < SLOT_COUNT; slot++) {
			const layer = this.layers[slot];
			if (!layer.source || layer.opacity <= 0.001) continue;

			// Force normal on the lowest active layer — multiply against a
			// transparent framebuffer would otherwise read as black.
			const mode: BlendMode = shouldForceNormalForLowestSlot(slot, lowestActive) ? 'normal' : layer.config.blend;
			const bs = blendStateFor(mode);
			gl.blendFuncSeparate(
				glFactor(gl, bs.srcRGB), glFactor(gl, bs.dstRGB),
				glFactor(gl, bs.srcA), glFactor(gl, bs.dstA),
			);
			gl.blendEquation(gl.FUNC_ADD);

			gl.activeTexture(gl.TEXTURE0);
			gl.bindTexture(gl.TEXTURE_2D, this.textures[slot]);
			gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, layer.source);

			gl.uniform1i(this.uniforms.uTex, 0);
			gl.uniform1i(this.uniforms.uMultiply, mode === 'multiply' ? 1 : 0);
			gl.uniform1f(this.uniforms.uOpacity, layer.opacity);
			gl.uniform1i(this.uniforms.uLumaOn, layer.config.lumaKey ? 1 : 0);
			gl.uniform1f(this.uniforms.uLumaBlack, layer.config.lumaBlack);
			gl.uniform1f(this.uniforms.uLumaWhite, layer.config.lumaWhite);
			gl.uniform1i(this.uniforms.uColorOn, layer.config.colorKey ? 1 : 0);
			gl.uniform1f(this.uniforms.uKeyHue, layer.config.colorHue);
			gl.uniform1f(this.uniforms.uKeyTol, layer.config.colorTol);
			// ColorParams fields are 0..1 with 0.5 = neutral (100%) for sat/bright/contrast —
			// same mapping colorParamsToFilter() uses for the CSS filter string equivalent.
			gl.uniform1f(this.uniforms.uHueRotateDeg, layer.color.hueRotate * 360);
			gl.uniform1f(this.uniforms.uSaturateMul, layer.color.saturate * 2);
			gl.uniform1f(this.uniforms.uBrightnessMul, layer.color.brightness * 2);
			gl.uniform1f(this.uniforms.uContrastMul, layer.color.contrast * 2);
			gl.uniform1f(this.uniforms.uInvertAmount, layer.color.invert);

			gl.drawArrays(gl.TRIANGLES, 0, 6);
		}

		// Video layer last, on top — normal (alpha-over) blend using its own opacity, so
		// it always shows at exactly its own slider strength over whatever the decks just
		// produced, regardless of deck opacity/crossfader (see class header comment).
		const videoActive = isVideoLayerActive(this.videoSource !== null, this.videoOpacity, this.videoSource?.readyState ?? 0);
		if (videoActive) {
			const vbs = blendStateFor('normal');
			gl.blendFuncSeparate(
				glFactor(gl, vbs.srcRGB), glFactor(gl, vbs.dstRGB),
				glFactor(gl, vbs.srcA), glFactor(gl, vbs.dstA),
			);
			gl.blendEquation(gl.FUNC_ADD);

			gl.activeTexture(gl.TEXTURE0);
			gl.bindTexture(gl.TEXTURE_2D, this.videoTexture);
			gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, this.videoSource!);

			gl.uniform1i(this.uniforms.uTex, 0);
			gl.uniform1i(this.uniforms.uMultiply, 0);
			gl.uniform1f(this.uniforms.uOpacity, this.videoOpacity);
			gl.uniform1i(this.uniforms.uLumaOn, 0);
			gl.uniform1f(this.uniforms.uLumaBlack, 0);
			gl.uniform1f(this.uniforms.uLumaWhite, 1);
			gl.uniform1i(this.uniforms.uColorOn, 0);
			gl.uniform1f(this.uniforms.uKeyHue, 0);
			gl.uniform1f(this.uniforms.uKeyTol, 0);
			gl.uniform1f(this.uniforms.uHueRotateDeg, this.videoHueRotateDeg);
			gl.uniform1f(this.uniforms.uSaturateMul, 1);
			gl.uniform1f(this.uniforms.uBrightnessMul, this.videoBrightness);
			gl.uniform1f(this.uniforms.uContrastMul, 1);
			gl.uniform1f(this.uniforms.uInvertAmount, 0);

			gl.drawArrays(gl.TRIANGLES, 0, 6);
		}
	}

	private onContextLost = (e: Event): void => {
		e.preventDefault();
		this.stop();
	};

	private onContextRestored = (): void => {
		this.setup();
		this.start();
	};
}
