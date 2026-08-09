/**
 * beat-tempo-actions.ts — per-beat reactive dispatch (overlay/video pulse,
 * auto-crossfade, playlist beat-sync, overlay queue advance) and manual
 * tempo control (tap tempo / clear). Extracted from +page.svelte — pure
 * orchestration touching the Clock/MainSync browser-facing instances, never
 * unit tested in this codebase (same precedent as the other *-actions.ts
 * modules).
 *
 * `applyMidiAction` is passed in rather than imported — it stays in
 * +page.svelte because it needs `registry`/`commandCtx`/`runStatusState`.
 *
 * `autoXfadeCount`/`tapTimes` are module-private cross-tick bookkeeping —
 * same category as `_lastStrobeVal`/`pausedSlots` in +page.svelte.
 */

import type { Clock } from './clock.js';
import type { MainSync } from './sync.js';
import type { CommandId } from './commands.js';
import { shouldTriggerOnBeat } from './beat-trigger.js';
import { beatSyncState } from './beat-sync-store.svelte.js';
import { deckState } from './deck-store.svelte.js';
import { audioSourceState } from './audio-source-store.svelte.js';
import { playlistState, setPlaylistBeatSyncInterval, playlistNext } from './playlist-store.svelte.js';
import { overlayState, advanceOverlayQueue } from './overlay-store.svelte.js';
import { onVideoBeat } from '../video-loops/playback-store.svelte.js';

let autoXfadeCount = 0;
let tapTimes: number[] = [];

export function onBeat(sync: MainSync | null, clock: Clock, applyMidiAction: (id: CommandId, value: number) => void): void {
	// Pulse overlay beat-reactive
	beatSyncState.beat = true;
	setTimeout(() => { beatSyncState.beat = false; }, 80);
	sync?.sendBeat(clock.bpm || audioSourceState.detectedBpm);

	onVideoBeat();

	if (beatSyncState.autoXfade) {
		autoXfadeCount = (autoXfadeCount + 1) % beatSyncState.beatsPerChange;
		if (autoXfadeCount === 0) {
			deckState.crossfader = deckState.crossfader < 0.5 ? 1 : 0;
			sync?.sendCrossfader(deckState.crossfader);
		}
	}
	if (beatSyncState.beatSyncA && !beatSyncState.lockA && shouldTriggerOnBeat(clock.beatCount, beatSyncState.beatTriggerA)) {
		if (playlistState.aItems.length > 0) playlistNext('A');
		else applyMidiAction('preset-next-a', 127);
	}
	if (beatSyncState.beatSyncB && !beatSyncState.lockB && shouldTriggerOnBeat(clock.beatCount, beatSyncState.beatTriggerB)) {
		if (playlistState.bItems.length > 0) playlistNext('B');
		else applyMidiAction('preset-next-b', 127);
	}
	if (overlayState.queueEnabled && shouldTriggerOnBeat(clock.beatCount, overlayState.queueTrigger)) {
		advanceOverlayQueue(1);
	}
}

export function resetAutoXfadeCount(): void {
	autoXfadeCount = 0;
}

export function toggleBeatSync(deck: 'A' | 'B'): void {
	if (deck === 'A') {
		beatSyncState.beatSyncA = !beatSyncState.beatSyncA;
		setPlaylistBeatSyncInterval('A', beatSyncState.beatSyncA ? Infinity : playlistState.intervalSec * 1000);
	} else {
		beatSyncState.beatSyncB = !beatSyncState.beatSyncB;
		setPlaylistBeatSyncInterval('B', beatSyncState.beatSyncB ? Infinity : playlistState.intervalSec * 1000);
	}
}

/** Shared by tapTempo (computed from tap intervals) and setManualBpm (typed directly). */
function applyManualBpm(clock: Clock, bpm: number): void {
	if (bpm < 20 || bpm > 300) return;
	audioSourceState.manualBpm = bpm;
	clock.setBpm(bpm);
	clock.pulse();
}

export function tapTempo(clock: Clock): void {
	const now = performance.now();
	tapTimes.push(now);
	if (tapTimes.length > 4) tapTimes = tapTimes.slice(-4);
	if (tapTimes.length < 2) return;
	const intervals = tapTimes.slice(1).map((t, i) => t - tapTimes[i]);
	const avg = intervals.reduce((s, v) => s + v, 0) / intervals.length;
	applyManualBpm(clock, Math.round(60000 / avg));
}

/** Type a BPM directly (keyboard entry) instead of tapping it out. Same 20-300 range as tapTempo. */
export function setManualBpm(clock: Clock, bpm: number): void {
	tapTimes = [];
	applyManualBpm(clock, Math.round(bpm));
}

export function clearManualBpm(clock: Clock): void {
	audioSourceState.manualBpm = 0;
	tapTimes = [];
	clock.setBpm(0);
}
