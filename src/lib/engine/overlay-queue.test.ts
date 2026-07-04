import { describe, it, expect } from 'vitest';
import {
	pickQueuedOverlays, advanceQueueIndex, retreatQueueIndex, clampQueueIndex, visibleOverlayIds,
} from './overlay-queue.js';
import { makeOverlay } from './overlay.js';

describe('pickQueuedOverlays', () => {
	it('filtre les overlays cochés, ordre préservé', () => {
		const a = makeOverlay('a', { inQueue: true });
		const b = makeOverlay('b', { inQueue: false });
		const c = makeOverlay('c', { inQueue: true });
		expect(pickQueuedOverlays([a, b, c])).toEqual([a, c]);
	});

	it('liste vide si aucun coché', () => {
		const a = makeOverlay('a', { inQueue: false });
		expect(pickQueuedOverlays([a])).toEqual([]);
	});
});

describe('advanceQueueIndex', () => {
	it('mode séquentiel : avance et boucle', () => {
		expect(advanceQueueIndex(0, 3, 'sequential')).toBe(1);
		expect(advanceQueueIndex(2, 3, 'sequential')).toBe(0);
	});

	it('mode shuffle : jamais égal au courant (plusieurs tirages)', () => {
		for (let i = 0; i < 20; i++) {
			const next = advanceQueueIndex(0, 4, 'shuffle');
			expect(next).not.toBe(0);
			expect(next).toBeGreaterThanOrEqual(0);
			expect(next).toBeLessThan(4);
		}
	});

	it('queueLength 0 → 0', () => {
		expect(advanceQueueIndex(0, 0, 'sequential')).toBe(0);
		expect(advanceQueueIndex(0, 0, 'shuffle')).toBe(0);
	});

	it('queueLength 1 → reste 0 (shuffle ne boucle pas indéfiniment)', () => {
		expect(advanceQueueIndex(0, 1, 'shuffle')).toBe(0);
		expect(advanceQueueIndex(0, 1, 'sequential')).toBe(0);
	});
});

describe('retreatQueueIndex', () => {
	it('recule et boucle, indépendamment du mode', () => {
		expect(retreatQueueIndex(0, 3)).toBe(2);
		expect(retreatQueueIndex(2, 3)).toBe(1);
	});

	it('queueLength 0 ou 1 → 0', () => {
		expect(retreatQueueIndex(0, 0)).toBe(0);
		expect(retreatQueueIndex(0, 1)).toBe(0);
	});
});

describe('clampQueueIndex', () => {
	it('index hors-bornes → 0', () => {
		expect(clampQueueIndex(5, 3)).toBe(0);
	});

	it('index valide → inchangé', () => {
		expect(clampQueueIndex(1, 3)).toBe(1);
	});

	it('index négatif → 0', () => {
		expect(clampQueueIndex(-1, 3)).toBe(0);
	});

	it('queueLength 0 → 0', () => {
		expect(clampQueueIndex(0, 0)).toBe(0);
	});
});

describe('visibleOverlayIds', () => {
	it('0 coché → tous les non-cochés visibles', () => {
		const a = makeOverlay('a', { inQueue: false });
		const b = makeOverlay('b', { inQueue: false });
		const ids = visibleOverlayIds([a, b], 0);
		expect(ids).toEqual(new Set([a.id, b.id]));
	});

	it('1 coché → toujours visible + les non-cochés', () => {
		const a = makeOverlay('a', { inQueue: true });
		const b = makeOverlay('b', { inQueue: false });
		const ids = visibleOverlayIds([a, b], 0);
		expect(ids).toEqual(new Set([a.id, b.id]));
	});

	it('plusieurs cochés → seul celui à l\'index actif + les non-cochés', () => {
		const a = makeOverlay('a', { inQueue: true });
		const b = makeOverlay('b', { inQueue: true });
		const c = makeOverlay('c', { inQueue: false });
		const ids = visibleOverlayIds([a, b, c], 1);
		expect(ids).toEqual(new Set([b.id, c.id]));
	});

	it('index hors-bornes → fallback propre (pas de crash, premier coché visible)', () => {
		const a = makeOverlay('a', { inQueue: true });
		const b = makeOverlay('b', { inQueue: true });
		const ids = visibleOverlayIds([a, b], 99);
		expect(ids).toEqual(new Set([a.id]));
	});
});
