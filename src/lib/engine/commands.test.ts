import { describe, it, expect, vi } from 'vitest';
import { createDefaultRegistry, CommandRegistry, type CommandContext, type Command } from './commands.js';

function makeCtx(overrides: Partial<CommandContext> = {}): CommandContext & { crossfader: number; activeDeck: 'A' | 'B' } {
	const state = { crossfader: 0.5, activeDeck: 'A' as 'A' | 'B' };
	return {
		getCrossfader: () => state.crossfader,
		setCrossfader: (v) => { state.crossfader = v; },
		getActiveDeck: () => state.activeDeck,
		switchActiveDeck: () => { state.activeDeck = state.activeDeck === 'A' ? 'B' : 'A'; },
		navigatePreset: vi.fn(),
		togglePlaylist: vi.fn(),
		playlistNext: vi.fn(),
		playlistPrev: vi.fn(),
		...overrides,
		crossfader: state.crossfader,
		activeDeck: state.activeDeck,
	} as CommandContext & { crossfader: number; activeDeck: 'A' | 'B' };
}

describe('CommandRegistry', () => {
	it('register + get', () => {
		const reg = new CommandRegistry();
		const cmd: Command = { id: 'crossfader', label: 'X', kind: 'range', run: vi.fn() };
		reg.register(cmd);
		expect(reg.get('crossfader')).toBe(cmd);
	});

	it('dispatch appelle run avec la bonne valeur', () => {
		const reg = new CommandRegistry();
		const run = vi.fn();
		reg.register({ id: 'crossfader', label: 'X', kind: 'range', run });
		const ctx = makeCtx();
		reg.dispatch('crossfader', 0.75, ctx);
		expect(run).toHaveBeenCalledWith(0.75, ctx);
	});

	it('dispatch sur une id inconnue ne plante pas', () => {
		const reg = new CommandRegistry();
		const ctx = makeCtx();
		expect(() => reg.dispatch('strobe-toggle', 1, ctx)).not.toThrow();
	});

	it('all() retourne toutes les commandes', () => {
		const reg = new CommandRegistry();
		reg.register({ id: 'crossfader', label: 'X', kind: 'range', run: vi.fn() });
		reg.register({ id: 'deck-switch', label: 'Y', kind: 'trigger', run: vi.fn() });
		expect(reg.all()).toHaveLength(2);
	});
});

describe('createDefaultRegistry — commandes de base', () => {
	const reg = createDefaultRegistry();

	it('contient les 11 commandes MIDI héritées', () => {
		const ids = [
			'crossfader',
			'preset-prev-a', 'preset-next-a', 'preset-prev-b', 'preset-next-b',
			'playlist-toggle-a', 'playlist-toggle-b',
			'playlist-prev-a', 'playlist-next-a', 'playlist-prev-b', 'playlist-next-b',
		] as const;
		for (const id of ids) {
			expect(reg.get(id), `missing: ${id}`).toBeDefined();
		}
	});

	it('crossfader (range) → setCrossfader avec la valeur', () => {
		const ctx = makeCtx();
		reg.dispatch('crossfader', 0.3, ctx);
		expect(ctx.getCrossfader()).toBe(0.3);
	});

	it('preset-next-a → navigatePreset(A, 1)', () => {
		const ctx = makeCtx();
		reg.dispatch('preset-next-a', 1, ctx);
		expect(ctx.navigatePreset).toHaveBeenCalledWith('A', 1);
	});

	it('preset-prev-b → navigatePreset(B, -1)', () => {
		const ctx = makeCtx();
		reg.dispatch('preset-prev-b', 1, ctx);
		expect(ctx.navigatePreset).toHaveBeenCalledWith('B', -1);
	});

	it('playlist-toggle-a → togglePlaylist(A)', () => {
		const ctx = makeCtx();
		reg.dispatch('playlist-toggle-a', 1, ctx);
		expect(ctx.togglePlaylist).toHaveBeenCalledWith('A');
	});

	it('playlist-next-b → playlistNext(B)', () => {
		const ctx = makeCtx();
		reg.dispatch('playlist-next-b', 1, ctx);
		expect(ctx.playlistNext).toHaveBeenCalledWith('B');
	});
});

describe('createDefaultRegistry — commandes de compositing (1.1)', () => {
	const reg = createDefaultRegistry();

	it('contient les 20 commandes composite/lumakey/colorkey pour les 4 slots', () => {
		const prefixes = ['composite-blend', 'lumakey-black', 'lumakey-white', 'colorkey-hue', 'colorkey-tolerance'] as const;
		for (const prefix of prefixes) {
			for (const slot of [0, 1, 2, 3] as const) {
				const id = `${prefix}-${slot}` as const;
				expect(reg.get(id), `missing: ${id}`).toBeDefined();
				expect(reg.get(id)?.kind).toBe('range');
			}
		}
	});
});

describe('createDefaultRegistry — recall snapshots (1.3)', () => {
	const reg = createDefaultRegistry();

	it('contient les 8 triggers recall-snapshot-0..7', () => {
		for (const slot of [0, 1, 2, 3, 4, 5, 6, 7] as const) {
			const id = `recall-snapshot-${slot}` as const;
			expect(reg.get(id), `missing: ${id}`).toBeDefined();
			expect(reg.get(id)?.kind).toBe('trigger');
		}
	});
});

describe('createDefaultRegistry — active-deck shortcuts', () => {
	const reg = createDefaultRegistry();

	it('crossfader-left décrémente de 0.05', () => {
		const ctx = makeCtx();
		ctx.setCrossfader(0.5);
		reg.dispatch('crossfader-left', 1, ctx);
		expect(ctx.getCrossfader()).toBeCloseTo(0.45);
	});

	it('crossfader-right incrémente de 0.05', () => {
		const ctx = makeCtx();
		ctx.setCrossfader(0.5);
		reg.dispatch('crossfader-right', 1, ctx);
		expect(ctx.getCrossfader()).toBeCloseTo(0.55);
	});

	it('crossfader-left est clampé à 0', () => {
		const ctx = makeCtx();
		ctx.setCrossfader(0.02);
		reg.dispatch('crossfader-left', 1, ctx);
		expect(ctx.getCrossfader()).toBe(0);
	});

	it('crossfader-right est clampé à 1', () => {
		const ctx = makeCtx();
		ctx.setCrossfader(0.98);
		reg.dispatch('crossfader-right', 1, ctx);
		expect(ctx.getCrossfader()).toBe(1);
	});

	it('deck-switch alterne A↔B', () => {
		const ctx = makeCtx();
		expect(ctx.getActiveDeck()).toBe('A');
		reg.dispatch('deck-switch', 1, ctx);
		expect(ctx.getActiveDeck()).toBe('B');
		reg.dispatch('deck-switch', 1, ctx);
		expect(ctx.getActiveDeck()).toBe('A');
	});

	it('preset-next-active utilise le deck actif', () => {
		const ctx = makeCtx();
		ctx.switchActiveDeck(); // → B
		reg.dispatch('preset-next-active', 1, ctx);
		expect(ctx.navigatePreset).toHaveBeenCalledWith('B', 1);
	});

	it('playlist-toggle-active utilise le deck actif', () => {
		const ctx = makeCtx();
		reg.dispatch('playlist-toggle-active', 1, ctx);
		expect(ctx.togglePlaylist).toHaveBeenCalledWith('A');
	});

	it('playlist-prev-active utilise le deck actif', () => {
		const ctx = makeCtx();
		reg.dispatch('playlist-prev-active', 1, ctx);
		expect(ctx.playlistPrev).toHaveBeenCalledWith('A');
	});
});

describe('createDefaultRegistry — time param sliders (1.4)', () => {
	const reg = createDefaultRegistry();

	it('contient les 32 commandes time-* pour les 4 slots', () => {
		const prefixes = ['time-speed', 'time-zoom', 'time-rot', 'time-warp', 'time-dx', 'time-dy', 'time-stretch', 'time-wave'] as const;
		for (const prefix of prefixes) {
			for (const slot of [0, 1, 2, 3] as const) {
				const id = `${prefix}-${slot}` as const;
				expect(reg.get(id), `missing: ${id}`).toBeDefined();
				expect(reg.get(id)?.kind).toBe('range');
			}
		}
	});
});
