import { describe, it, expect } from 'vitest';
import { defaultQVarParams, injectQVarParams } from './q-vars.js';

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
