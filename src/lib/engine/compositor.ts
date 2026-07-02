/**
 * Compositor — pure blend/keying logic (step 1: no WebGL yet).
 *
 * The GL-facing Compositor class (texture upload, shader, draw loop) lands
 * in a later step once the canvas-as-texture feasibility spike is proven.
 * This file only holds the parts that must be correct and unit-testable
 * without a real WebGL context: mapping a BlendMode to blend-equation
 * factors, decoding a MIDI/keyboard range value into a blend mode, and
 * migrating the old single global blend string to the new per-slot model.
 */

import type { BlendMode } from './sync.js';

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
