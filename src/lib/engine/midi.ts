export type MidiMessage = {
	type: 'cc' | 'note_on' | 'note_off' | 'pitchbend';
	deviceId: string;
	channel: number;
	number: number;    // CC/note number; 0 for pitchbend
	value: number;     // 0-127 for 7-bit; 0-16383 for 14-bit CC and pitchbend
	is14bit?: boolean; // true when value uses 14-bit range
};

// Clé unique pour identifier un trigger MIDI (inclut deviceId pour multi-controller)
export type MidiTriggerKey = string; // `${deviceId}:cc:${ch}:${num}` | `${deviceId}:note:${ch}:${num}` | `${deviceId}:pb:${ch}`

export function triggerKey(msg: MidiMessage): MidiTriggerKey {
	if (msg.type === 'pitchbend') return `${msg.deviceId}:pb:${msg.channel}`;
	const t = msg.type === 'cc' ? 'cc' : 'note';
	return `${msg.deviceId}:${t}:${msg.channel}:${msg.number}`;
}

export function formatTrigger(key: MidiTriggerKey): string {
	const parts = key.split(':');
	// New format: parts[1] is the type token (cc|note|pb)
	if (parts.length >= 3 && /^(cc|note|pb)$/.test(parts[1])) {
		const type = parts[1];
		const ch = parts[2];
		const num = parts[3] ?? '';
		if (type === 'pb') return `PitchBend ch${ch}`;
		return type === 'cc' ? `CC${num} ch${ch}` : `Note${num} ch${ch}`;
	}
	// Legacy 3-part format (before M4): type:ch:num
	const [type, ch, num] = parts;
	if (type === 'cc') return `CC${num} ch${ch}`;
	if (type === 'note') return `Note${num} ch${ch}`;
	return key;
}

export type ParsedTriggerKey =
	| { deviceId: string; kind: 'note' | 'cc'; channel: number; number: number }
	| { deviceId: string; kind: 'pb'; channel: number };

/** Parses the new-format key produced by triggerKey(). Returns null for the legacy
 * 3-part format (no deviceId) — those mappings predate multi-controller support and
 * can't be addressed for output feedback. */
export function parseTriggerKey(key: MidiTriggerKey): ParsedTriggerKey | null {
	const parts = key.split(':');
	if (parts.length < 3) return null;
	const kind = parts[1];
	if (kind !== 'cc' && kind !== 'note' && kind !== 'pb') return null;
	const deviceId = parts[0];
	const channel = Number(parts[2]);
	if (!Number.isFinite(channel)) return null;
	if (kind === 'pb') return { deviceId, kind, channel };
	if (parts.length < 4) return null;
	const number = Number(parts[3]);
	if (!Number.isFinite(number)) return null;
	return { deviceId, kind, channel, number };
}

/** Finds the output port whose name matches the given input port's name — WebMIDI
 * doesn't guarantee a shared id between the input/output of the same physical device. */
export function findMatchingOutputId(
	inputName: string | null | undefined,
	outputs: Array<{ id: string; name: string | null }>,
): string | null {
	if (!inputName) return null;
	const match = outputs.find((o) => o.name === inputName);
	return match ? match.id : null;
}

type MsgCb = (msg: MidiMessage) => void;
type ClockCb = () => void;

export class MidiEngine {
	private access: MIDIAccess | null = null;
	private msgCb: MsgCb | null = null;
	private clockCb: ClockCb | null = null;
	private outputReconnectCb: (() => void) | null = null;
	// 14-bit pending MSBs keyed by `${deviceId}:${ch}:${ccNum}`
	private readonly _msb = new Map<string, number>();

	async connect(): Promise<void> {
		this.access = await navigator.requestMIDIAccess({ sysex: false });
		this.access.inputs.forEach((input) => {
			input.onmidimessage = (e) => this._handle(e);
		});
		this.access.onstatechange = (e) => {
			if (!e.port) return;
			if (e.port.type === 'input' && e.port.state === 'connected') {
				(e.port as MIDIInput).onmidimessage = (ev) => this._handle(ev);
			}
			if (e.port.type === 'output' && e.port.state === 'connected') {
				this.outputReconnectCb?.();
			}
		};
	}

	onMessage(cb: MsgCb) { this.msgCb = cb; }

	/** Called on every MIDI clock pulse (0xF8, 24 per quarter note). */
	onClock(cb: ClockCb) { this.clockCb = cb; }

	/** Called whenever an output port (re)connects — used to re-push LED feedback state. */
	onOutputReconnect(cb: () => void) { this.outputReconnectCb = cb; }

	/** Sends generic on/off LED feedback (no color/sysex) for a bound trigger key. */
	sendFeedback(key: MidiTriggerKey, on: boolean): void {
		if (!this.access) return;
		const parsed = parseTriggerKey(key);
		if (!parsed || parsed.kind === 'pb') return;
		const inputName = this.access.inputs.get(parsed.deviceId)?.name;
		const outputs = Array.from(this.access.outputs.values(), (o) => ({ id: o.id, name: o.name }));
		const outputId = findMatchingOutputId(inputName, outputs);
		if (!outputId) return;
		const output = this.access.outputs.get(outputId);
		if (!output) return;
		const statusByte = (parsed.kind === 'note' ? 0x90 : 0xb0) | (parsed.channel - 1);
		output.send([statusByte, parsed.number, on ? 127 : 0]);
	}

	get deviceNames(): string[] {
		if (!this.access) return [];
		return Array.from(this.access.inputs.values(), i => i.name || i.id);
	}

	private _handle(e: MIDIMessageEvent) {
		const data = e.data;
		if (!data || data.length < 1) return;

		// MIDI clock — single byte, fire callback and return
		if (data[0] === 0xf8) {
			this.clockCb?.();
			return;
		}

		if (data.length < 2) return;
		const status = data[0];
		const num = data[1];
		const val = data.length > 2 ? data[2] : 0;
		const typeByte = status & 0xf0;
		const channel = (status & 0x0f) + 1;
		const deviceId = (e.target as MIDIInput)?.id ?? 'unknown';

		// Pitchbend: lsb=data[1], msb=data[2], range 0..16383, center=8192
		if (typeByte === 0xe0) {
			const pb14 = (val << 7) | num;
			this.msgCb?.({ type: 'pitchbend', deviceId, channel, number: 0, value: pb14, is14bit: true });
			return;
		}

		// CC
		if (typeByte === 0xb0) {
			if (num <= 31) {
				// MSB of potential 14-bit control — store and dispatch immediately as 7-bit
				this._msb.set(`${deviceId}:${channel}:${num}`, val);
				this.msgCb?.({ type: 'cc', deviceId, channel, number: num, value: val });
			} else if (num >= 32 && num <= 63) {
				// Possible LSB of a 14-bit pair
				const coarse = num - 32;
				const msb = this._msb.get(`${deviceId}:${channel}:${coarse}`);
				if (msb !== undefined) {
					const value14 = (msb << 7) | (val & 0x7f);
					this._msb.delete(`${deviceId}:${channel}:${coarse}`);
					// Dispatch on the coarse CC number with 14-bit value
					this.msgCb?.({ type: 'cc', deviceId, channel, number: coarse, value: value14, is14bit: true });
				} else {
					// No matching MSB — treat as standalone 7-bit
					this.msgCb?.({ type: 'cc', deviceId, channel, number: num, value: val });
				}
			} else {
				this.msgCb?.({ type: 'cc', deviceId, channel, number: num, value: val });
			}
			return;
		}

		let type: MidiMessage['type'];
		if (typeByte === 0x90) type = val > 0 ? 'note_on' : 'note_off';
		else if (typeByte === 0x80) type = 'note_off';
		else return;

		this.msgCb?.({ type, deviceId, channel, number: num, value: val });
	}

	destroy() {
		if (this.access) {
			this.access.inputs.forEach((i) => { i.onmidimessage = null; });
			this.access = null;
		}
		this.msgCb = null;
		this.clockCb = null;
		this.outputReconnectCb = null;
		this._msb.clear();
	}
}
