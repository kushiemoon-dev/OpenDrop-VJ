import { describe, it, expect } from 'vitest';
import { DEFAULT_TIER, DEFAULT_PERF } from './quality.js';
import { perfState } from './perf-store.svelte.js';

describe('perf-store', () => {
	it('starts at the default quality tier and perf settings', () => {
		expect(perfState.quality).toBe(DEFAULT_TIER);
		expect(perfState.targetFps).toBe(DEFAULT_PERF.targetFps);
		expect(perfState.invisibleMode).toBe(DEFAULT_PERF.invisibleMode);
		expect(perfState.invisibleFps).toBe(DEFAULT_PERF.invisibleFps);
	});
});
