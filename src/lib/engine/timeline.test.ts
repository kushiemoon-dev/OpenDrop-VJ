import { describe, it, expect } from 'vitest';
import { timelineLoopDuration, timelineValuesAt, type TimelineKeyframe } from './timeline.js';
import type { Snapshot } from './snapshot.js';
import type { CommandId } from './commands.js';

const A = 'color-hue-a' as CommandId;

function snap(value: number): Snapshot {
	return { name: 's', values: { [A]: value } };
}

describe('timelineLoopDuration', () => {
	it('fewer than 2 keyframes → 0', () => {
		expect(timelineLoopDuration([])).toBe(0);
		expect(timelineLoopDuration([{ slot: 0, timeSec: 5 }])).toBe(0);
	});
	it('returns the timestamp of the last keyframe', () => {
		const kfs: TimelineKeyframe[] = [
			{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 8 }, { slot: 2, timeSec: 20 },
		];
		expect(timelineLoopDuration(kfs)).toBe(20);
	});
});

describe('timelineValuesAt', () => {
	const snapshots: (Snapshot | null)[] = [snap(0), snap(1), null];

	it('fewer than 2 keyframes → {}', () => {
		expect(timelineValuesAt([], snapshots, 0)).toEqual({});
		expect(timelineValuesAt([{ slot: 0, timeSec: 0 }], snapshots, 0)).toEqual({});
	});

	it('at the first keyframe (t=0) → exact values of the first slot', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 10 }];
		expect(timelineValuesAt(kfs, snapshots, 0)).toEqual({ [A]: 0 });
	});

	it('at mid-segment → smoothstep interpolation (not linear)', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 10 }];
		const mid = timelineValuesAt(kfs, snapshots, 5);
		expect(mid[A]).toBeCloseTo(0.5); // smoothstep(0.5) = 0.5, exact midpoint
		const quarter = timelineValuesAt(kfs, snapshots, 2.5);
		expect(quarter[A]).not.toBeCloseTo(0.25, 2); // non-linear
	});

	it('just before the last keyframe → close to its value (never reached exactly, cut at wrap)', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 10 }];
		const out = timelineValuesAt(kfs, snapshots, 9.999);
		expect(out[A]).toBeCloseTo(1, 1);
	});

	it('3 keyframes → selects the correct segment', () => {
		const kfs: TimelineKeyframe[] = [
			{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 10 }, { slot: 0, timeSec: 20 },
		];
		expect(timelineValuesAt(kfs, snapshots, 15)[A]).toBeCloseTo(0.5); // midpoint of the 2nd segment (1→0)
	});

	it('empty slot referenced → treated as {} (existing interpolateSnapshot behavior)', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 0 }, { slot: 2, timeSec: 10 }]; // slot 2 = null
		expect(timelineValuesAt(kfs, snapshots, 5)).toEqual({});
	});

	it('tSec before the first keyframe → holds the value of the first one (smoothstep clamps)', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 5 }, { slot: 1, timeSec: 10 }];
		expect(timelineValuesAt(kfs, snapshots, 0)).toEqual({ [A]: 0 });
	});
});
