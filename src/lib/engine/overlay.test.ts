import { describe, it, expect } from 'vitest';
import { makeOverlay } from './overlay.js';

describe('makeOverlay', () => {
	it('défaut à kind média (image/vidéo), rétro-compatible', () => {
		const ov = makeOverlay('mon-image');
		expect(ov.kind).toBe('media');
		expect(ov.video).toBe(false);
		expect(ov.text).toBe('');
	});

	it('crée un overlay texte avec les bons défauts', () => {
		const ov = makeOverlay('Texte', { kind: 'text', text: 'Hello' });
		expect(ov.kind).toBe('text');
		expect(ov.text).toBe('Hello');
		expect(ov.fontFamily).toBe('sans');
		expect(ov.fontSize).toBe(8);
		expect(ov.color).toBe('#ffffff');
	});

	it('les champs partagés (x/y/scale/opacity/spin/drift) restent inchangés', () => {
		const ov = makeOverlay('Texte', { kind: 'text' });
		expect(ov.x).toBe(0.5);
		expect(ov.y).toBe(0.5);
		expect(ov.scale).toBe(1);
		expect(ov.opacity).toBe(1);
		expect(ov.spin).toBe(0);
		expect(ov.driftX).toBe(0);
		expect(ov.driftY).toBe(0);
	});

	it('partial override remplace uniquement les champs fournis', () => {
		const ov = makeOverlay('Texte', { kind: 'text', fontFamily: 'impact', color: '#ff2d78' });
		expect(ov.fontFamily).toBe('impact');
		expect(ov.color).toBe('#ff2d78');
		expect(ov.fontSize).toBe(8); // pas touché, garde le défaut
	});

	it('inQueue défaut à false (non-cassant, opt-in)', () => {
		const ov = makeOverlay('mon-image');
		expect(ov.inQueue).toBe(false);
	});
});
