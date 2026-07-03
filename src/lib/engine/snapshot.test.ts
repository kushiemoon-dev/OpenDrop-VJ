import { describe, it, expect } from 'vitest';
import { smoothstep, interpolateSnapshot } from './snapshot.js';
import type { CommandId } from './commands.js';

describe('smoothstep', () => {
	it('vaut 0 à t=0 et 1 à t=1', () => {
		expect(smoothstep(0)).toBe(0);
		expect(smoothstep(1)).toBe(1);
	});
	it('vaut 0.5 au milieu (t=0.5)', () => {
		expect(smoothstep(0.5)).toBeCloseTo(0.5);
	});
	it("n'est PAS linéaire : t=0.25 ne donne pas 0.25", () => {
		expect(smoothstep(0.25)).toBeCloseTo(0.15625); // 0.25²·(3−0.5)
		expect(smoothstep(0.25)).not.toBe(0.25);
	});
	it('est symétrique : f(t) + f(1−t) = 1', () => {
		expect(smoothstep(0.25) + smoothstep(0.75)).toBeCloseTo(1);
	});
	it('clampe hors bornes', () => {
		expect(smoothstep(-1)).toBe(0);
		expect(smoothstep(2)).toBe(1);
	});
});

describe('interpolateSnapshot', () => {
	const A = 'color-hue-a' as CommandId;
	const B = 'composite-blend-0' as CommandId;

	it('progress 0 → renvoie les valeurs de départ', () => {
		expect(interpolateSnapshot({ [A]: 0, [B]: 1 }, { [A]: 1, [B]: 0 }, 0)).toEqual({ [A]: 0, [B]: 1 });
	});
	it('progress 1 → renvoie exactement la cible', () => {
		expect(interpolateSnapshot({ [A]: 0, [B]: 1 }, { [A]: 1, [B]: 0 }, 1)).toEqual({ [A]: 1, [B]: 0 });
	});
	it('progress 0.5 → milieu par clé', () => {
		expect(interpolateSnapshot({ [A]: 0 }, { [A]: 1 }, 0.5)).toEqual({ [A]: 0.5 });
	});
	it('clé absente du départ → part de la cible (aucun mouvement)', () => {
		expect(interpolateSnapshot({}, { [A]: 0.8 }, 0.5)).toEqual({ [A]: 0.8 });
	});
	it('clé absente de la cible → ignorée (crossfader jamais piloté)', () => {
		// A présent des deux côtés (interpole normalement) ; crossfader présent
		// seulement au départ (absent de la cible) → doit être ignoré, pas
		// juste "non modifié depuis le départ".
		const out = interpolateSnapshot({ [A]: 0, ['crossfader' as CommandId]: 0.2 }, { [A]: 1 }, 0.5);
		expect(out).toEqual({ [A]: 0.5 });
		expect('crossfader' in out).toBe(false);
	});
});
