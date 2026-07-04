import type { Overlay } from './overlay.js';
import type { PlaylistMode } from './playlist.js';

export function pickQueuedOverlays(overlays: Overlay[]): Overlay[] {
	return overlays.filter((o) => o.inQueue);
}

// "next" — respecte le mode (séquentiel ou aléatoire-différent-du-courant), extrait du couple
// next()/_randomIndex() privé de PlaylistEngine.
export function advanceQueueIndex(currentIndex: number, queueLength: number, mode: PlaylistMode): number {
	if (queueLength === 0) return 0;
	if (mode === 'sequential') return (currentIndex + 1) % queueLength;
	if (queueLength === 1) return 0;
	let idx: number;
	do { idx = Math.floor(Math.random() * queueLength); } while (idx === currentIndex);
	return idx;
}

// "prev" — toujours séquentiel-arrière quel que soit le mode, même comportement que
// PlaylistEngine.prev() aujourd'hui (qui ignore déjà le mode shuffle pour le "précédent").
export function retreatQueueIndex(currentIndex: number, queueLength: number): number {
	if (queueLength === 0) return 0;
	return (currentIndex - 1 + queueLength) % queueLength;
}

// Défensif : si la file a rétréci (overlay actif supprimé), retombe sur 0 plutôt que de
// pointer hors-bornes — même pattern que PlaylistEngine.setItems().
export function clampQueueIndex(index: number, queueLength: number): number {
	if (queueLength === 0) return 0;
	return index >= queueLength || index < 0 ? 0 : index;
}

// Overlays à rendre : tous les non-cochés (toujours visibles) + l'overlay actif de la
// rotation (s'il y en a au moins un coché).
export function visibleOverlayIds(overlays: Overlay[], activeQueueIndex: number): Set<string> {
	const queued = pickQueuedOverlays(overlays);
	const ids = new Set(overlays.filter((o) => !o.inQueue).map((o) => o.id));
	if (queued.length > 0) {
		const idx = clampQueueIndex(activeQueueIndex, queued.length);
		ids.add(queued[idx].id);
	}
	return ids;
}
