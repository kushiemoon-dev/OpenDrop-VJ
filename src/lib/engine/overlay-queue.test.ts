import { describe, it, expect } from 'vitest';
import {
	pickQueuedOverlays, advanceQueueIndex, retreatQueueIndex, clampQueueIndex, visibleOverlayIds,
} from './overlay-queue.js';
import { makeOverlay } from './overlay.js';

describe('pickQueuedOverlays', () => {
	it('filters checked overlays, order preserved', () => {
		const a = makeOverlay('a', { inQueue: true });
		const b = makeOverlay('b', { inQueue: false });
		const c = makeOverlay('c', { inQueue: true });
		expect(pickQueuedOverlays([a, b, c])).toEqual([a, c]);
	});

	it('empty list if none checked', () => {
		const a = makeOverlay('a', { inQueue: false });
		expect(pickQueuedOverlays([a])).toEqual([]);
	});
});

describe('advanceQueueIndex', () => {
	it('sequential mode: advances and loops', () => {
		expect(advanceQueueIndex(0, 3, 'sequential')).toBe(1);
		expect(advanceQueueIndex(2, 3, 'sequential')).toBe(0);
	});

	it('shuffle mode: never equal to current (multiple draws)', () => {
		for (let i = 0; i < 20; i++) {
			const next = advanceQueueIndex(0, 4, 'shuffle');
			expect(next).not.toBe(0);
			expect(next).toBeGreaterThanOrEqual(0);
			expect(next).toBeLessThan(4);
		}
	});

	it('queueLength 0 → 0', () => {
		expect(advanceQueueIndex(0, 0, 'sequential')).toBe(0);
		expect(advanceQueueIndex(0, 0, 'shuffle')).toBe(0);
	});

	it("queueLength 1 → stays 0 (shuffle doesn't loop indefinitely)", () => {
		expect(advanceQueueIndex(0, 1, 'shuffle')).toBe(0);
		expect(advanceQueueIndex(0, 1, 'sequential')).toBe(0);
	});
});

describe('retreatQueueIndex', () => {
	it('goes backward and loops, regardless of mode', () => {
		expect(retreatQueueIndex(0, 3)).toBe(2);
		expect(retreatQueueIndex(2, 3)).toBe(1);
	});

	it('queueLength 0 or 1 → 0', () => {
		expect(retreatQueueIndex(0, 0)).toBe(0);
		expect(retreatQueueIndex(0, 1)).toBe(0);
	});
});

describe('clampQueueIndex', () => {
	it('out-of-bounds index → 0', () => {
		expect(clampQueueIndex(5, 3)).toBe(0);
	});

	it('valid index → unchanged', () => {
		expect(clampQueueIndex(1, 3)).toBe(1);
	});

	it('negative index → 0', () => {
		expect(clampQueueIndex(-1, 3)).toBe(0);
	});

	it('queueLength 0 → 0', () => {
		expect(clampQueueIndex(0, 0)).toBe(0);
	});
});

describe('visibleOverlayIds', () => {
	it('0 checked → all unchecked ones are visible', () => {
		const a = makeOverlay('a', { inQueue: false });
		const b = makeOverlay('b', { inQueue: false });
		const ids = visibleOverlayIds([a, b], 0);
		expect(ids).toEqual(new Set([a.id, b.id]));
	});

	it('1 checked → always visible + the unchecked ones', () => {
		const a = makeOverlay('a', { inQueue: true });
		const b = makeOverlay('b', { inQueue: false });
		const ids = visibleOverlayIds([a, b], 0);
		expect(ids).toEqual(new Set([a.id, b.id]));
	});

	it('multiple checked → only the one at the active index + the unchecked ones', () => {
		const a = makeOverlay('a', { inQueue: true });
		const b = makeOverlay('b', { inQueue: true });
		const c = makeOverlay('c', { inQueue: false });
		const ids = visibleOverlayIds([a, b, c], 1);
		expect(ids).toEqual(new Set([b.id, c.id]));
	});

	it('out-of-bounds index → clean fallback (no crash, first checked one visible)', () => {
		const a = makeOverlay('a', { inQueue: true });
		const b = makeOverlay('b', { inQueue: true });
		const ids = visibleOverlayIds([a, b], 99);
		expect(ids).toEqual(new Set([a.id]));
	});
});
