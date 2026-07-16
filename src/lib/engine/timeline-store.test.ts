import { describe, it, expect, beforeEach } from 'vitest';
import { snapshotsState } from './snapshots-store.svelte.js';
import {
	timelineState, toggleTimelinePlay, addTimelineKeyframe, removeTimelineKeyframe, updateTimelineKeyframe,
} from './timeline-store.svelte.js';

function resetState() {
	timelineState.keyframes = [];
	timelineState.playing = false;
	snapshotsState.snapshots = new Array(8).fill(null);
}

describe('timeline-store', () => {
	beforeEach(resetState);

	it('starts empty and not playing', () => {
		expect(timelineState.keyframes).toEqual([]);
		expect(timelineState.playing).toBe(false);
	});

	it('toggleTimelinePlay is a silent no-op with fewer than 2 keyframes', () => {
		addTimelineKeyframe();
		toggleTimelinePlay();
		expect(timelineState.playing).toBe(false);
	});

	it('toggleTimelinePlay starts and stops once there is a loop to play', () => {
		addTimelineKeyframe();
		addTimelineKeyframe();
		toggleTimelinePlay();
		expect(timelineState.playing).toBe(true);
		toggleTimelinePlay();
		expect(timelineState.playing).toBe(false);
	});

	it('addTimelineKeyframe defaults to slot 0 when no snapshot is filled, and starts at -5+5=0', () => {
		addTimelineKeyframe();
		expect(timelineState.keyframes).toEqual([{ slot: 0, timeSec: 0 }]);
	});

	it('addTimelineKeyframe uses the first filled snapshot slot and stacks 5s after the last keyframe', () => {
		snapshotsState.snapshots[3] = { name: 'Drop', values: {} };
		addTimelineKeyframe();
		addTimelineKeyframe();
		expect(timelineState.keyframes).toEqual([
			{ slot: 3, timeSec: 0 },
			{ slot: 3, timeSec: 5 },
		]);
	});

	it('removeTimelineKeyframe removes by index', () => {
		addTimelineKeyframe();
		addTimelineKeyframe();
		removeTimelineKeyframe(0);
		expect(timelineState.keyframes).toEqual([{ slot: 0, timeSec: 5 }]);
	});

	it('updateTimelineKeyframe patches a keyframe and keeps the list sorted by time', () => {
		addTimelineKeyframe();
		addTimelineKeyframe();
		updateTimelineKeyframe(1, { timeSec: -1 });
		expect(timelineState.keyframes).toEqual([
			{ slot: 0, timeSec: -1 },
			{ slot: 0, timeSec: 0 },
		]);
	});
});
