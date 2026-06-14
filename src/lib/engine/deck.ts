/**
 * Deck — wraps a single Butterchurn visualizer instance.
 *
 * One Deck = one Butterchurn instance on one HTMLCanvasElement.
 * The Deck renders into its canvas; the Compositor reads those
 * pixels to build the final output frame.
 *
 * butterchurn is a browser-only UMD module; deck.ts is only ever
 * imported from components that run in the browser (ssr: false).
 */

// Static import — safe because ssr: false disables server-side execution.
// butterchurn is a webpack-in-UMD bundle; Vite wraps it as { default: Visualizer }.
// The real API object may be on .default.default due to double-wrapping.
import _butterchurn from 'butterchurn';
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const bc = (_butterchurn as any).createVisualizer
	? _butterchurn
	: // eslint-disable-next-line @typescript-eslint/no-explicit-any
		((_butterchurn as any).default ?? _butterchurn);

export type DeckState = 'idle' | 'running' | 'stopped';

export interface DeckOptions {
	width?: number;
	height?: number;
	meshWidth?: number;
	meshHeight?: number;
}

export class Deck {
	private viz: import('butterchurn').Visualizer | null = null; // typed via src/lib/types/butterchurn.d.ts
	private rafId: number | null = null;
	private _state: DeckState = 'idle';

	readonly canvas: HTMLCanvasElement;
	readonly id: string;

	get state(): DeckState {
		return this._state;
	}

	constructor(canvas: HTMLCanvasElement, id: string) {
		this.canvas = canvas;
		this.id = id;
	}

	/**
	 * Initialize Butterchurn on this deck's canvas.
	 * Must be called once an AudioContext exists (user gesture).
	 */
	async init(audioCtx: AudioContext, opts: DeckOptions = {}): Promise<void> {
		const w = opts.width ?? (this.canvas.clientWidth || 800);
		const h = opts.height ?? (this.canvas.clientHeight || 600);

		this.canvas.width = w;
		this.canvas.height = h;

		this.viz = bc.createVisualizer(audioCtx, this.canvas, {
			width: w,
			height: h,
			meshWidth: opts.meshWidth ?? 32,
			meshHeight: opts.meshHeight ?? 24
		});

		this._state = 'running';
	}

	/**
	 * Connect an audio source node (analyser, gain, etc.) to this deck.
	 */
	connectAudio(node: AudioNode): void {
		this.viz?.connectAudio(node);
	}

	/**
	 * Load a Butterchurn preset object.
	 * @param preset  The preset JSON object (from butterchurn-presets or converted).
	 * @param blend   Blend time in seconds (0 = hard cut, 2 = soft transition).
	 */
	loadPreset(preset: object, blend = 2.0): void {
		this.viz?.loadPreset(preset, blend);
	}

	/**
	 * Render a single frame. Call in a requestAnimationFrame loop.
	 */
	render(): void {
		this.viz?.render();
	}

	/**
	 * Start the render loop on this deck.
	 */
	startRenderLoop(): void {
		if (this.rafId !== null) return;
		const loop = () => {
			if (this._state !== 'running') return;
			this.render();
			this.rafId = requestAnimationFrame(loop);
		};
		this.rafId = requestAnimationFrame(loop);
	}

	/**
	 * Stop the render loop without destroying the instance.
	 */
	pause(): void {
		if (this.rafId !== null) {
			cancelAnimationFrame(this.rafId);
			this.rafId = null;
		}
		this._state = 'idle';
	}

	/**
	 * Resize the renderer. Call on window resize.
	 */
	resize(width: number, height: number): void {
		this.canvas.width = width;
		this.canvas.height = height;
		this.viz?.setRendererSize(width, height);
	}

	/**
	 * Destroy the deck and release resources.
	 */
	destroy(): void {
		this.pause();
		this._state = 'stopped';
		this.viz = null;
	}
}
