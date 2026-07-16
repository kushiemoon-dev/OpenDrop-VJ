import { describe, it, expect } from 'vitest';
import { deckState } from './deck-store.svelte.js';

describe('deck-store', () => {
	it('starts at slot 0, decks A/B empty, bus A/B/off/off, crossfader centered on A, 2s transition', () => {
		expect(deckState.activeSlot).toBe(0);
		expect(deckState.presetA).toBe('');
		expect(deckState.presetB).toBe('');
		expect(deckState.preset2).toBe('');
		expect(deckState.preset3).toBe('');
		expect(deckState.deckBus).toEqual(['A', 'B', 'off', 'off']);
		expect(deckState.crossfader).toBe(0);
		expect(deckState.transitionTime).toBe(2.0);
		expect(deckState.slotEpoch).toBe(0);
	});
});
