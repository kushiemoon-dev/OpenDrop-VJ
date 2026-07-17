import { describe, it, expect } from 'vitest';
import { beginNdiTransition, endNdiTransition } from './ndi-deck-actions.js';

// Only the pure in-flight guard is unit tested here — startNdiDeck/stopNdiDeck
// themselves touch window.electronAPI/HTMLCanvasElement/requestAnimationFrame,
// browser boundaries this suite's `environment: 'node'` vitest config can't
// exercise (see this file's header comment). This guard is what closes the
// Task 3 review carryover: a second toggle arriving mid-transition must be a
// no-op instead of racing the first call's IPC round-trip.
describe('ndi-deck-actions in-flight guard', () => {
	it('allows a transition when the slot is idle', () => {
		expect(beginNdiTransition(0)).toBe(true);
		endNdiTransition(0);
	});

	it('rejects a second transition while one is already in flight for the same slot', () => {
		expect(beginNdiTransition(1)).toBe(true);
		expect(beginNdiTransition(1)).toBe(false);
		endNdiTransition(1);
	});

	it('allows a new transition once the in-flight one ends', () => {
		expect(beginNdiTransition(2)).toBe(true);
		endNdiTransition(2);
		expect(beginNdiTransition(2)).toBe(true);
		endNdiTransition(2);
	});

	it('tracks each slot independently', () => {
		expect(beginNdiTransition(3)).toBe(true);
		expect(beginNdiTransition(0)).toBe(true);
		endNdiTransition(3);
		endNdiTransition(0);
	});
});
