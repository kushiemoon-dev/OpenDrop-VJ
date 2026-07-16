import { describe, it, expect } from 'vitest';
import { shareSetState } from './share-set-store.svelte.js';

describe('share-set-store', () => {
	it('starts with an empty name, the default copy label, and no pending set', () => {
		expect(shareSetState.name).toBe('');
		expect(shareSetState.copyLabel).toBe('Copier le lien');
		expect(shareSetState.pending).toBeNull();
	});
});
