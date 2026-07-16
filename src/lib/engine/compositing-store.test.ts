import { describe, it, expect, beforeEach } from 'vitest';
import { DEFAULT_SLOT_COMPOSITE } from './sync.js';
import { compositingState, updateComposite } from './compositing-store.svelte.js';

function resetState() {
	compositingState.slotComposites = [
		{ ...DEFAULT_SLOT_COMPOSITE }, { ...DEFAULT_SLOT_COMPOSITE }, { ...DEFAULT_SLOT_COMPOSITE }, { ...DEFAULT_SLOT_COMPOSITE },
	];
}

describe('compositing-store', () => {
	beforeEach(resetState);

	it('starts with 4 slots of default composite config', () => {
		expect(compositingState.slotComposites).toHaveLength(4);
		expect(compositingState.slotComposites[0]).toEqual(DEFAULT_SLOT_COMPOSITE);
	});

	it('updates one slot without touching the others', () => {
		updateComposite(2, { blend: 'additive' });
		expect(compositingState.slotComposites[2].blend).toBe('additive');
		expect(compositingState.slotComposites[0].blend).toBe(DEFAULT_SLOT_COMPOSITE.blend);
		expect(compositingState.slotComposites[1].blend).toBe(DEFAULT_SLOT_COMPOSITE.blend);
	});
});
