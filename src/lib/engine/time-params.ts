/**
 * Time param sliders per deck (1.4) — engine-level multipliers applied on
 * top of whatever a loaded preset's own code computes, matching NestDrop's
 * "Time" controls. Butterchurn compiles each preset's frame_eqs_str into a
 * `new Function` at every loadPreset() call — a `new Function` body can only
 * see global scope, never a local closure (standard JS, not a Butterchurn
 * limitation), so the live-adjustable multiplier values must live on a
 * global, namespaced per deck slot to avoid collisions between decks.
 */

export interface DeckTimeParams {
  speedMult: number
  zoomMult: number
  rotMult: number
  warpMult: number
  dxMult: number
  dyMult: number
  stretchMult: number
  waveMult: number
}

export function defaultTimeParams(): DeckTimeParams {
  return {
    speedMult: 1,
    zoomMult: 1,
    rotMult: 1,
    warpMult: 1,
    dxMult: 1,
    dyMult: 1,
    stretchMult: 1,
    waveMult: 1,
  }
}

/** Time params for the 4 decks, indexed 0-3. */
export type TimeParamsTuple = [DeckTimeParams, DeckTimeParams, DeckTimeParams, DeckTimeParams]

/**
 * Merge a patch into one slot's DeckTimeParams. Pure — returns a new tuple.
 * Does not write through to getGlobalTimeParams(); callers driving the
 * Butterchurn-visible global must still do that side effect themselves.
 */
export function withTimeParams(
  params: TimeParamsTuple,
  slot: number,
  patch: Partial<DeckTimeParams>
): TimeParamsTuple {
  const next = [...params] as TimeParamsTuple
  next[slot] = { ...next[slot]!, ...patch }
  return next
}

/**
 * Shallow-clones `preset` and appends/prepends lines to its frame_eqs_str
 * that reference window.__odDeckParams[slot].*mult — pure string
 * manipulation, no window access here, so this stays unit-testable without
 * a DOM. The 8 lines are injected unconditionally on every call: no
 * "which variables are active" state to manage, every slider is always
 * live-adjustable without a preset reload.
 */
export function injectTimeParams(preset: object, slot: number): object {
  const patched: Record<string, unknown> = { ...preset }
  const original = typeof patched.frame_eqs_str === 'string' ? patched.frame_eqs_str : ''
  const p = `window.__odDeckParams[${slot}]`
  patched.frame_eqs_str =
    `a.time = a.time * ${p}.speedMult;\n` +
    original +
    `\na.zoom = 1 + (a.zoom - 1) * ${p}.zoomMult;` +
    `\na.rot = a.rot * ${p}.rotMult;` +
    `\na.warp = a.warp * ${p}.warpMult;` +
    `\na.dx = a.dx * ${p}.dxMult;` +
    `\na.dy = a.dy * ${p}.dyMult;` +
    `\na.sx = 1 + (a.sx - 1) * ${p}.stretchMult;` +
    `\na.sy = 1 + (a.sy - 1) * ${p}.stretchMult;` +
    `\na.wave_a = a.wave_a * ${p}.waveMult;`
  return patched
}

/**
 * Lazily initializes and returns the global per-slot params array Butterchurn's
 * compiled preset code reads from. Not unit tested (touches `window`) —
 * verified in a real browser, same precedent as Compositor/SnapshotEngine.
 */
export function getGlobalTimeParams(): DeckTimeParams[] {
  const w = window as unknown as { __odDeckParams?: DeckTimeParams[] }
  if (!w.__odDeckParams) {
    w.__odDeckParams = [
      defaultTimeParams(),
      defaultTimeParams(),
      defaultTimeParams(),
      defaultTimeParams(),
    ]
  }
  return w.__odDeckParams
}

// Eager init at module load, in whichever window (control or output) imports this
// module — both do, transitively, via deck-manager.ts. Ensures the compiled preset
// code's window.__odDeckParams[slot] reference is never undefined, even before the
// first slider touch or localStorage restore.
if (typeof window !== 'undefined') getGlobalTimeParams()
