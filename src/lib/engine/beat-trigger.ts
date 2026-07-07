/**
 * Playlist beat/volume trigger refinement (1.5) — per-deck threshold, offset,
 * and an alternative volume-peak trigger, replacing the single shared
 * beatsPerChange that used to drive playlist A/B beat-sync (autoXfade keeps
 * using the original shared beatsPerChange unchanged — separate feature).
 */

export interface BeatTriggerConfig {
	mode: 'beat' | 'volume-peak';
	beatsPerChange: number; // 1..64
	offset: number;         // 0..beatsPerChange-1
	sensitivity: number;    // 0..1, used only in volume-peak mode
}

export function defaultBeatTriggerConfig(): BeatTriggerConfig {
	return { mode: 'beat', beatsPerChange: 8, offset: 0, sensitivity: 0.5 };
}

export function shouldTriggerOnBeat(beatCount: number, config: BeatTriggerConfig): boolean {
	if (config.mode !== 'beat') return false;
	return ((beatCount + config.offset) % config.beatsPerChange) === 0;
}

export interface VolumePeakState {
	rollingAvg: number;
	lastTriggerAt: number; // ms timestamp, for cooldown
}

export function defaultVolumePeakState(): VolumePeakState {
	return { rollingAvg: 0, lastTriggerAt: -Infinity };
}

const COOLDOWN_MS = 500;
const SMOOTHING = 0.05;
const SILENCE_FLOOR = 0.02;

export function detectVolumePeak(
	rms: number,
	state: VolumePeakState,
	sensitivity: number,
	nowMs: number,
): { triggered: boolean; next: VolumePeakState } {
	const nextAvg = state.rollingAvg * (1 - SMOOTHING) + rms * SMOOTHING;
	const thresholdMult = 1.3 + sensitivity * 1.7;
	const cooledDown = nowMs - state.lastTriggerAt >= COOLDOWN_MS;
	const triggered = cooledDown && nextAvg > SILENCE_FLOOR && rms > nextAvg * thresholdMult;
	return {
		triggered,
		next: { rollingAvg: nextAvg, lastTriggerAt: triggered ? nowMs : state.lastTriggerAt },
	};
}

export function clampBeatsPerChange(n: number): number {
	return Math.max(1, Math.min(64, Math.round(n)));
}

export function clampOffset(offset: number, beatsPerChange: number): number {
	return Math.max(0, Math.min(beatsPerChange - 1, Math.round(offset)));
}

/**
 * Merge a patch into a BeatTriggerConfig, re-clamping beatsPerChange/offset so
 * they stay valid together (offset depends on the NEW beatsPerChange, not the
 * old one). Pure — returns a new object. Same merge-then-clamp shape used for
 * beatTriggerA, beatTriggerB, and the overlay queue trigger.
 */
export function applyBeatTriggerPatch(current: BeatTriggerConfig, patch: Partial<BeatTriggerConfig>): BeatTriggerConfig {
	const next = { ...current, ...patch };
	next.beatsPerChange = clampBeatsPerChange(next.beatsPerChange);
	next.offset = clampOffset(next.offset, next.beatsPerChange);
	return next;
}
