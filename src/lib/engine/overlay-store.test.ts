import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('./overlay.js', () => ({
	makeOverlay: vi.fn((name: string, partial: Record<string, unknown> = {}) => ({
		id: `id-${name}`,
		name,
		x: 0.5, y: 0.5, scale: 1, rotation: 0, opacity: 1, blendMode: 'screen',
		beatReactive: false, beatScale: 1.25, video: false, spin: 0, driftX: 0, driftY: 0,
		kind: 'media', text: '', fontFamily: 'sans', fontSize: 8, color: '#ffffff', inQueue: false,
		...partial,
	})),
	saveAsset: vi.fn(async () => {}),
	deleteAsset: vi.fn(async () => {}),
}));

import * as overlayApi from './overlay.js';
import {
	overlayState, addTextOverlay, addOverlayAtPosition, onOverlayFilePick,
	removeOverlay, updateOverlay, toggleOverlayQueue, setOverlayQueueMode,
	updateOverlayQueueTrigger, advanceOverlayQueue, onVisualizerDragOver,
} from './overlay-store.svelte.js';
import { defaultBeatTriggerConfig } from './beat-trigger.js';

function resetState() {
	overlayState.overlays = [];
	overlayState.dragOver = false;
	overlayState.queueEnabled = false;
	overlayState.queueIndex = 0;
	overlayState.queueTrigger = defaultBeatTriggerConfig();
	overlayState.queueMode = 'sequential';
}

describe('overlay-store', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		resetState();
	});

	describe('addTextOverlay', () => {
		it('ajoute un overlay texte et retourne son id', () => {
			const id = addTextOverlay();
			expect(overlayState.overlays).toHaveLength(1);
			expect(overlayState.overlays[0].kind).toBe('text');
			expect(overlayState.overlays[0].id).toBe(id);
		});
	});

	describe('addOverlayAtPosition', () => {
		it('sauvegarde l\'asset et ajoute un overlay positionné', async () => {
			await addOverlayAtPosition('photo', 'data:image/png;base64,xxx', 0.2, 0.8);
			expect(overlayApi.saveAsset).toHaveBeenCalledWith('id-photo', 'data:image/png;base64,xxx');
			expect(overlayState.overlays).toHaveLength(1);
			expect(overlayState.overlays[0].x).toBe(0.2);
			expect(overlayState.overlays[0].y).toBe(0.8);
		});
	});

	describe('onOverlayFilePick', () => {
		it("ne fait rien si aucun fichier n'est sélectionné", async () => {
			const input = { files: null, value: '' };
			await onOverlayFilePick({ target: input } as unknown as Event);
			expect(overlayApi.saveAsset).not.toHaveBeenCalled();
			expect(overlayState.overlays).toHaveLength(0);
		});

		it('lit chaque fichier sélectionné via FileReader et ajoute un overlay par fichier', async () => {
			class FakeFileReader {
				result: string | null = null;
				onload: (() => void) | null = null;
				readAsDataURL(_file: unknown) {
					this.result = 'data:image/png;base64,fake';
					this.onload?.();
				}
			}
			vi.stubGlobal('FileReader', FakeFileReader);

			const file1 = { name: 'a.png', type: 'image/png' };
			const file2 = { name: 'b.png', type: 'image/png' };
			const input = { files: [file1, file2], value: 'C:\\fakepath\\a.png' };
			await onOverlayFilePick({ target: input } as unknown as Event);

			expect(overlayState.overlays).toHaveLength(2);
			expect(input.value).toBe('');

			vi.unstubAllGlobals();
		});
	});

	describe('removeOverlay', () => {
		it('supprime l\'asset et retire l\'overlay de la liste', async () => {
			addTextOverlay();
			const id = overlayState.overlays[0].id;
			await removeOverlay(id);
			expect(overlayApi.deleteAsset).toHaveBeenCalledWith(id);
			expect(overlayState.overlays).toHaveLength(0);
		});

		it('re-clampe queueIndex si la file rétrécit', async () => {
			overlayState.overlays = [
				{ id: 'a', inQueue: true } as never,
				{ id: 'b', inQueue: true } as never,
			];
			overlayState.queueIndex = 1;
			await removeOverlay('b');
			expect(overlayState.queueIndex).toBe(0);
		});
	});

	describe('updateOverlay', () => {
		it('merge un patch sur le bon overlay uniquement', () => {
			addTextOverlay();
			const id = overlayState.overlays[0].id;
			updateOverlay(id, { opacity: 0.4 });
			expect(overlayState.overlays[0].opacity).toBe(0.4);
			expect(overlayState.overlays[0].text).toBe('Texte');
		});
	});

	describe('toggleOverlayQueue', () => {
		it('bascule enabled', () => {
			expect(overlayState.queueEnabled).toBe(false);
			toggleOverlayQueue();
			expect(overlayState.queueEnabled).toBe(true);
			toggleOverlayQueue();
			expect(overlayState.queueEnabled).toBe(false);
		});
	});

	describe('setOverlayQueueMode', () => {
		it('remplace le mode', () => {
			setOverlayQueueMode('shuffle');
			expect(overlayState.queueMode).toBe('shuffle');
		});
	});

	describe('updateOverlayQueueTrigger', () => {
		it('merge et re-clampe via applyBeatTriggerPatch', () => {
			updateOverlayQueueTrigger({ beatsPerChange: 100 });
			expect(overlayState.queueTrigger.beatsPerChange).toBe(64);
		});
	});

	describe('advanceOverlayQueue', () => {
		it('avance séquentiellement parmi les overlays en queue', () => {
			overlayState.overlays = [
				{ id: 'a', inQueue: true } as never,
				{ id: 'b', inQueue: true } as never,
				{ id: 'c', inQueue: false } as never,
			];
			overlayState.queueMode = 'sequential';
			overlayState.queueIndex = 0;
			advanceOverlayQueue(1);
			expect(overlayState.queueIndex).toBe(1);
		});

		it('recule sans tenir compte du mode', () => {
			overlayState.overlays = [
				{ id: 'a', inQueue: true } as never,
				{ id: 'b', inQueue: true } as never,
			];
			overlayState.queueIndex = 1;
			advanceOverlayQueue(-1);
			expect(overlayState.queueIndex).toBe(0);
		});
	});

	describe('onVisualizerDragOver', () => {
		it('active dragOver seulement si des fichiers sont traînés', () => {
			const withFiles = { dataTransfer: { types: ['Files'] }, preventDefault: vi.fn() };
			onVisualizerDragOver(withFiles as unknown as DragEvent);
			expect(overlayState.dragOver).toBe(true);
			expect(withFiles.preventDefault).toHaveBeenCalled();
		});

		it('ignore un dragover sans fichiers', () => {
			const withoutFiles = { dataTransfer: { types: ['text/plain'] }, preventDefault: vi.fn() };
			onVisualizerDragOver(withoutFiles as unknown as DragEvent);
			expect(overlayState.dragOver).toBe(false);
			expect(withoutFiles.preventDefault).not.toHaveBeenCalled();
		});
	});
});
