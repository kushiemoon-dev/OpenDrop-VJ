import { describe, it, expect } from 'vitest';
import { triggerKey, formatTrigger, type MidiMessage } from './midi.js';

describe('triggerKey', () => {
	it('CC → clé cc:channel:number', () => {
		const msg: MidiMessage = { type: 'cc', channel: 1, number: 7, value: 64 };
		expect(triggerKey(msg)).toBe('cc:1:7');
	});

	it('note_on → clé note:channel:number', () => {
		const msg: MidiMessage = { type: 'note_on', channel: 2, number: 60, value: 127 };
		expect(triggerKey(msg)).toBe('note:2:60');
	});

	it('note_off → clé note:channel:number (même que note_on)', () => {
		const msg: MidiMessage = { type: 'note_off', channel: 1, number: 36, value: 0 };
		expect(triggerKey(msg)).toBe('note:1:36');
	});
});

describe('formatTrigger', () => {
	it('formate une clé CC lisiblement', () => {
		expect(formatTrigger('cc:1:7')).toBe('CC7 ch1');
	});

	it('formate une clé Note lisiblement', () => {
		expect(formatTrigger('note:3:60')).toBe('Note60 ch3');
	});
});
