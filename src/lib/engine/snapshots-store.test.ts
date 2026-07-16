import { describe, it, expect, beforeEach } from 'vitest';
import { snapshotsState, setSnapshotValues, renameSnapshot, clearSnapshot } from './snapshots-store.svelte.js';

function resetState() {
	snapshotsState.snapshots = new Array(8).fill(null);
	snapshotsState.recallDuration = 2;
}

describe('snapshots-store', () => {
	beforeEach(resetState);

	it('starts with 8 empty slots and a 2s recall duration', () => {
		expect(snapshotsState.snapshots).toHaveLength(8);
		expect(snapshotsState.snapshots.every((s) => s === null)).toBe(true);
		expect(snapshotsState.recallDuration).toBe(2);
	});

	it('setSnapshotValues on an empty slot defaults the name to "Slot N"', () => {
		setSnapshotValues(3, { crossfader: 0.5 });
		expect(snapshotsState.snapshots[3]).toEqual({ name: 'Slot 3', values: { crossfader: 0.5 } });
	});

	it('setSnapshotValues on an existing slot keeps its custom name', () => {
		setSnapshotValues(1, { crossfader: 0.1 });
		renameSnapshot(1, 'Drop');
		setSnapshotValues(1, { crossfader: 0.9 });
		expect(snapshotsState.snapshots[1]).toEqual({ name: 'Drop', values: { crossfader: 0.9 } });
	});

	it('renameSnapshot is a no-op on an empty slot', () => {
		renameSnapshot(2, 'Ghost');
		expect(snapshotsState.snapshots[2]).toBeNull();
	});

	it('clearSnapshot resets a slot to null', () => {
		setSnapshotValues(0, { crossfader: 1 });
		clearSnapshot(0);
		expect(snapshotsState.snapshots[0]).toBeNull();
	});
});
