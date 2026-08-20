/**
 * beat-sync-store.svelte.ts — reactive wrapper around beat-reactive
 * playback: per-deck beat-sync toggle + lock + trigger config, auto-
 * crossfade, the auto-crossfade cadence, and the shared beat-flash pulse
 * (also consumed by the video layer / overlay layer). Extracted from
 * +page.svelte, same shape as overlay-store.svelte.ts — mutate the exported
 * state object's fields, never reassign the export.
 *
 * `autoXfadeCount`, `tapTimes`, and `volumePeakStateA/B` stay in
 * +page.svelte — cross-tick bookkeeping non-reactive locals, same category
 * as `_lastStrobeVal`/`pausedSlots` in the Performance decks section.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import {
  type BeatTriggerConfig,
  defaultBeatTriggerConfig,
  applyBeatTriggerPatch,
} from './beat-trigger.js'

export const beatSyncState = $state({
  beatSyncA: false,
  beatSyncB: false,
  beatsPerChange: 8,
  beatTriggerA: defaultBeatTriggerConfig() as BeatTriggerConfig,
  beatTriggerB: defaultBeatTriggerConfig() as BeatTriggerConfig,
  autoXfade: false,
  lockA: false,
  lockB: false,
  beat: false,
})

export function updateBeatTriggerA(patch: Partial<BeatTriggerConfig>): void {
  beatSyncState.beatTriggerA = applyBeatTriggerPatch(beatSyncState.beatTriggerA, patch)
}

export function updateBeatTriggerB(patch: Partial<BeatTriggerConfig>): void {
  beatSyncState.beatTriggerB = applyBeatTriggerPatch(beatSyncState.beatTriggerB, patch)
}
