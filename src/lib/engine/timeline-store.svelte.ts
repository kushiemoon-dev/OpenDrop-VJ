/**
 * timeline-store.svelte.ts — reactive wrapper around the Timeline
 * keyframe-playback sequencer (Track 2). Extracted from +page.svelte, same
 * shape as overlay-store.svelte.ts — mutate the exported state object's
 * fields, never reassign the export.
 *
 * The RAF-owning `TimelineEngine` instance and the $effect piloting its
 * play()/pause() stay in +page.svelte — no existing precedent in this
 * codebase for a browser-API/RAF-owning class living in a .svelte.ts store
 * (same reasoning as SnapshotEngine/Compositor staying local).
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import { type TimelineKeyframe, timelineLoopDuration } from './timeline.js'
import { snapshotsState } from './snapshots-store.svelte.js'

export const timelineState = $state({
  keyframes: [] as TimelineKeyframe[],
  playing: false,
})

export function toggleTimelinePlay(): void {
  if (timelineState.playing) {
    timelineState.playing = false
    return
  }
  if (timelineLoopDuration(timelineState.keyframes) <= 0) return // nothing to interpolate, silent no-op
  timelineState.playing = true
}

export function addTimelineKeyframe(): void {
  const firstFilledSlot = snapshotsState.snapshots.findIndex((s) => s !== null)
  const lastTime =
    timelineState.keyframes.length > 0
      ? timelineState.keyframes[timelineState.keyframes.length - 1]!.timeSec
      : -5
  timelineState.keyframes = [
    ...timelineState.keyframes,
    { slot: firstFilledSlot >= 0 ? firstFilledSlot : 0, timeSec: lastTime + 5 },
  ].sort((a, b) => a.timeSec - b.timeSec)
}

export function removeTimelineKeyframe(index: number): void {
  timelineState.keyframes = timelineState.keyframes.filter((_, i) => i !== index)
}

export function updateTimelineKeyframe(index: number, patch: Partial<TimelineKeyframe>): void {
  timelineState.keyframes = timelineState.keyframes
    .map((kf, i) => (i === index ? { ...kf, ...patch } : kf))
    .sort((a, b) => a.timeSec - b.timeSec)
}
