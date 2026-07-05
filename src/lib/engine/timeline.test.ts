import { describe, it, expect } from 'vitest';
import { timelineLoopDuration, timelineValuesAt, type TimelineKeyframe } from './timeline.js';
import type { Snapshot } from './snapshot.js';
import type { CommandId } from './commands.js';

const A = 'color-hue-a' as CommandId;

function snap(value: number): Snapshot {
	return { name: 's', values: { [A]: value } };
}

describe('timelineLoopDuration', () => {
	it('moins de 2 keyframes → 0', () => {
		expect(timelineLoopDuration([])).toBe(0);
		expect(timelineLoopDuration([{ slot: 0, timeSec: 5 }])).toBe(0);
	});
	it('retourne le timestamp du dernier keyframe', () => {
		const kfs: TimelineKeyframe[] = [
			{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 8 }, { slot: 2, timeSec: 20 },
		];
		expect(timelineLoopDuration(kfs)).toBe(20);
	});
});

describe('timelineValuesAt', () => {
	const snapshots: (Snapshot | null)[] = [snap(0), snap(1), null];

	it('moins de 2 keyframes → {}', () => {
		expect(timelineValuesAt([], snapshots, 0)).toEqual({});
		expect(timelineValuesAt([{ slot: 0, timeSec: 0 }], snapshots, 0)).toEqual({});
	});

	it('au premier keyframe (t=0) → valeurs exactes du premier slot', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 10 }];
		expect(timelineValuesAt(kfs, snapshots, 0)).toEqual({ [A]: 0 });
	});

	it('à mi-segment → interpolation smoothstep (pas linéaire)', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 10 }];
		const mid = timelineValuesAt(kfs, snapshots, 5);
		expect(mid[A]).toBeCloseTo(0.5); // smoothstep(0.5) = 0.5, milieu exact
		const quarter = timelineValuesAt(kfs, snapshots, 2.5);
		expect(quarter[A]).not.toBeCloseTo(0.25, 2); // non-linéaire
	});

	it('juste avant le dernier keyframe → proche de sa valeur (jamais atteint exactement, coupure au wrap)', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 10 }];
		const out = timelineValuesAt(kfs, snapshots, 9.999);
		expect(out[A]).toBeCloseTo(1, 1);
	});

	it('3 keyframes → sélectionne le bon segment', () => {
		const kfs: TimelineKeyframe[] = [
			{ slot: 0, timeSec: 0 }, { slot: 1, timeSec: 10 }, { slot: 0, timeSec: 20 },
		];
		expect(timelineValuesAt(kfs, snapshots, 15)[A]).toBeCloseTo(0.5); // milieu du 2e segment (1→0)
	});

	it('slot vide référencé → traité comme {} (comportement interpolateSnapshot existant)', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 0 }, { slot: 2, timeSec: 10 }]; // slot 2 = null
		expect(timelineValuesAt(kfs, snapshots, 5)).toEqual({});
	});

	it('tSec avant le premier keyframe → tient la valeur du premier (smoothstep clampe)', () => {
		const kfs: TimelineKeyframe[] = [{ slot: 0, timeSec: 5 }, { slot: 1, timeSec: 10 }];
		expect(timelineValuesAt(kfs, snapshots, 0)).toEqual({ [A]: 0 });
	});
});
