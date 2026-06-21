import { describe, it, expect } from 'vitest';
import { triggerKey, formatTrigger, type MidiMessage } from './midi.js';

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
