import { describe, it, expect } from 'vitest';
import { defaultQVarParams, injectQVarParams, withQVarValue, withQVarWatch, withoutQVarWatch, type QVarParamsTuple } from './q-vars.js';

describe('defaultQVarParams', () => {
	it('32 slots, tous désactivés, valeur 0', () => {
		const p = defaultQVarParams();
		expect(p.enabled).toHaveLength(32);
		expect(p.value).toHaveLength(32);
		expect(p.enabled.every((e) => e === false)).toBe(true);
		expect(p.value.every((v) => v === 0)).toBe(true);
	});
});

describe('injectQVarParams', () => {
	it("clone le preset sans muter l'original", () => {
		const preset = { frame_eqs_str: 'a.zoom = 1.01;', other: 'field' };
		const patched = injectQVarParams(preset, 0);
		expect(preset.frame_eqs_str).toBe('a.zoom = 1.01;');
		expect(patched).not.toBe(preset);
		expect((patched as any).other).toBe('field');
	});

	it('ajoute le code original avant les 32 lignes de garde', () => {
		const preset = { frame_eqs_str: 'a.zoom = 1.01;' };
		const patched = injectQVarParams(preset, 0) as any;
		const originalIndex = patched.frame_eqs_str.indexOf('a.zoom = 1.01;');
		const firstGuardIndex = patched.frame_eqs_str.indexOf('if (window.__odQVarParams[0].enabled[0])');
		expect(originalIndex).toBeGreaterThanOrEqual(0);
		expect(firstGuardIndex).toBeGreaterThan(originalIndex);
	});

	it('génère 32 lignes de garde q1..q32, référençant window.__odQVarParams[slot]', () => {
		const preset = { frame_eqs_str: '' };
		const patched = injectQVarParams(preset, 2) as any;
		for (let n = 1; n <= 32; n++) {
			expect(patched.frame_eqs_str).toContain(
				`if (window.__odQVarParams[2].enabled[${n - 1}]) { q${n} = window.__odQVarParams[2].value[${n - 1}]; }`
			);
		}
	});

	it('namespace correctement par slot (pas de collision entre decks)', () => {
		const preset = { frame_eqs_str: '' };
		const patched0 = injectQVarParams(preset, 0) as any;
		const patched3 = injectQVarParams(preset, 3) as any;
		expect(patched0.frame_eqs_str).toContain('window.__odQVarParams[0]');
		expect(patched0.frame_eqs_str).not.toContain('window.__odQVarParams[3]');
		expect(patched3.frame_eqs_str).toContain('window.__odQVarParams[3]');
		expect(patched3.frame_eqs_str).not.toContain('window.__odQVarParams[0]');
	});

	it('gère un preset sans frame_eqs_str (chaîne vide par défaut)', () => {
		const preset = {};
		const patched = injectQVarParams(preset, 0) as any;
		expect(patched.frame_eqs_str).toContain('if (window.__odQVarParams[0].enabled[0])');
	});
});

describe('withQVarValue', () => {
	const params: QVarParamsTuple = [defaultQVarParams(), defaultQVarParams(), defaultQVarParams(), defaultQVarParams()];

	it('met à jour la valeur du q-var ciblé (1-indexé) pour le slot ciblé', () => {
		const next = withQVarValue(params, 1, 5, 1.5);
		expect(next[1].value[4]).toBe(1.5);
		expect(next[0].value[4]).toBe(0);
		expect(next[2].value[4]).toBe(0);
	});

	it('ne touche pas enabled ni les autres valeurs', () => {
		const next = withQVarValue(params, 0, 3, -1);
		expect(next[0].enabled[2]).toBe(false);
		expect(next[0].value[0]).toBe(0);
	});

	it('ne mute pas le tableau ni les objets source', () => {
		const next = withQVarValue(params, 2, 1, 2);
		expect(next).not.toBe(params);
		expect(next[2]).not.toBe(params[2]);
		expect(params[2].value[0]).toBe(0);
	});
});

describe('withQVarWatch', () => {
	const params: QVarParamsTuple = [defaultQVarParams(), defaultQVarParams(), defaultQVarParams(), defaultQVarParams()];

	it('active le watch et remet la valeur à 0', () => {
		const dirty: QVarParamsTuple = withQVarValue(params, 0, 7, 1.9);
		const next = withQVarWatch(dirty, 0, 7);
		expect(next[0].enabled[6]).toBe(true);
		expect(next[0].value[6]).toBe(0);
	});

	it('ne touche pas les autres slots/q-vars', () => {
		const next = withQVarWatch(params, 1, 10);
		expect(next[0].enabled[9]).toBe(false);
		expect(next[1].enabled.filter(Boolean)).toHaveLength(1);
	});
});

describe('withoutQVarWatch', () => {
	it('désactive le watch sans toucher à la dernière valeur', () => {
		const params: QVarParamsTuple = [defaultQVarParams(), defaultQVarParams(), defaultQVarParams(), defaultQVarParams()];
		const watched = withQVarWatch(params, 0, 12);
		const valued = withQVarValue(watched, 0, 12, 1.2);
		const next = withoutQVarWatch(valued, 0, 12);
		expect(next[0].enabled[11]).toBe(false);
		expect(next[0].value[11]).toBe(1.2);
	});
});
