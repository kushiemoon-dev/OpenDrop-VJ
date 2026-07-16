/**
 * share-set-actions.ts — build/copy a shareable set-by-URL, and apply one
 * decoded from an incoming #share= link. Extracted from +page.svelte — pure
 * orchestration touching the DeckManager/MainSync/clipboard browser-facing
 * APIs, never unit tested in this codebase (same precedent as the other
 * *-actions.ts modules).
 */

import type { DeckManager } from './deck-manager.js';
import type { MainSync } from './sync.js';
import { type SharedSet, filterShareableOverlays, encodeSharedSet } from './share-set.js';
import { deckState } from './deck-store.svelte.js';
import { colorState } from './color-store.svelte.js';
import { compositingState } from './compositing-store.svelte.js';
import { timeParamsState } from './time-params-store.svelte.js';
import { getGlobalTimeParams } from './time-params.js';
import { qvarState } from './q-vars-store.svelte.js';
import { getGlobalQVarParams } from './q-vars.js';
import { snapshotsState } from './snapshots-store.svelte.js';
import { timelineState } from './timeline-store.svelte.js';
import { overlayState } from './overlay-store.svelte.js';
import { beatSyncState } from './beat-sync-store.svelte.js';
import { runStatusState } from './run-status-store.svelte.js';
import { shareSetState } from './share-set-store.svelte.js';
import { loadPresetData } from '../presets/index.js';

export async function selectPresetForDeck(deck: 'A' | 'B', name: string, manager: DeckManager, sync: MainSync | null): Promise<void> {
	const d = await loadPresetData(name);
	if (!d) return;
	if (deck === 'A') {
		deckState.presetA = name;
		manager.loadPreset(0, d, deckState.transitionTime);
		sync?.sendPreset('A', name, deckState.transitionTime);
	} else {
		deckState.presetB = name;
		manager.loadPreset(1, d, deckState.transitionTime);
		sync?.sendPreset('B', name, deckState.transitionTime);
	}
}

export function buildCurrentSharedSet(): SharedSet {
	return {
		version: 1,
		name: shareSetState.name,
		presetA: deckState.presetA, presetB: deckState.presetB,
		deckBus: deckState.deckBus,
		crossfader: deckState.crossfader, transitionTime: deckState.transitionTime,
		colorParamsA: colorState.a, colorParamsB: colorState.b,
		slotComposites: compositingState.slotComposites,
		timeParams: timeParamsState.params,
		qVarParams: qvarState.params,
		snapshots: snapshotsState.snapshots,
		snapshotRecallDuration: snapshotsState.recallDuration,
		timelineKeyframes: timelineState.keyframes,
		overlays: overlayState.overlays,
		beatTriggerA: beatSyncState.beatTriggerA, beatTriggerB: beatSyncState.beatTriggerB,
		beatSyncA: beatSyncState.beatSyncA, beatSyncB: beatSyncState.beatSyncB,
		overlayQueueEnabled: overlayState.queueEnabled, overlayQueueTrigger: overlayState.queueTrigger,
	};
}

export async function copyShareLink(): Promise<void> {
	const set = buildCurrentSharedSet();
	set.overlays = filterShareableOverlays(set.overlays);
	const encoded = await encodeSharedSet(set);
	const url = `${location.origin}${location.pathname}#share=${encoded}`;
	await navigator.clipboard.writeText(url);
	shareSetState.copyLabel = 'Copied!';
	setTimeout(() => { shareSetState.copyLabel = 'Copy link'; }, 1500);
}

export async function applyPendingSharedSet(manager: DeckManager, sync: MainSync | null): Promise<void> {
	if (!shareSetState.pending) return;
	const s = shareSetState.pending;
	deckState.deckBus = s.deckBus;
	deckState.crossfader = s.crossfader; deckState.transitionTime = s.transitionTime;
	colorState.a = s.colorParamsA; colorState.b = s.colorParamsB;
	compositingState.slotComposites = s.slotComposites;
	timeParamsState.params = s.timeParams as typeof timeParamsState.params;
	for (let slot = 0; slot < 4; slot++) Object.assign(getGlobalTimeParams()[slot], timeParamsState.params[slot]);
	qvarState.params = s.qVarParams as typeof qvarState.params;
	for (let slot = 0; slot < 4; slot++) Object.assign(getGlobalQVarParams()[slot], { enabled: [...qvarState.params[slot].enabled], value: [...qvarState.params[slot].value] });
	snapshotsState.snapshots = s.snapshots; snapshotsState.recallDuration = s.snapshotRecallDuration;
	timelineState.keyframes = s.timelineKeyframes;
	overlayState.overlays = s.overlays;
	beatSyncState.beatTriggerA = s.beatTriggerA; beatSyncState.beatTriggerB = s.beatTriggerB;
	beatSyncState.beatSyncA = s.beatSyncA; beatSyncState.beatSyncB = s.beatSyncB;
	overlayState.queueEnabled = s.overlayQueueEnabled; overlayState.queueTrigger = s.overlayQueueTrigger;

	if (runStatusState.status === 'running') {
		await selectPresetForDeck('A', s.presetA, manager, sync);
		await selectPresetForDeck('B', s.presetB, manager, sync);
	} else {
		deckState.presetA = s.presetA;
		deckState.presetB = s.presetB;
	}
	shareSetState.pending = null;
}

export function cancelPendingSharedSet(): void {
	shareSetState.pending = null;
}
