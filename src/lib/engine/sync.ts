import type { Overlay } from '$lib/engine/overlay.js';

export type SyncMessage =
	| { type: 'preset'; deck: 'A' | 'B'; name: string }
	| { type: 'crossfader'; value: number }
	| { type: 'source'; deviceId: string }
	| { type: 'loopback'; deviceId: number }
	| { type: 'quality'; tier: string }
	| { type: 'overlays'; list: Overlay[] }
	| { type: 'beat' }
	| { type: 'hello' };

const CHANNEL = 'opendrop-output';

// In Electron, BroadcastChannel is scoped per renderer process.
// We relay through the main process via IPC instead.
const eAPI = typeof window !== 'undefined' ? window.electronAPI : undefined;

function sendMsg(bc: BroadcastChannel | null, msg: SyncMessage) {
	if (eAPI) {
		eAPI.sendBroadcast(msg);
	} else {
		bc!.postMessage(msg);
	}
}

export class MainSync {
	private bc: BroadcastChannel | null = eAPI ? null : new BroadcastChannel(CHANNEL);
	private readyCb: (() => void) | null = null;
	private unlisten: (() => void) | null = null;

	constructor() {
		const onMsg = (data: unknown) => {
			if ((data as SyncMessage).type === 'hello') this.readyCb?.();
		};
		if (eAPI) {
			this.unlisten = eAPI.onBroadcast(onMsg);
		} else {
			this.bc!.onmessage = (e) => onMsg(e.data);
		}
	}

	onOutputReady(cb: () => void) { this.readyCb = cb; }

	sendPreset(deck: 'A' | 'B', name: string) {
		sendMsg(this.bc, { type: 'preset', deck, name });
	}

	sendCrossfader(value: number) {
		sendMsg(this.bc, { type: 'crossfader', value });
	}

	sendSource(deviceId: string) {
		sendMsg(this.bc, { type: 'source', deviceId });
	}

	sendLoopback(deviceId: number) {
		sendMsg(this.bc, { type: 'loopback', deviceId });
	}

	sendQuality(tier: string) {
		sendMsg(this.bc, { type: 'quality', tier });
	}

	sendOverlays(list: Overlay[]) {
		sendMsg(this.bc, { type: 'overlays', list });
	}

	sendBeat() {
		sendMsg(this.bc, { type: 'beat' });
	}

	destroy() {
		this.bc?.close();
		this.unlisten?.();
	}
}

export class OutputSync {
	private bc: BroadcastChannel | null = eAPI ? null : new BroadcastChannel(CHANNEL);
	private unlisten: (() => void) | null = null;

	listen(cb: (msg: SyncMessage) => void) {
		if (eAPI) {
			this.unlisten = eAPI.onBroadcast((data) => cb(data as SyncMessage));
		} else {
			this.bc!.onmessage = (e) => cb(e.data as SyncMessage);
		}
	}

	sendHello() {
		sendMsg(this.bc, { type: 'hello' });
	}

	destroy() {
		this.bc?.close();
		this.unlisten?.();
	}
}
