/**
 * overlay-store.svelte.ts — reactive wrapper around overlay CRUD and the
 * overlay auto-cycle queue. Extracted from +page.svelte. Singleton module,
 * same shape as cloud-presets-store.svelte.ts — mutate the exported state
 * object's fields, never reassign the export.
 *
 * `beat` (the shared beat-flash pulse also consumed by the video layer) and
 * the drag/drop file-type dispatch (video clips vs image overlays) stay in
 * +page.svelte — they're cross-cutting orchestration, not overlay-only state.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import { makeOverlay, saveAsset, deleteAsset, type Overlay } from './overlay.js';
import { pickQueuedOverlays, advanceQueueIndex, retreatQueueIndex, clampQueueIndex } from './overlay-queue.js';
import { type BeatTriggerConfig, defaultBeatTriggerConfig, applyBeatTriggerPatch } from './beat-trigger.js';
import type { PlaylistMode } from './playlist.js';

export const overlayState = $state({
	overlays: [] as Overlay[],
	dragOver: false,
	queueEnabled: false,
	queueIndex: 0,
	queueTrigger: defaultBeatTriggerConfig() as BeatTriggerConfig,
	queueMode: 'sequential' as PlaylistMode,
});

async function addOverlayFromFile(file: File): Promise<void> {
	return new Promise<void>((resolve) => {
		const reader = new FileReader();
		reader.onload = async () => {
			const dataUrl = reader.result as string;
			const ov = makeOverlay(file.name.replace(/\.[^.]+$/, ''), { video: file.type.startsWith('video/') });
			await saveAsset(ov.id, dataUrl);
			overlayState.overlays = [...overlayState.overlays, ov];
			resolve();
		};
		reader.readAsDataURL(file);
	});
}

export async function onOverlayFilePick(e: Event): Promise<void> {
	const files = (e.target as HTMLInputElement).files;
	if (!files) return;
	for (const f of Array.from(files)) await addOverlayFromFile(f);
	(e.target as HTMLInputElement).value = '';
}

export function addTextOverlay(): string {
	const ov = makeOverlay('Texte', { kind: 'text', text: 'Texte' });
	overlayState.overlays = [...overlayState.overlays, ov];
	return ov.id;
}

/** Save an image dropped at a specific normalized position (visualizer-wrap drop handler). */
export async function addOverlayAtPosition(name: string, dataUrl: string, x: number, y: number): Promise<void> {
	const ov = makeOverlay(name, { x, y });
	await saveAsset(ov.id, dataUrl);
	overlayState.overlays = [...overlayState.overlays, ov];
}

export function onVisualizerDragOver(e: DragEvent): void {
	if (!e.dataTransfer?.types.includes('Files')) return;
	e.preventDefault();
	overlayState.dragOver = true;
}

export async function removeOverlay(id: string): Promise<void> {
	await deleteAsset(id);
	overlayState.overlays = overlayState.overlays.filter(o => o.id !== id);
	overlayState.queueIndex = clampQueueIndex(overlayState.queueIndex, pickQueuedOverlays(overlayState.overlays).length);
}

export function updateOverlay(id: string, patch: Partial<Overlay>): void {
	overlayState.overlays = overlayState.overlays.map(o => o.id === id ? { ...o, ...patch } : o);
}

export function toggleOverlayQueue(): void {
	overlayState.queueEnabled = !overlayState.queueEnabled;
}

export function setOverlayQueueMode(mode: PlaylistMode): void {
	overlayState.queueMode = mode;
}

export function updateOverlayQueueTrigger(patch: Partial<BeatTriggerConfig>): void {
	overlayState.queueTrigger = applyBeatTriggerPatch(overlayState.queueTrigger, patch);
}

export function advanceOverlayQueue(direction: 1 | -1): void {
	const queued = pickQueuedOverlays(overlayState.overlays);
	overlayState.queueIndex = direction === 1
		? advanceQueueIndex(overlayState.queueIndex, queued.length, overlayState.queueMode)
		: retreatQueueIndex(overlayState.queueIndex, queued.length);
}
