import type { Overlay } from '$lib/engine/overlay.js';
import type { ClipRef } from '$lib/engine/video-store.js';
import type { InvisibleMode } from '$lib/engine/quality.js';
import type { DeckTimeParams } from '$lib/engine/time-params.js';

export interface ColorParams {
	hueRotate: number;   // 0..1 → 0..360°
	saturate: number;    // 0..1 → 0..200% (0.5 = 100% = normal)
	brightness: number;  // 0..1 → 0..200% (0.5 = 100% = normal)
	contrast: number;    // 0..1 → 0..200% (0.5 = 100% = normal)
	invert: number;      // 0..1
}

export const DEFAULT_COLOR_PARAMS: ColorParams = {
	hueRotate: 0,
	saturate: 0.5,
	brightness: 0.5,
	contrast: 0.5,
	invert: 0,
}

export function colorParamsToFilter(p: ColorParams): string {
	const isDefault =
		p.hueRotate === 0 && p.saturate === 0.5 && p.brightness === 0.5 &&
		p.contrast === 0.5 && p.invert === 0;
	if (isDefault) return 'none';
	const parts: string[] = [];
	if (p.hueRotate !== 0) parts.push(`hue-rotate(${Math.round(p.hueRotate * 360)}deg)`);
	if (p.saturate !== 0.5) parts.push(`saturate(${Math.round(p.saturate * 200)}%)`);
	if (p.brightness !== 0.5) parts.push(`brightness(${Math.round(p.brightness * 200)}%)`);
	if (p.contrast !== 0.5) parts.push(`contrast(${Math.round(p.contrast * 200)}%)`);
	if (p.invert !== 0) parts.push(`invert(${Math.round(p.invert * 100)}%)`);
	return parts.join(' ');
}

export type BlendMode = 'normal' | 'additive' | 'screen' | 'multiply';

export interface SlotComposite {
	blend: BlendMode;
	lumaKey: boolean;
	lumaBlack: number;   // 0..1
	lumaWhite: number;   // 0..1
	colorKey: boolean;
	colorHue: number;    // 0..1 → 0..360°
	colorTol: number;    // 0..1
}

export const DEFAULT_SLOT_COMPOSITE: SlotComposite = {
	blend: 'normal',
	lumaKey: false,
	lumaBlack: 0,
	lumaWhite: 1,
	colorKey: false,
	colorHue: 0,
	colorTol: 0,
};

export type SyncMessage =
	| { type: 'preset'; deck: 'A' | 'B'; name: string; blend?: number }
	| { type: 'preset-slot'; slot: number; name: string; blend?: number }
	| { type: 'crossfader'; value: number }
	| { type: 'deckbus'; opacities: [number, number, number, number] }
	| { type: 'source'; deviceId: string }
	| { type: 'loopback'; deviceId: number }
	| { type: 'quality'; tier: string }
	| { type: 'composite'; slot: number; config: SlotComposite }
	| { type: 'time'; slot: number; params: DeckTimeParams }
	| { type: 'overlays'; list: Overlay[] }
	| { type: 'overlay-queue-index'; index: number }
	| { type: 'video'; enabled: boolean; clip: ClipRef | null; opacity: number; playbackRate: number; flashOn: boolean; hueOn: boolean }
	| { type: 'beat'; bpm: number }
	| { type: 'strobe'; on: boolean; rate: number; intensity: number; color: string }
	| { type: 'color'; deck: 'A' | 'B'; params: ColorParams }
	| { type: 'perf'; targetFps: number; invisibleMode: InvisibleMode; invisibleFps: number }
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

	sendPreset(deck: 'A' | 'B', name: string, blend?: number) {
		sendMsg(this.bc, { type: 'preset', deck, name, blend });
	}

	sendPresetSlot(slot: number, name: string, blend?: number) {
		sendMsg(this.bc, { type: 'preset-slot', slot, name, blend });
	}

	sendDeckBus(opacities: [number, number, number, number]) {
		sendMsg(this.bc, { type: 'deckbus', opacities });
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

	sendComposite(slot: number, config: SlotComposite) {
		sendMsg(this.bc, { type: 'composite', slot, config: { ...config } });
	}

	sendTime(slot: number, params: DeckTimeParams) {
		sendMsg(this.bc, { type: 'time', slot, params: { ...params } });
	}

	sendOverlays(list: Overlay[]) {
		// Shallow copy is load-bearing: a raw Svelte 5 $state-proxied array/object
		// throws DataCloneError on postMessage. Don't simplify back to `{ list }`.
		sendMsg(this.bc, { type: 'overlays', list: list.map((o) => ({ ...o })) });
	}

	sendOverlayQueueIndex(index: number) {
		sendMsg(this.bc, { type: 'overlay-queue-index', index });
	}

	sendVideo(state: { enabled: boolean; clip: ClipRef | null; opacity: number; playbackRate: number; flashOn: boolean; hueOn: boolean }) {
		sendMsg(this.bc, { type: 'video', ...state });
	}

	sendBeat(bpm: number) {
		sendMsg(this.bc, { type: 'beat', bpm });
	}

	sendStrobe(on: boolean, rate: number, intensity: number, color: string) {
		sendMsg(this.bc, { type: 'strobe', on, rate, intensity, color });
	}

	sendColor(deck: 'A' | 'B', params: ColorParams) {
		sendMsg(this.bc, { type: 'color', deck, params: { ...params } });
	}

	sendPerf(settings: { targetFps: number; invisibleMode: InvisibleMode; invisibleFps: number }) {
		sendMsg(this.bc, { type: 'perf', ...settings });
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
