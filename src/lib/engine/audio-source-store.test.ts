import { describe, it, expect } from 'vitest';
import { audioSourceState } from './audio-source-store.svelte.js';

describe('audio-source-store', () => {
	it('starts with no device selected, no devices listed, and BPM at 0', () => {
		expect(audioSourceState.currentDeviceId).toBe('');
		expect(audioSourceState.currentLoopbackDeviceId).toBe(0);
		expect(audioSourceState.devices).toEqual([]);
		expect(audioSourceState.manualBpm).toBe(0);
		expect(audioSourceState.detectedBpm).toBe(0);
	});
});
