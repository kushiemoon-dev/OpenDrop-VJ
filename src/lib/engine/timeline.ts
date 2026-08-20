/**
 * Timeline — sequences the app's existing 8 snapshot slots across a wall-clock
 * loop. Same split as snapshot.ts: pure, unit-testable functions here; the
 * RAF-owning TimelineEngine class lives alongside them but is not unit tested
 * (verified in a real browser — same precedent as SnapshotEngine/Compositor).
 */

import type { CommandId } from './commands.js'
import type { Snapshot } from './snapshot.js'
import { smoothstep, interpolateSnapshot } from './snapshot.js'

export interface TimelineKeyframe {
  slot: number
  timeSec: number
}

/**
 * Loop length in seconds: the timestamp of the last keyframe. Callers are
 * responsible for keeping `keyframes` sorted by timeSec (see +page.svelte's
 * add/update helpers, which re-sort on every edit). Fewer than 2 keyframes
 * means there's nothing to interpolate between, so the loop has no length.
 */
export function timelineLoopDuration(keyframes: TimelineKeyframe[]): number {
  if (keyframes.length < 2) return 0
  return keyframes[keyframes.length - 1]!.timeSec
}

/**
 * Resolves the interpolated "look" values at a point in time within one loop
 * cycle. `tSec` is assumed already reduced modulo timelineLoopDuration by the
 * caller — TimelineEngine owns the looping/wraparound logic; this function
 * only resolves a position already inside [0, loopDuration).
 *
 * Edge cases (decided):
 *  - Fewer than 2 keyframes → {} (nothing to drive).
 *  - A keyframe referencing an empty snapshot slot (snapshots[slot] === null)
 *    is treated as {} for that endpoint — same absent-key semantics as
 *    interpolateSnapshot itself (a missing key never invents a jump).
 *  - tSec before the first keyframe's timeSec (possible if the user edits
 *    the first keyframe's time away from 0) → progress goes negative,
 *    smoothstep clamps it to 0, so the first keyframe's value is held until
 *    reached. No special-casing needed — smoothstep's existing clamp covers it.
 */
export function timelineValuesAt(
  keyframes: TimelineKeyframe[],
  snapshots: (Snapshot | null)[],
  tSec: number
): Partial<Record<CommandId, number>> {
  if (keyframes.length < 2) return {}

  let i = 0
  while (i < keyframes.length - 2 && tSec >= keyframes[i + 1]!.timeSec) i++
  const from = keyframes[i]!
  const to = keyframes[i + 1]!

  const span = to.timeSec - from.timeSec
  const progress = span <= 0 ? 1 : (tSec - from.timeSec) / span

  const startValues = snapshots[from.slot]?.values ?? {}
  const targetValues = snapshots[to.slot]?.values ?? {}
  return interpolateSnapshot(startValues, targetValues, smoothstep(progress))
}

/**
 * Owns a requestAnimationFrame loop that plays a timeline in a loop. The only
 * instance state is `rafId` and `resumeFromSec` — pause() just cancels the
 * frame, and the next play() reconstructs the elapsed time from the last
 * value `resumeFromSec` was written to, so a resumed playback continues from
 * where it left off rather than restarting at t=0.
 */
export class TimelineEngine {
  private rafId: number | null = null
  private resumeFromSec = 0

  play(
    keyframes: TimelineKeyframe[],
    snapshots: (Snapshot | null)[],
    onTick: (values: Partial<Record<CommandId, number>>) => void
  ): void {
    this.cancelFrame()
    const loopDuration = timelineLoopDuration(keyframes)
    if (loopDuration <= 0) return

    const startTime = performance.now() - this.resumeFromSec * 1000
    const frame = (now: number) => {
      const elapsedSec = (now - startTime) / 1000
      this.resumeFromSec = elapsedSec % loopDuration
      onTick(timelineValuesAt(keyframes, snapshots, this.resumeFromSec))
      this.rafId = requestAnimationFrame(frame)
    }
    this.rafId = requestAnimationFrame(frame)
  }

  pause(): void {
    this.cancelFrame()
  }

  destroy(): void {
    this.cancelFrame()
  }

  private cancelFrame(): void {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId)
      this.rafId = null
    }
  }
}
