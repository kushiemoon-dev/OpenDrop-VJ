import { describe, it, expect } from 'vitest';
import { runStatusState } from './run-status-store.svelte.js';

describe('run-status-store', () => {
	it('starts idle with no error message', () => {
		expect(runStatusState.status).toBe('idle');
		expect(runStatusState.errorMsg).toBe('');
		expect(runStatusState.sourceError).toBe('');
	});
});
