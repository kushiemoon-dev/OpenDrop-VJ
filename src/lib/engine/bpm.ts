export class BeatDetector {
	private rafId: number | null = null;
	private energyHistory: number[] = new Array(43).fill(0);
	private beatIntervals: number[] = [];
	private lastBeatTime = 0;
	private _bpm = 0;
	private _active = false;
	private cb: (() => void) | null = null;
	private readonly fftData: Uint8Array<ArrayBuffer>;

	constructor(private readonly analyser: AnalyserNode) {
		this.fftData = new Uint8Array(analyser.frequencyBinCount) as Uint8Array<ArrayBuffer>;
	}

	get bpm() { return this._bpm; }
	get active() { return this._active; }

	start(onBeat: () => void) {
		if (this._active) return;
		this.cb = onBeat;
		this._active = true;
		this._tick();
	}

	stop() {
		this._active = false;
		if (this.rafId !== null) {
			cancelAnimationFrame(this.rafId);
			this.rafId = null;
		}
	}

	destroy() { this.stop(); }

	private _tick() {
		if (!this._active) return;
		this.rafId = requestAnimationFrame(() => this._tick());

		this.analyser.getByteFrequencyData(this.fftData);

		// Bass energy: first 5% of bins (~0-500Hz for fftSize=2048)
		const bassEnd = Math.max(1, Math.floor(this.fftData.length * 0.05));
		let energy = 0;
		for (let i = 0; i < bassEnd; i++) energy += this.fftData[i] ** 2;
		energy = Math.sqrt(energy / bassEnd);

		this.energyHistory.push(energy);
		if (this.energyHistory.length > 43) this.energyHistory.shift();

		const avg = this.energyHistory.reduce((s, v) => s + v, 0) / this.energyHistory.length;
		const now = performance.now();

		// Beat detected when energy exceeds 1.35x the rolling average, the signal isn't silent, and the 300ms cooldown has elapsed
		if (energy > avg * 1.35 && avg > 8 && now - this.lastBeatTime > 300) {
			const interval = now - this.lastBeatTime;
			this.lastBeatTime = now;

			// Only accumulate the interval if it's plausible (60-220 BPM)
			if (interval > 270 && interval < 1000) {
				this.beatIntervals.push(interval);
				if (this.beatIntervals.length > 8) this.beatIntervals.shift();
				if (this.beatIntervals.length >= 2) {
					const avgInterval = this.beatIntervals.reduce((s, v) => s + v, 0) / this.beatIntervals.length;
					const bpm = Math.round(60000 / avgInterval);
					if (bpm >= 60 && bpm <= 220) this._bpm = bpm;
				}
			}

			this.cb?.();
		}
	}
}
