import { describe, it, expect } from 'vitest';
import { makeOverlay } from './overlay.js';

describe('makeOverlay', () => {
	it('defaults to kind media (image/video), backward-compatible', () => {
		const ov = makeOverlay('mon-image');
		expect(ov.kind).toBe('media');
		expect(ov.video).toBe(false);
		expect(ov.text).toBe('');
	});

	it('creates a text overlay with the correct defaults', () => {
		const ov = makeOverlay('Texte', { kind: 'text', text: 'Hello' });
		expect(ov.kind).toBe('text');
		expect(ov.text).toBe('Hello');
		expect(ov.fontFamily).toBe('sans');
		expect(ov.fontSize).toBe(8);
		expect(ov.color).toBe('#ffffff');
	});

	it('shared fields (x/y/scale/opacity/spin/drift) remain unchanged', () => {
		const ov = makeOverlay('Texte', { kind: 'text' });
		expect(ov.x).toBe(0.5);
		expect(ov.y).toBe(0.5);
		expect(ov.scale).toBe(1);
		expect(ov.opacity).toBe(1);
		expect(ov.spin).toBe(0);
		expect(ov.driftX).toBe(0);
		expect(ov.driftY).toBe(0);
	});

	it('partial override replaces only the provided fields', () => {
		const ov = makeOverlay('Texte', { kind: 'text', fontFamily: 'impact', color: '#ff2d78' });
		expect(ov.fontFamily).toBe('impact');
		expect(ov.color).toBe('#ff2d78');
		expect(ov.fontSize).toBe(8); // not touched, keeps the default
	});

	it('inQueue defaults to false (non-breaking, opt-in)', () => {
		const ov = makeOverlay('mon-image');
		expect(ov.inQueue).toBe(false);
	});
});
