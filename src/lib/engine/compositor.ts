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
 */

import type { BlendMode, SlotComposite } from './sync.js';
import { DEFAULT_SLOT_COMPOSITE } from './sync.js';

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

export type GLBlendFactor = (typeof GLBlend)[keyof typeof GLBlend];

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
// compute an alpha mask; multiply mode needs a different RGB formula than the
// other three (which all share the same premultiplied C*A output — only their
// GPU blendFunc differs, set by the caller via blendStateFor).
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

void main() {
	vec3 C = texture(uTex, vUV).rgb;

	float mask = 1.0;
	if (uLumaOn) {
		float l = luma(C);
		mask *= smoothstep(uLumaBlack - 0.02, uLumaBlack + 0.02, l);
		mask *= 1.0 - smoothstep(uLumaWhite - 0.02, uLumaWhite + 0.02, l);
	}
	if (uColorOn) {
		float dh = hueDist(rgb2hue(C), uKeyHue);
		mask *= smoothstep(uKeyTol, uKeyTol + 0.05, dh);
	}

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
}

interface Layer {
	source: HTMLCanvasElement | null;
	opacity: number;
	config: SlotComposite;
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
	}));
	private rafId: number | null = null;

	constructor(canvas: HTMLCanvasElement) {
		this.canvas = canvas;
		const gl = canvas.getContext('webgl2', { alpha: true, premultipliedAlpha: true, antialias: false });
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

	/** Resize the drawing buffer. CSS sizing (100% via stylesheet) handles display scaling. */
	resize(w: number, h: number, pixelRatio = 1): void {
		this.canvas.width = Math.max(1, Math.round(w * pixelRatio));
		this.canvas.height = Math.max(1, Math.round(h * pixelRatio));
	}

	start(): void {
		if (this.rafId !== null) return;
		const tick = () => {
			this.drawFrame();
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
		gl.deleteProgram(this.program);
	}

	private setup(): void {
		this.program = this.buildProgram();
		this.uniforms = this.locateUniforms();
		this.textures = this.createTextures();
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
		};
	}

	private createTextures(): WebGLTexture[] {
		const gl = this.gl;
		const textures: WebGLTexture[] = [];
		for (let i = 0; i < SLOT_COUNT; i++) {
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

		const lowestActive = this.layers.findIndex((l) => l.source && l.opacity > 0.001);

		for (let slot = 0; slot < SLOT_COUNT; slot++) {
			const layer = this.layers[slot];
			if (!layer.source || layer.opacity <= 0.001) continue;

			// Force normal on the lowest active layer — multiply against a
			// transparent framebuffer would otherwise read as black.
			const mode: BlendMode = slot === lowestActive ? 'normal' : layer.config.blend;
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
