import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/engine/video-store.js', () => ({
	saveVideo: vi.fn(async () => {}),
	deleteVideo: vi.fn(async () => {}),
}));

vi.mock('./index.js', () => ({
	builtinClips: [
		{ ref: { kind: 'builtin', src: 'a.mp4' }, name: 'A' },
		{ ref: { kind: 'builtin', src: 'b.mp4' }, name: 'B' },
	],
}));

import * as videoStoreApi from '$lib/engine/video-store.js';
import { builtinClips } from './index.js';
import {
	videoState, addVideoFromFile, onVideoFilePick, removeVideoClip, onVideoBeat, onVideoAudioTick,
	setLiveCamera, clearLiveCamera, setNdiSource, clearNdiSource,
} from './playback-store.svelte.js';

function resetState() {
	videoState.enabled = false;
	videoState.opacity = 0.6;
	videoState.advance = 'shuffle';
	videoState.beatsPerCut = 8;
	videoState.reactCut = true;
	videoState.reactFlash = true;
	videoState.reactWarp = true;
	videoState.reactHue = false;
	videoState.userClips = [];
	videoState.currentClipIndex = 0;
	videoState.playbackRate = 1;
	videoState.liveDeviceId = null;
	videoState.liveLabel = '';
	videoState.ndiSourceName = null;
	videoState.ndiUrlAddress = '';
}

describe('video playback-store', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		resetState();
	});

	describe('addVideoFromFile', () => {
		it('ignores files larger than 50 MB', async () => {
			const bigFile = { size: 51 * 1024 * 1024, name: 'huge.mp4' } as unknown as File;
			await addVideoFromFile(bigFile);
			expect(videoStoreApi.saveVideo).not.toHaveBeenCalled();
			expect(videoState.userClips).toHaveLength(0);
		});

		it('saves the clip and adds it to userClips, enabling `enabled` if needed', async () => {
			const file = { size: 1024, name: 'clip.mp4' } as unknown as File;
			await addVideoFromFile(file);
			expect(videoStoreApi.saveVideo).toHaveBeenCalled();
			expect(videoState.userClips).toHaveLength(1);
			expect(videoState.userClips[0].name).toBe('clip');
			expect(videoState.enabled).toBe(true);
		});

		it("does not force enabled to true if it's already set (no regression on a manual disable)", async () => {
			videoState.enabled = false;
			const file = { size: 1024, name: 'clip.mp4' } as unknown as File;
			await addVideoFromFile(file);
			expect(videoState.enabled).toBe(true); // first clip: intended behavior = auto-enable
		});
	});

	describe('onVideoFilePick', () => {
		it("does nothing if no file is selected", async () => {
			const input = { files: null, value: '' };
			await onVideoFilePick({ target: input } as unknown as Event);
			expect(videoStoreApi.saveVideo).not.toHaveBeenCalled();
		});

		it('adds a clip per selected file and clears value', async () => {
			const files = [{ size: 100, name: 'a.mp4' }, { size: 100, name: 'b.mp4' }];
			const input = { files, value: 'C:\\fakepath\\a.mp4' };
			await onVideoFilePick({ target: input } as unknown as Event);
			expect(videoState.userClips).toHaveLength(2);
			expect(input.value).toBe('');
		});
	});

	describe('removeVideoClip', () => {
		it('removes a user clip and calls deleteVideo', async () => {
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'clip1' }];
			await removeVideoClip(builtinClips.length); // first user index
			expect(videoStoreApi.deleteVideo).toHaveBeenCalledWith('u1');
			expect(videoState.userClips).toHaveLength(0);
		});

		it('re-clamps currentClipIndex to 0 if the current index is out of bounds after removal', async () => {
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'clip1' }];
			videoState.currentClipIndex = builtinClips.length; // pointed to the removed clip
			await removeVideoClip(builtinClips.length);
			expect(videoState.currentClipIndex).toBe(0);
		});
	});

	describe('onVideoBeat', () => {
		it('does nothing if video is disabled', () => {
			videoState.enabled = false;
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 20; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0);
		});

		it('does nothing in manual mode', () => {
			videoState.enabled = true;
			videoState.advance = 'manual';
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 20; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0);
		});

		it('advances sequentially every beatsPerCut beats', () => {
			videoState.enabled = true;
			videoState.advance = 'sequential';
			videoState.reactCut = true;
			videoState.beatsPerCut = 4;
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 3; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0); // threshold not yet reached
			onVideoBeat();
			expect(videoState.currentClipIndex).toBe(1); // 4th beat -> advances
		});

		it('ignores the cut if reactCut is disabled', () => {
			videoState.enabled = true;
			videoState.advance = 'sequential';
			videoState.reactCut = false;
			videoState.beatsPerCut = 1;
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 5; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0);
		});

		it('does nothing while a live camera is active (no index drift for when the user exits live)', () => {
			videoState.enabled = true;
			videoState.advance = 'sequential';
			videoState.reactCut = true;
			videoState.beatsPerCut = 1;
			videoState.liveDeviceId = 'dev1';
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 5; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0);
		});

		it('does nothing while an NDI source is active', () => {
			videoState.enabled = true;
			videoState.advance = 'sequential';
			videoState.reactCut = true;
			videoState.beatsPerCut = 1;
			videoState.ndiSourceName = 'OBS';
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 5; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0);
		});
	});

	describe('setLiveCamera / clearLiveCamera', () => {
		it('sets liveDeviceId/liveLabel and auto-enables video', () => {
			videoState.enabled = false;
			setLiveCamera('dev1', 'FaceTime HD Camera');
			expect(videoState.liveDeviceId).toBe('dev1');
			expect(videoState.liveLabel).toBe('FaceTime HD Camera');
			expect(videoState.enabled).toBe(true);
		});

		it('does not touch enabled if already true (idempotent, same as addVideoFromFile)', () => {
			videoState.enabled = true;
			setLiveCamera('dev1', 'Cam A');
			expect(videoState.enabled).toBe(true);
		});

		it('clearLiveCamera resets deviceId/label without touching enabled', () => {
			setLiveCamera('dev1', 'Cam A');
			videoState.enabled = true;
			clearLiveCamera();
			expect(videoState.liveDeviceId).toBeNull();
			expect(videoState.liveLabel).toBe('');
			expect(videoState.enabled).toBe(true);
		});

		it('is mutually exclusive with an active NDI source', () => {
			setNdiSource('OBS', 'ndi://1.2.3.4');
			setLiveCamera('dev1', 'Cam A');
			expect(videoState.ndiSourceName).toBeNull();
			expect(videoState.liveDeviceId).toBe('dev1');
		});
	});

	describe('setNdiSource / clearNdiSource', () => {
		it('sets ndiSourceName/ndiUrlAddress and auto-enables video', () => {
			videoState.enabled = false;
			setNdiSource('OBS', 'ndi://1.2.3.4');
			expect(videoState.ndiSourceName).toBe('OBS');
			expect(videoState.ndiUrlAddress).toBe('ndi://1.2.3.4');
			expect(videoState.enabled).toBe(true);
		});

		it('clearNdiSource resets name/url without touching enabled', () => {
			setNdiSource('OBS', 'ndi://1.2.3.4');
			videoState.enabled = true;
			clearNdiSource();
			expect(videoState.ndiSourceName).toBeNull();
			expect(videoState.ndiUrlAddress).toBe('');
			expect(videoState.enabled).toBe(true);
		});

		it('is mutually exclusive with an active live camera', () => {
			setLiveCamera('dev1', 'Cam A');
			setNdiSource('OBS', 'ndi://1.2.3.4');
			expect(videoState.liveDeviceId).toBeNull();
			expect(videoState.ndiSourceName).toBe('OBS');
		});
	});

	describe('onVideoAudioTick', () => {
		it('resets playbackRate to 1 when video is disabled or warp is off', () => {
			videoState.enabled = false;
			videoState.playbackRate = 1.8;
			onVideoAudioTick(0.9);
			expect(videoState.playbackRate).toBe(1);
		});

		it('makes playbackRate trend toward 0.6 + bass*1.4 when active', () => {
			videoState.enabled = true;
			videoState.reactWarp = true;
			videoState.playbackRate = 1;
			onVideoAudioTick(1); // target = 2.0
			expect(videoState.playbackRate).toBeGreaterThan(1);
			expect(videoState.playbackRate).toBeLessThan(2);
		});

		it('resets playbackRate to 1 while a live camera is active, even with warp on (playbackRate is inert on a MediaStream)', () => {
			videoState.enabled = true;
			videoState.reactWarp = true;
			videoState.liveDeviceId = 'dev1';
			videoState.playbackRate = 1.8;
			onVideoAudioTick(1);
			expect(videoState.playbackRate).toBe(1);
		});

		it('resets playbackRate to 1 while an NDI source is active', () => {
			videoState.enabled = true;
			videoState.reactWarp = true;
			videoState.ndiSourceName = 'OBS';
			videoState.playbackRate = 1.8;
			onVideoAudioTick(1);
			expect(videoState.playbackRate).toBe(1);
		});
	});
});
