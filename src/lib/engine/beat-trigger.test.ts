import { describe, it, expect } from 'vitest';
import {
	defaultBeatTriggerConfig, shouldTriggerOnBeat,
	defaultVolumePeakState, detectVolumePeak,
	clampBeatsPerChange, clampOffset,
	applyBeatTriggerPatch,
} from './beat-trigger.js';

describe('defaultBeatTriggerConfig', () => {
	it('mode beat, seuil 8, offset 0, sensibilité 0.5', () => {
		expect(defaultBeatTriggerConfig()).toEqual({ mode: 'beat', beatsPerChange: 8, offset: 0, sensitivity: 0.5 });
	});
});

describe('shouldTriggerOnBeat', () => {
	it('déclenche tous les N beats (N = beatsPerChange)', () => {
		const cfg = { mode: 'beat' as const, beatsPerChange: 4, offset: 0, sensitivity: 0.5 };
		expect(shouldTriggerOnBeat(0, cfg)).toBe(true);
		expect(shouldTriggerOnBeat(1, cfg)).toBe(false);
		expect(shouldTriggerOnBeat(4, cfg)).toBe(true);
		expect(shouldTriggerOnBeat(8, cfg)).toBe(true);
	});

	it('respecte le décalage (offset)', () => {
		const cfg = { mode: 'beat' as const, beatsPerChange: 4, offset: 2, sensitivity: 0.5 };
		expect(shouldTriggerOnBeat(0, cfg)).toBe(false);
		expect(shouldTriggerOnBeat(2, cfg)).toBe(true);
		expect(shouldTriggerOnBeat(6, cfg)).toBe(true);
	});

	it('ne déclenche jamais en mode volume-peak', () => {
		const cfg = { mode: 'volume-peak' as const, beatsPerChange: 4, offset: 0, sensitivity: 0.5 };
		expect(shouldTriggerOnBeat(0, cfg)).toBe(false);
		expect(shouldTriggerOnBeat(4, cfg)).toBe(false);
	});
});

describe('detectVolumePeak', () => {
	it('ne déclenche pas sous le seuil', () => {
		const state = { rollingAvg: 0.3, lastTriggerAt: -Infinity };
		const { triggered } = detectVolumePeak(0.35, state, 0.5, 1000);
		expect(triggered).toBe(false);
	});

	it('déclenche sur un pic net au-dessus de la moyenne glissante', () => {
		const state = { rollingAvg: 0.2, lastTriggerAt: -Infinity };
		const { triggered } = detectVolumePeak(0.9, state, 0.5, 1000);
		expect(triggered).toBe(true);
	});

	it('respecte le cooldown (pas de re-déclenchement avant 500ms)', () => {
		const state = { rollingAvg: 0.2, lastTriggerAt: 1000 };
		const { triggered } = detectVolumePeak(0.9, state, 0.5, 1300);
		expect(triggered).toBe(false);
	});

	it('redéclenche après le cooldown', () => {
		const state = { rollingAvg: 0.2, lastTriggerAt: 1000 };
		const { triggered } = detectVolumePeak(0.9, state, 0.5, 1600);
		expect(triggered).toBe(true);
	});

	it('ignore le quasi-silence même avec un ratio élevé', () => {
		const state = { rollingAvg: 0.005, lastTriggerAt: -Infinity };
		const { triggered } = detectVolumePeak(0.019, state, 1, 1000);
		expect(triggered).toBe(false);
	});

	it('la moyenne glissante suit une tendance de volume croissante', () => {
		let state = defaultVolumePeakState();
		for (let i = 0; i < 50; i++) {
			state = detectVolumePeak(0.5, state, 0.5, i * 100).next;
		}
		expect(state.rollingAvg).toBeGreaterThan(0.4);
	});

	it('met à jour lastTriggerAt seulement quand ça déclenche', () => {
		const state = { rollingAvg: 0.3, lastTriggerAt: -Infinity };
		const { next } = detectVolumePeak(0.35, state, 0.5, 1000);
		expect(next.lastTriggerAt).toBe(-Infinity);
	});
});

describe('clampBeatsPerChange', () => {
	it('borne entre 1 et 64', () => {
		expect(clampBeatsPerChange(0)).toBe(1);
		expect(clampBeatsPerChange(100)).toBe(64);
		expect(clampBeatsPerChange(8)).toBe(8);
	});
});

describe('clampOffset', () => {
	it('borne entre 0 et beatsPerChange - 1', () => {
		expect(clampOffset(-1, 8)).toBe(0);
		expect(clampOffset(10, 8)).toBe(7);
		expect(clampOffset(3, 8)).toBe(3);
	});
});

describe('applyBeatTriggerPatch', () => {
	it('merge un patch partiel sans toucher aux champs non fournis', () => {
		const current = defaultBeatTriggerConfig();
		const next = applyBeatTriggerPatch(current, { mode: 'volume-peak' });
		expect(next.mode).toBe('volume-peak');
		expect(next.beatsPerChange).toBe(8);
		expect(next.sensitivity).toBe(0.5);
	});

	it('re-clamp beatsPerChange après le patch', () => {
		const current = defaultBeatTriggerConfig();
		expect(applyBeatTriggerPatch(current, { beatsPerChange: 100 }).beatsPerChange).toBe(64);
		expect(applyBeatTriggerPatch(current, { beatsPerChange: 0 }).beatsPerChange).toBe(1);
	});

	it('re-clamp offset relatif au NOUVEAU beatsPerChange (pas l\'ancien)', () => {
		const current = { ...defaultBeatTriggerConfig(), beatsPerChange: 8, offset: 7 };
		const next = applyBeatTriggerPatch(current, { beatsPerChange: 4 });
		expect(next.offset).toBe(3);
	});

	it('ne mute pas l\'objet current', () => {
		const current = defaultBeatTriggerConfig();
		const next = applyBeatTriggerPatch(current, { mode: 'volume-peak' });
		expect(current.mode).toBe('beat');
		expect(next).not.toBe(current);
	});
});
