import type { Overlay } from './overlay.js';
import type { PlaylistMode } from './playlist.js';

export function pickQueuedOverlays(overlays: Overlay[]): Overlay[] {
	return overlays.filter((o) => o.inQueue);
}

// "next" — respects the mode (sequential or random-different-from-current), extracted from the
// next()/_randomIndex() pair that used to be private to PlaylistEngine.
export function advanceQueueIndex(currentIndex: number, queueLength: number, mode: PlaylistMode): number {
	if (queueLength === 0) return 0;
	if (mode === 'sequential') return (currentIndex + 1) % queueLength;
	if (queueLength === 1) return 0;
	let idx: number;
	do { idx = Math.floor(Math.random() * queueLength); } while (idx === currentIndex);
	return idx;
}

// "prev" — always steps backward sequentially regardless of mode, same behavior as
// PlaylistEngine.prev() today (which already ignores shuffle mode for "previous").
export function retreatQueueIndex(currentIndex: number, queueLength: number): number {
	if (queueLength === 0) return 0;
	return (currentIndex - 1 + queueLength) % queueLength;
}

// Defensive: if the queue has shrunk (active overlay removed), falls back to 0 instead of
// pointing out of bounds — same pattern as PlaylistEngine.setItems().
export function clampQueueIndex(index: number, queueLength: number): number {
	if (queueLength === 0) return 0;
	return index >= queueLength || index < 0 ? 0 : index;
}

// Overlays to render: all non-queued ones (always visible) + the active overlay from the
// rotation (if at least one is queued).
export function visibleOverlayIds(overlays: Overlay[], activeQueueIndex: number): Set<string> {
	const queued = pickQueuedOverlays(overlays);
	const ids = new Set(overlays.filter((o) => !o.inQueue).map((o) => o.id));
	if (queued.length > 0) {
		const idx = clampQueueIndex(activeQueueIndex, queued.length);
		ids.add(queued[idx].id);
	}
	return ids;
}
