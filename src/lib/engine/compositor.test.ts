import { describe, it, expect } from 'vitest';
import { blendStateFor, blendModeFromValue01, blendModeToValue01, migrateBlendModeString, GLBlend, withSlotComposite, type SlotComposites } from './compositor.js';
import { DEFAULT_SLOT_COMPOSITE } from './sync.js';

describe('blendStateFor', () => {
	it('normal → classic over (SRC_ALPHA coverage constant)', () => {
		expect(blendStateFor('normal')).toEqual({
			srcRGB: GLBlend.ONE, dstRGB: GLBlend.ONE_MINUS_SRC_ALPHA,
			srcA: GLBlend.ONE, dstA: GLBlend.ONE_MINUS_SRC_ALPHA,
		});
	});

	it('additive → ONE/ONE in RGB', () => {
		expect(blendStateFor('additive')).toEqual({
			srcRGB: GLBlend.ONE, dstRGB: GLBlend.ONE,
			srcA: GLBlend.ONE, dstA: GLBlend.ONE_MINUS_SRC_ALPHA,
		});
	});

	it('screen → ONE/ONE_MINUS_SRC_COLOR in RGB', () => {
		expect(blendStateFor('screen')).toEqual({
			srcRGB: GLBlend.ONE, dstRGB: GLBlend.ONE_MINUS_SRC_COLOR,
			srcA: GLBlend.ONE, dstA: GLBlend.ONE_MINUS_SRC_ALPHA,
		});
	});

	it('multiply → ZERO/SRC_COLOR in RGB', () => {
		expect(blendStateFor('multiply')).toEqual({
			srcRGB: GLBlend.ZERO, dstRGB: GLBlend.SRC_COLOR,
			srcA: GLBlend.ONE, dstA: GLBlend.ONE_MINUS_SRC_ALPHA,
		});
	});

	it('coverage alpha identical across the 4 modes', () => {
		const modes = ['normal', 'additive', 'screen', 'multiply'] as const;
		for (const m of modes) {
			const bs = blendStateFor(m);
			expect(bs.srcA).toBe(GLBlend.ONE);
			expect(bs.dstA).toBe(GLBlend.ONE_MINUS_SRC_ALPHA);
		}
	});
});

describe('blendModeFromValue01', () => {
	it('0 → normal', () => { expect(blendModeFromValue01(0)).toBe('normal'); });
	it('0.24 → normal', () => { expect(blendModeFromValue01(0.24)).toBe('normal'); });
	it('0.25 → additive', () => { expect(blendModeFromValue01(0.25)).toBe('additive'); });
	it('0.5 → screen', () => { expect(blendModeFromValue01(0.5)).toBe('screen'); });
	it('0.75 → multiply', () => { expect(blendModeFromValue01(0.75)).toBe('multiply'); });
	it('1 → multiply (clamped)', () => { expect(blendModeFromValue01(1)).toBe('multiply'); });
	it('negative out-of-range value → normal (clamped)', () => { expect(blendModeFromValue01(-0.5)).toBe('normal'); });
});

describe('blendModeToValue01', () => {
	it('returns the bucket center for each mode', () => {
		expect(blendModeToValue01('normal')).toBeCloseTo(0.125);
		expect(blendModeToValue01('additive')).toBeCloseTo(0.375);
		expect(blendModeToValue01('screen')).toBeCloseTo(0.625);
		expect(blendModeToValue01('multiply')).toBeCloseTo(0.875);
	});

	it('round-trip with blendModeFromValue01', () => {
		for (const m of ['normal', 'additive', 'screen', 'multiply'] as const) {
			expect(blendModeFromValue01(blendModeToValue01(m))).toBe(m);
		}
	});
});

describe('migrateBlendModeString', () => {
	it('screen → screen', () => { expect(migrateBlendModeString('screen')).toBe('screen'); });
	it('multiply → multiply', () => { expect(migrateBlendModeString('multiply')).toBe('multiply'); });
	it('plus-lighter → additive', () => { expect(migrateBlendModeString('plus-lighter')).toBe('additive'); });
	it('overlay (unsupported) → normal', () => { expect(migrateBlendModeString('overlay')).toBe('normal'); });
	it('lighten (unsupported) → normal', () => { expect(migrateBlendModeString('lighten')).toBe('normal'); });
	it('any unknown value → normal', () => { expect(migrateBlendModeString('garbage')).toBe('normal'); });
});

describe('withSlotComposite', () => {
	const composites: SlotComposites = [
		{ ...DEFAULT_SLOT_COMPOSITE }, { ...DEFAULT_SLOT_COMPOSITE }, { ...DEFAULT_SLOT_COMPOSITE }, { ...DEFAULT_SLOT_COMPOSITE },
	];

	it('updates only the targeted slot', () => {
		const next = withSlotComposite(composites, 1, { blend: 'screen' });
		expect(next[1].blend).toBe('screen');
		expect(next[0].blend).toBe('normal');
		expect(next[2].blend).toBe('normal');
		expect(next[3].blend).toBe('normal');
	});

	it("merges a partial patch without touching the slot's other fields", () => {
		const next = withSlotComposite(composites, 0, { lumaBlack: 0.3 });
		expect(next[0].lumaBlack).toBe(0.3);
		expect(next[0].lumaWhite).toBe(1);
		expect(next[0].blend).toBe('normal');
	});

	it('does not mutate the array or the source objects', () => {
		const next = withSlotComposite(composites, 2, { colorKey: true });
		expect(next).not.toBe(composites);
		expect(next[2]).not.toBe(composites[2]);
		expect(composites[2].colorKey).toBe(false);
	});
});
