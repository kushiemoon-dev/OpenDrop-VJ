import { describe, it, expect, afterEach } from 'vitest';
import { MainSync } from './sync.js';

describe('MainSync.sendVideo', () => {
	let sync: MainSync | null = null;

	afterEach(() => {
		sync?.destroy();
		sync = null;
	});

	it('does not throw when clip is a reactive-proxied object (Svelte 5 $state shape)', () => {
		sync = new MainSync();
		// Simulates what `currentClip` ($derived, reading through `allClips`) actually
		// hands sendVideo in the app — a plain-shaped ClipRef wrapped in a Proxy, which
		// is what makes BroadcastChannel's real structured-clone algorithm throw
		// DataCloneError (confirmed directly against Node's BroadcastChannel — same
		// class of bug already fixed for sendOverlays/sendQVars/sendPoll in this file).
		const proxiedClip = new Proxy({ kind: 'builtin' as const, src: '/clip.mp4' }, {});
		expect(() => {
			sync!.sendVideo({
				enabled: true, clip: proxiedClip, opacity: 0.6,
				playbackRate: 1, flashOn: false, hueOn: false, separatorFlash: false,
			});
		}).not.toThrow();
	});

	it('handles clip: null (video disabled)', () => {
		sync = new MainSync();
		expect(() => {
			sync!.sendVideo({
				enabled: false, clip: null, opacity: 0.6,
				playbackRate: 1, flashOn: false, hueOn: false, separatorFlash: false,
			});
		}).not.toThrow();
	});
});
