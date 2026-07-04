import { describe, it, expect, vi } from 'vitest';
import { triggerKey, formatTrigger, parseTriggerKey, findMatchingOutputId, MidiEngine, type MidiMessage } from './midi.js';

const DEV = 'dev0';

describe('triggerKey — format multi-controller', () => {
	it('CC → deviceId:cc:channel:number', () => {
		const msg: MidiMessage = { type: 'cc', deviceId: DEV, channel: 1, number: 7, value: 64 };
		expect(triggerKey(msg)).toBe('dev0:cc:1:7');
	});

	it('note_on → deviceId:note:channel:number', () => {
		const msg: MidiMessage = { type: 'note_on', deviceId: DEV, channel: 2, number: 60, value: 127 };
		expect(triggerKey(msg)).toBe('dev0:note:2:60');
	});

	it('note_off → même clé que note_on', () => {
		const msg: MidiMessage = { type: 'note_off', deviceId: DEV, channel: 1, number: 36, value: 0 };
		expect(triggerKey(msg)).toBe('dev0:note:1:36');
	});

	it('pitchbend → deviceId:pb:channel', () => {
		const msg: MidiMessage = { type: 'pitchbend', deviceId: DEV, channel: 1, number: 0, value: 8192, is14bit: true };
		expect(triggerKey(msg)).toBe('dev0:pb:1');
	});

	it('deux contrôleurs différents → clés différentes pour même CC', () => {
		const a: MidiMessage = { type: 'cc', deviceId: 'ctrl-a', channel: 1, number: 1, value: 64 };
		const b: MidiMessage = { type: 'cc', deviceId: 'ctrl-b', channel: 1, number: 1, value: 64 };
		expect(triggerKey(a)).not.toBe(triggerKey(b));
	});
});

describe('formatTrigger', () => {
	it('formate une clé CC new-format', () => {
		expect(formatTrigger('dev0:cc:1:7')).toBe('CC7 ch1');
	});

	it('formate une clé Note new-format', () => {
		expect(formatTrigger('dev0:note:3:60')).toBe('Note60 ch3');
	});

	it('formate pitchbend', () => {
		expect(formatTrigger('dev0:pb:1')).toBe('PitchBend ch1');
	});

	it('rétrocompat avec ancien format 3 parties (legacy)', () => {
		expect(formatTrigger('cc:1:7')).toBe('CC7 ch1');
		expect(formatTrigger('note:3:60')).toBe('Note60 ch3');
	});
});

describe('normalisation 14-bit vs 7-bit', () => {
	it('14-bit pitchbend centre (8192) → 0.5', () => {
		expect(8192 / 16383).toBeCloseTo(0.5, 2);
	});

	it('14-bit CC max (16383) → 1.0', () => {
		expect(16383 / 16383).toBe(1);
	});

	it('7-bit max (127) → 1.0', () => {
		expect(127 / 127).toBe(1);
	});
});

describe('parseTriggerKey', () => {
	it('parse une clé note', () => {
		expect(parseTriggerKey('dev0:note:2:60')).toEqual({ deviceId: 'dev0', kind: 'note', channel: 2, number: 60 });
	});

	it('parse une clé cc', () => {
		expect(parseTriggerKey('dev0:cc:1:7')).toEqual({ deviceId: 'dev0', kind: 'cc', channel: 1, number: 7 });
	});

	it('parse pitchbend (pas de champ number)', () => {
		expect(parseTriggerKey('dev0:pb:1')).toEqual({ deviceId: 'dev0', kind: 'pb', channel: 1 });
	});

	it('rejette le format legacy 3-parties (pas de deviceId)', () => {
		expect(parseTriggerKey('cc:1:7')).toBeNull();
		expect(parseTriggerKey('note:3:60')).toBeNull();
	});

	it('rejette une clé invalide', () => {
		expect(parseTriggerKey('')).toBeNull();
		expect(parseTriggerKey('garbage')).toBeNull();
		expect(parseTriggerKey('dev0:cc:notanumber:7')).toBeNull();
		expect(parseTriggerKey('dev0:cc:1')).toBeNull(); // manque le number pour cc
	});
});

describe('findMatchingOutputId', () => {
	const outputs = [{ id: 'out1', name: 'FakePad' }, { id: 'out2', name: 'Other' }];

	it('matche par nom exact', () => {
		expect(findMatchingOutputId('FakePad', outputs)).toBe('out1');
	});

	it('retourne null si aucun match', () => {
		expect(findMatchingOutputId('Unknown', outputs)).toBeNull();
	});

	it('retourne null pour un nom absent (null/undefined)', () => {
		expect(findMatchingOutputId(null, outputs)).toBeNull();
		expect(findMatchingOutputId(undefined, outputs)).toBeNull();
	});

	it('prend le premier match si les noms sont dupliqués', () => {
		const dup = [{ id: 'a', name: 'X' }, { id: 'b', name: 'X' }];
		expect(findMatchingOutputId('X', dup)).toBe('a');
	});
});

function fakeAccess(opts: {
	inputs?: Array<{ id: string; name: string }>;
	outputs?: Array<{ id: string; name: string; send: (data: number[]) => void }>;
}) {
	return {
		inputs: new Map((opts.inputs ?? []).map((i) => [i.id, i])),
		outputs: new Map((opts.outputs ?? []).map((o) => [o.id, o])),
		onstatechange: null,
	};
}

describe('MidiEngine.sendFeedback', () => {
	it('envoie Note On/Off vers le port de sortie qui matche par nom', () => {
		const send = vi.fn();
		const engine = new MidiEngine();
		// @ts-expect-error — injection directe pour tester sans navigator.requestMIDIAccess
		engine.access = fakeAccess({
			inputs: [{ id: 'dev0', name: 'FakePad' }],
			outputs: [{ id: 'out0', name: 'FakePad', send }],
		});
		engine.sendFeedback('dev0:note:1:60', true);
		expect(send).toHaveBeenCalledWith([0x90, 60, 127]);
		engine.sendFeedback('dev0:note:1:60', false);
		expect(send).toHaveBeenCalledWith([0x90, 60, 0]);
	});

	it('envoie un CC pour un binding de type cc', () => {
		const send = vi.fn();
		const engine = new MidiEngine();
		// @ts-expect-error — injection directe
		engine.access = fakeAccess({
			inputs: [{ id: 'dev0', name: 'FakePad' }],
			outputs: [{ id: 'out0', name: 'FakePad', send }],
		});
		engine.sendFeedback('dev0:cc:1:7', true);
		expect(send).toHaveBeenCalledWith([0xb0, 7, 127]);
	});

	it('ne fait rien pour un binding pitchbend', () => {
		const send = vi.fn();
		const engine = new MidiEngine();
		// @ts-expect-error — injection directe
		engine.access = fakeAccess({
			inputs: [{ id: 'dev0', name: 'FakePad' }],
			outputs: [{ id: 'out0', name: 'FakePad', send }],
		});
		engine.sendFeedback('dev0:pb:1', true);
		expect(send).not.toHaveBeenCalled();
	});

	it('ne fait rien si aucun output ne matche le nom du input', () => {
		const send = vi.fn();
		const engine = new MidiEngine();
		// @ts-expect-error — injection directe
		engine.access = fakeAccess({
			inputs: [{ id: 'dev0', name: 'FakePad' }],
			outputs: [{ id: 'out0', name: 'OtherDevice', send }],
		});
		engine.sendFeedback('dev0:note:1:60', true);
		expect(send).not.toHaveBeenCalled();
	});

	it('ne fait rien et ne throw pas si non connecté', () => {
		const engine = new MidiEngine();
		expect(() => engine.sendFeedback('dev0:note:1:60', true)).not.toThrow();
	});

	it('ne fait rien pour une clé legacy sans deviceId', () => {
		const send = vi.fn();
		const engine = new MidiEngine();
		// @ts-expect-error — injection directe
		engine.access = fakeAccess({
			inputs: [{ id: 'dev0', name: 'FakePad' }],
			outputs: [{ id: 'out0', name: 'FakePad', send }],
		});
		engine.sendFeedback('note:1:60', true);
		expect(send).not.toHaveBeenCalled();
	});
});
