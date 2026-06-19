import { Deck, type DeckOptions } from './deck.js';

type SlotEntry = { deck: Deck; canvas: HTMLCanvasElement } | null;

/**
 * DeckManager — gère N slots Butterchurn avec lazy init et pause/resume.
 *
 * Un slot est instancié au premier start() et ne libère jamais son
 * contexte WebGL (pause/resume). Plafond 4 slots = sous la limite browser.
 */
export class DeckManager {
	private slots: SlotEntry[] = [null, null, null, null];
	private canvases: (HTMLCanvasElement | null)[] = [null, null, null, null];
	private audioNode: AudioNode | null = null;
	private _targetFps = 0;  // 0 = illimité

	attachCanvas(slot: number, canvas: HTMLCanvasElement): void {
		this.canvases[slot] = canvas;
	}

	/**
	 * Démarre ou reprend un slot.
	 * - Premier appel : crée et initialise un Deck (coûteux).
	 * - Appels suivants : appelle deck.resume() (instantané, zéro fuite WebGL).
	 */
	async start(
		slot: number,
		audioCtx: AudioContext,
		audioNode: AudioNode,
		quality: DeckOptions,
		presetData: object | null
	): Promise<void> {
		this.audioNode = audioNode;
		const existing = this.slots[slot];
		if (existing) {
			existing.deck.resume();
			return;
		}
		const canvas = this.canvases[slot];
		if (!canvas) throw new Error(`DeckManager: no canvas attached for slot ${slot}`);
		const deck = new Deck(canvas, `deck-${slot}`);
		const w = canvas.clientWidth || 1280;
		const h = canvas.clientHeight || 720;
		await deck.init(audioCtx, { width: w, height: h, ...quality });
		deck.connectAudio(audioNode);
		if (presetData) deck.loadPreset(presetData, 0.0);
		deck.startRenderLoop();
		deck.setTargetFps(this._targetFps);
		this.slots[slot] = { deck, canvas };
	}

	pause(slot: number): void {
		this.slots[slot]?.deck.pause();
	}

	isRunning(slot: number): boolean {
		return this.slots[slot]?.deck.state === 'running';
	}

	loadPreset(slot: number, data: object, blend = 2.0): void {
		this.slots[slot]?.deck.loadPreset(data, blend);
	}

	/** Re-route l'audio vers TOUS les slots initialisés (ex: switch source/loopback). */
	connectAudio(node: AudioNode): void {
		this.audioNode = node;
		for (const slot of this.slots) {
			slot?.deck.connectAudio(node);
		}
	}

	applyQuality(opts: {
		meshWidth: number;
		meshHeight: number;
		pixelRatio: number;
		textureRatio: number;
		outputFXAA: boolean;
	}): void {
		for (const slot of this.slots) {
			slot?.deck.applyQuality(opts);
		}
	}

	/**
	 * Set a global FPS cap applied to all current and future slots.
	 * @param fps  Target frames per second. 0 = unlimited.
	 */
	setTargetFps(fps: number): void {
		this._targetFps = fps;
		for (const slot of this.slots) slot?.deck.setTargetFps(fps);
	}

	/**
	 * Set a per-slot FPS cap without changing the global default.
	 * @param slot  Slot index (0–3).
	 * @param fps   Target frames per second. 0 = unlimited.
	 */
	setSlotTargetFps(slot: number, fps: number): void {
		this.slots[slot]?.deck.setTargetFps(fps);
	}

	resize(slot: number, w: number, h: number): void {
		this.slots[slot]?.deck.resize(w, h);
	}

	runningCount(): number {
		return this.slots.filter((s) => s?.deck.state === 'running').length;
	}

	destroyAll(): void {
		for (const slot of this.slots) {
			slot?.deck.destroy();
		}
		this.slots = [null, null, null, null];
	}
}
