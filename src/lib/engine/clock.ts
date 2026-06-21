type BeatCb = () => void;
type TickCb = (phase01: number, beatCount: number) => void;

export class Clock {
	private _bpm = 0;
	private _phase01 = 0;
	private _beatCount = 0;
	private _rafId: number | null = null;
	private _lastTs: number | null = null;
	private readonly _beatCbs: Set<BeatCb> = new Set();
	private readonly _tickCbs: Set<TickCb> = new Set();

	get bpm(): number { return this._bpm; }
	get phase01(): number { return this._phase01; }
	get beatCount(): number { return this._beatCount; }

	/** Subscribe to beat events. Returns an unsubscribe function. */
	onBeat(cb: BeatCb): () => void {
		this._beatCbs.add(cb);
		return () => this._beatCbs.delete(cb);
	}

	/** Subscribe to every RAF tick (called with current phase and beat count). */
	onTick(cb: TickCb): () => void {
		this._tickCbs.add(cb);
		return () => this._tickCbs.delete(cb);
	}

	/** Set the target BPM. Pass 0 to switch to pulse-only mode (audio-reactive). */
	setBpm(bpm: number): void {
		this._bpm = Math.max(0, Math.min(300, bpm));
	}

	/**
	 * Sync phase to 0 — called by external sources (audio detector, tap-tempo).
	 * In pulse-only mode (bpm === 0) also emits a beat immediately.
	 */
	pulse(bpm?: number): void {
		if (bpm !== undefined) this.setBpm(bpm);
		this._phase01 = 0;
		if (this._bpm === 0) this._emitBeat();
	}

	start(): void {
		if (this._rafId !== null) return;
		this._lastTs = null;
		const tick = (ts: number) => {
			if (this._lastTs !== null && this._bpm > 0) {
				// Clamp dt to avoid large jumps on tab-visibility change
				const dt = Math.min((ts - this._lastTs) / 1000, 0.1);
				this._phase01 += dt * this._bpm / 60;
				while (this._phase01 >= 1) {
					this._phase01 -= 1;
					this._emitBeat();
				}
			}
			this._lastTs = ts;
			for (const cb of this._tickCbs) cb(this._phase01, this._beatCount);
			this._rafId = requestAnimationFrame(tick);
		};
		this._rafId = requestAnimationFrame(tick);
	}

	stop(): void {
		if (this._rafId !== null) {
			cancelAnimationFrame(this._rafId);
			this._rafId = null;
		}
		this._lastTs = null;
	}

	/** Step the clock manually by dt seconds — for unit testing without RAF. */
	_stepForTest(dtSeconds: number): void {
		if (this._bpm > 0) {
			this._phase01 += dtSeconds * this._bpm / 60;
			while (this._phase01 >= 1) {
				this._phase01 -= 1;
				this._emitBeat();
			}
		}
		for (const cb of this._tickCbs) cb(this._phase01, this._beatCount);
	}

	private _emitBeat(): void {
		this._beatCount++;
		for (const cb of this._beatCbs) cb();
	}
}
