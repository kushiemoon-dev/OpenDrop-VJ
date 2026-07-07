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
}

describe('video playback-store', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		resetState();
	});

	describe('addVideoFromFile', () => {
		it('ignore les fichiers de plus de 50 Mo', async () => {
			const bigFile = { size: 51 * 1024 * 1024, name: 'huge.mp4' } as unknown as File;
			await addVideoFromFile(bigFile);
			expect(videoStoreApi.saveVideo).not.toHaveBeenCalled();
			expect(videoState.userClips).toHaveLength(0);
		});

		it('sauvegarde le clip et l\'ajoute à userClips, active enabled si besoin', async () => {
			const file = { size: 1024, name: 'clip.mp4' } as unknown as File;
			await addVideoFromFile(file);
			expect(videoStoreApi.saveVideo).toHaveBeenCalled();
			expect(videoState.userClips).toHaveLength(1);
			expect(videoState.userClips[0].name).toBe('clip');
			expect(videoState.enabled).toBe(true);
		});

		it("ne force pas enabled à true s'il l'est déjà (pas de régression sur un désactivé manuel)", async () => {
			videoState.enabled = false;
			const file = { size: 1024, name: 'clip.mp4' } as unknown as File;
			await addVideoFromFile(file);
			expect(videoState.enabled).toBe(true); // premier clip: comportement voulu = auto-enable
		});
	});

	describe('onVideoFilePick', () => {
		it("ne fait rien si aucun fichier n'est sélectionné", async () => {
			const input = { files: null, value: '' };
			await onVideoFilePick({ target: input } as unknown as Event);
			expect(videoStoreApi.saveVideo).not.toHaveBeenCalled();
		});

		it('ajoute un clip par fichier sélectionné et vide value', async () => {
			const files = [{ size: 100, name: 'a.mp4' }, { size: 100, name: 'b.mp4' }];
			const input = { files, value: 'C:\\fakepath\\a.mp4' };
			await onVideoFilePick({ target: input } as unknown as Event);
			expect(videoState.userClips).toHaveLength(2);
			expect(input.value).toBe('');
		});
	});

	describe('removeVideoClip', () => {
		it('supprime un clip utilisateur et appelle deleteVideo', async () => {
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'clip1' }];
			await removeVideoClip(builtinClips.length); // premier index utilisateur
			expect(videoStoreApi.deleteVideo).toHaveBeenCalledWith('u1');
			expect(videoState.userClips).toHaveLength(0);
		});

		it('re-clampe currentClipIndex à 0 si l\'index courant sort des bornes après suppression', async () => {
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'clip1' }];
			videoState.currentClipIndex = builtinClips.length; // pointait sur le clip supprimé
			await removeVideoClip(builtinClips.length);
			expect(videoState.currentClipIndex).toBe(0);
		});
	});

	describe('onVideoBeat', () => {
		it('ne fait rien si video désactivée', () => {
			videoState.enabled = false;
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 20; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0);
		});

		it('ne fait rien en mode manuel', () => {
			videoState.enabled = true;
			videoState.advance = 'manual';
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 20; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0);
		});

		it('avance séquentiellement toutes les beatsPerCut beats', () => {
			videoState.enabled = true;
			videoState.advance = 'sequential';
			videoState.reactCut = true;
			videoState.beatsPerCut = 4;
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 3; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0); // pas encore atteint le seuil
			onVideoBeat();
			expect(videoState.currentClipIndex).toBe(1); // 4e beat -> avance
		});

		it('ignore le cut si reactCut est désactivé', () => {
			videoState.enabled = true;
			videoState.advance = 'sequential';
			videoState.reactCut = false;
			videoState.beatsPerCut = 1;
			videoState.userClips = [{ ref: { kind: 'user', id: 'u1' }, name: 'c1' }, { ref: { kind: 'user', id: 'u2' }, name: 'c2' }];
			for (let i = 0; i < 5; i++) onVideoBeat();
			expect(videoState.currentClipIndex).toBe(0);
		});
	});

	describe('onVideoAudioTick', () => {
		it('ramène playbackRate à 1 quand video désactivée ou warp off', () => {
			videoState.enabled = false;
			videoState.playbackRate = 1.8;
			onVideoAudioTick(0.9);
			expect(videoState.playbackRate).toBe(1);
		});

		it('fait tendre playbackRate vers 0.6 + bass*1.4 quand actif', () => {
			videoState.enabled = true;
			videoState.reactWarp = true;
			videoState.playbackRate = 1;
			onVideoAudioTick(1); // target = 2.0
			expect(videoState.playbackRate).toBeGreaterThan(1);
			expect(videoState.playbackRate).toBeLessThan(2);
		});
	});
});
