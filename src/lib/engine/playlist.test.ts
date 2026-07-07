import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { PlaylistEngine } from './playlist.js';

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { vi.useRealTimers(); });

describe('PlaylistEngine', () => {
	it('start() charge le preset courant immédiatement', () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb);
		pl.start();
		expect(cb).toHaveBeenCalledOnce();
		expect(cb).toHaveBeenCalledWith('A');
	});

	it('start() planifie le suivant après intervalMs', () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb);
		pl.start();
		cb.mockClear();
		vi.advanceTimersByTime(5000);
		expect(cb).toHaveBeenCalledWith('B');
	});

	it('cycle séquentiel tourne en boucle', () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B'], 'sequential', 1000, cb);
		pl.start();
		cb.mockClear();
		vi.advanceTimersByTime(1000); // B
		vi.advanceTimersByTime(1000); // A
		vi.advanceTimersByTime(1000); // B
		expect(cb.mock.calls.map((c) => c[0])).toEqual(['B', 'A', 'B']);
	});

	it('stop() arrête le cycle', () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 1000, cb);
		pl.start();
		pl.stop();
		cb.mockClear();
		vi.advanceTimersByTime(5000);
		expect(cb).not.toHaveBeenCalled();
	});

	it('next() avance manuellement', () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb);
		pl.next();
		expect(cb).toHaveBeenCalledWith('B');
	});

	it('prev() recule manuellement', () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb);
		pl.next(); // → B
		pl.prev(); // → A
		expect(cb).toHaveBeenLastCalledWith('A');
	});

	it('prev() depuis index 0 va au dernier', () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb);
		pl.prev();
		expect(cb).toHaveBeenCalledWith('C');
	});

	it("ne démarre pas si la liste est vide", () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine([], 'sequential', 1000, cb);
		pl.start();
		expect(pl.playing).toBe(false);
		expect(cb).not.toHaveBeenCalled();
	});

	it('setItems() réinitialise index si hors-bornes', () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 1000, cb);
		pl.next(); // idx=1
		pl.next(); // idx=2
		pl.setItems(['X']); // idx doit revenir à 0
		pl.start();
		expect(cb).toHaveBeenCalledWith('X');
	});

	it('setInterval() met à jour la durée du cycle', () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B'], 'sequential', 5000, cb);
		pl.setInterval(1000);
		pl.start();
		cb.mockClear();
		vi.advanceTimersByTime(1000);
		expect(cb).toHaveBeenCalledWith('B');
	});

	it("setInterval(Infinity) désactive l'avance automatique (mode beat-sync)", () => {
		const cb = vi.fn();
		const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', Infinity, cb);
		pl.start();
		cb.mockClear();
		vi.advanceTimersByTime(100000);
		expect(cb).not.toHaveBeenCalled();
	});

	it('playing reflète correctement start/stop', () => {
		const pl = new PlaylistEngine(['A', 'B'], 'sequential', 1000, vi.fn());
		expect(pl.playing).toBe(false);
		pl.start();
		expect(pl.playing).toBe(true);
		pl.stop();
		expect(pl.playing).toBe(false);
	});
});
