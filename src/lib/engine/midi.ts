export type MidiMessage = {
	type: 'cc' | 'note_on' | 'note_off';
	channel: number;
	number: number; // CC number ou note number
	value: number;  // 0-127
};

// Clé unique pour identifier un trigger MIDI
export type MidiTriggerKey = string; // `cc:${ch}:${num}` ou `note:${ch}:${num}`

export function triggerKey(msg: MidiMessage): MidiTriggerKey {
	const t = msg.type === 'cc' ? 'cc' : 'note';
	return `${t}:${msg.channel}:${msg.number}`;
}

export function formatTrigger(key: MidiTriggerKey): string {
	const [type, ch, num] = key.split(':');
	return type === 'cc' ? `CC${num} ch${ch}` : `Note${num} ch${ch}`;
}

export class MidiEngine {
	private access: MIDIAccess | null = null;
	private cb: ((msg: MidiMessage) => void) | null = null;

	async connect(): Promise<void> {
		this.access = await navigator.requestMIDIAccess();
		this.access.inputs.forEach((input) => {
			input.onmidimessage = (e) => this.handle(e);
		});
		this.access.onstatechange = (e) => {
			if (!e.port) return;
			if (e.port.type === 'input' && e.port.state === 'connected') {
				(e.port as MIDIInput).onmidimessage = (ev) => this.handle(ev);
			}
		};
	}

	onMessage(cb: (msg: MidiMessage) => void) {
		this.cb = cb;
	}

	get deviceNames(): string[] {
		if (!this.access) return [];
		const names: string[] = [];
		this.access.inputs.forEach((i) => names.push(i.name || i.id));
		return names;
	}

	private handle(e: MIDIMessageEvent) {
		const data = e.data;
		if (!data || data.length < 2) return;
		const [status, num, val = 0] = data;
		const typeByte = status & 0xf0;
		const channel = (status & 0x0f) + 1;

		let type: MidiMessage['type'];
		if (typeByte === 0xb0) type = 'cc';
		else if (typeByte === 0x90) type = val > 0 ? 'note_on' : 'note_off';
		else if (typeByte === 0x80) type = 'note_off';
		else return;

		this.cb?.({ type, channel, number: num, value: val });
	}

	destroy() {
		if (this.access) {
			this.access.inputs.forEach((i) => { i.onmidimessage = null; });
			this.access = null;
		}
		this.cb = null;
	}
}
