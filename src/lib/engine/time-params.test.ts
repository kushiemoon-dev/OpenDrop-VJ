import { describe, it, expect } from 'vitest';
import { defaultTimeParams, injectTimeParams, withTimeParams, type TimeParamsTuple } from './time-params.js';

describe('defaultTimeParams', () => {
	it('tous les multiplicateurs valent 1 (neutre)', () => {
		expect(defaultTimeParams()).toEqual({
			speedMult: 1, zoomMult: 1, rotMult: 1, warpMult: 1,
			dxMult: 1, dyMult: 1, stretchMult: 1, waveMult: 1,
		});
	});
});

describe('injectTimeParams', () => {
	it('clone le preset sans muter l\'original', () => {
		const preset = { frame_eqs_str: 'a.zoom = 1.01;', other: 'field' };
		const patched = injectTimeParams(preset, 0);
		expect(preset.frame_eqs_str).toBe('a.zoom = 1.01;'); // inchangé
		expect(patched).not.toBe(preset); // objet différent
		expect((patched as any).other).toBe('field'); // champs non touchés préservés
	});

	it('préfixe a.time scalé, avant le code original du preset', () => {
		const preset = { frame_eqs_str: 'a.zoom = 1.01;' };
		const patched = injectTimeParams(preset, 0) as any;
		const speedLineIndex = patched.frame_eqs_str.indexOf('a.time = a.time *');
		const originalLineIndex = patched.frame_eqs_str.indexOf('a.zoom = 1.01;');
		expect(speedLineIndex).toBeGreaterThanOrEqual(0);
		expect(speedLineIndex).toBeLessThan(originalLineIndex);
	});

	it('ajoute les 7 lignes de multiplicateur après le code original, référençant window.__odDeckParams[slot]', () => {
		const preset = { frame_eqs_str: 'a.zoom = 1.01;' };
		const patched = injectTimeParams(preset, 2) as any;
		for (const field of ['zoomMult', 'rotMult', 'warpMult', 'dxMult', 'dyMult', 'stretchMult', 'waveMult']) {
			expect(patched.frame_eqs_str).toContain(`window.__odDeckParams[2].${field}`);
		}
	});

	it('namespace correctement par slot (pas de collision entre decks)', () => {
		const preset = { frame_eqs_str: '' };
		const patched0 = injectTimeParams(preset, 0) as any;
		const patched3 = injectTimeParams(preset, 3) as any;
		expect(patched0.frame_eqs_str).toContain('window.__odDeckParams[0]');
		expect(patched0.frame_eqs_str).not.toContain('window.__odDeckParams[3]');
		expect(patched3.frame_eqs_str).toContain('window.__odDeckParams[3]');
		expect(patched3.frame_eqs_str).not.toContain('window.__odDeckParams[0]');
	});

	it('gère un preset sans frame_eqs_str (chaîne vide par défaut)', () => {
		const preset = {};
		const patched = injectTimeParams(preset, 0) as any;
		expect(patched.frame_eqs_str).toContain('a.time = a.time *');
	});
});

describe('withTimeParams', () => {
	const params: TimeParamsTuple = [defaultTimeParams(), defaultTimeParams(), defaultTimeParams(), defaultTimeParams()];

	it('met à jour uniquement le slot ciblé', () => {
		const next = withTimeParams(params, 1, { speedMult: 1.5 });
		expect(next[1].speedMult).toBe(1.5);
		expect(next[0].speedMult).toBe(1);
		expect(next[2].speedMult).toBe(1);
	});

	it('merge un patch partiel sans toucher aux autres champs', () => {
		const next = withTimeParams(params, 0, { zoomMult: 2 });
		expect(next[0].zoomMult).toBe(2);
		expect(next[0].speedMult).toBe(1);
	});

	it('ne mute pas le tableau ni les objets source', () => {
		const next = withTimeParams(params, 3, { rotMult: 0.5 });
		expect(next).not.toBe(params);
		expect(next[3]).not.toBe(params[3]);
		expect(params[3].rotMult).toBe(1);
	});
});
