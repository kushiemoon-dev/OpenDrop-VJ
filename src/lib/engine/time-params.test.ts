import { describe, it, expect } from 'vitest';
import { defaultTimeParams, injectTimeParams, withTimeParams, type TimeParamsTuple } from './time-params.js';

describe('defaultTimeParams', () => {
	it('all multipliers equal 1 (neutral)', () => {
		expect(defaultTimeParams()).toEqual({
			speedMult: 1, zoomMult: 1, rotMult: 1, warpMult: 1,
			dxMult: 1, dyMult: 1, stretchMult: 1, waveMult: 1,
		});
	});
});

describe('injectTimeParams', () => {
	it('clones the preset without mutating the original', () => {
		const preset = { frame_eqs_str: 'a.zoom = 1.01;', other: 'field' };
		const patched = injectTimeParams(preset, 0);
		expect(preset.frame_eqs_str).toBe('a.zoom = 1.01;'); // unchanged
		expect(patched).not.toBe(preset); // different object
		expect((patched as any).other).toBe('field'); // untouched fields preserved
	});

	it('prefixes scaled a.time, before the preset\'s original code', () => {
		const preset = { frame_eqs_str: 'a.zoom = 1.01;' };
		const patched = injectTimeParams(preset, 0) as any;
		const speedLineIndex = patched.frame_eqs_str.indexOf('a.time = a.time *');
		const originalLineIndex = patched.frame_eqs_str.indexOf('a.zoom = 1.01;');
		expect(speedLineIndex).toBeGreaterThanOrEqual(0);
		expect(speedLineIndex).toBeLessThan(originalLineIndex);
	});

	it('adds the 7 multiplier lines after the original code, referencing window.__odDeckParams[slot]', () => {
		const preset = { frame_eqs_str: 'a.zoom = 1.01;' };
		const patched = injectTimeParams(preset, 2) as any;
		for (const field of ['zoomMult', 'rotMult', 'warpMult', 'dxMult', 'dyMult', 'stretchMult', 'waveMult']) {
			expect(patched.frame_eqs_str).toContain(`window.__odDeckParams[2].${field}`);
		}
	});

	it('namespaces correctly per slot (no collision between decks)', () => {
		const preset = { frame_eqs_str: '' };
		const patched0 = injectTimeParams(preset, 0) as any;
		const patched3 = injectTimeParams(preset, 3) as any;
		expect(patched0.frame_eqs_str).toContain('window.__odDeckParams[0]');
		expect(patched0.frame_eqs_str).not.toContain('window.__odDeckParams[3]');
		expect(patched3.frame_eqs_str).toContain('window.__odDeckParams[3]');
		expect(patched3.frame_eqs_str).not.toContain('window.__odDeckParams[0]');
	});

	it('handles a preset without frame_eqs_str (empty string by default)', () => {
		const preset = {};
		const patched = injectTimeParams(preset, 0) as any;
		expect(patched.frame_eqs_str).toContain('a.time = a.time *');
	});
});

describe('withTimeParams', () => {
	const params: TimeParamsTuple = [defaultTimeParams(), defaultTimeParams(), defaultTimeParams(), defaultTimeParams()];

	it('updates only the targeted slot', () => {
		const next = withTimeParams(params, 1, { speedMult: 1.5 });
		expect(next[1].speedMult).toBe(1.5);
		expect(next[0].speedMult).toBe(1);
		expect(next[2].speedMult).toBe(1);
	});

	it('merges a partial patch without touching other fields', () => {
		const next = withTimeParams(params, 0, { zoomMult: 2 });
		expect(next[0].zoomMult).toBe(2);
		expect(next[0].speedMult).toBe(1);
	});

	it('does not mutate the array or the source objects', () => {
		const next = withTimeParams(params, 3, { rotMult: 0.5 });
		expect(next).not.toBe(params);
		expect(next[3]).not.toBe(params[3]);
		expect(params[3].rotMult).toBe(1);
	});
});
