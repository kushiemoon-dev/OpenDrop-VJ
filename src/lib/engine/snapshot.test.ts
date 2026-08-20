import { describe, it, expect } from 'vitest'
import { smoothstep, interpolateSnapshot } from './snapshot.js'
import type { CommandId } from './commands.js'

describe('smoothstep', () => {
  it('equals 0 at t=0 and 1 at t=1', () => {
    expect(smoothstep(0)).toBe(0)
    expect(smoothstep(1)).toBe(1)
  })
  it('equals 0.5 at the midpoint (t=0.5)', () => {
    expect(smoothstep(0.5)).toBeCloseTo(0.5)
  })
  it('is NOT linear: t=0.25 does not give 0.25', () => {
    expect(smoothstep(0.25)).toBeCloseTo(0.15625) // 0.25²·(3−0.5)
    expect(smoothstep(0.25)).not.toBe(0.25)
  })
  it('is symmetric: f(t) + f(1−t) = 1', () => {
    expect(smoothstep(0.25) + smoothstep(0.75)).toBeCloseTo(1)
  })
  it('clamps out of bounds', () => {
    expect(smoothstep(-1)).toBe(0)
    expect(smoothstep(2)).toBe(1)
  })
})

describe('interpolateSnapshot', () => {
  const A = 'color-hue-a' as CommandId
  const B = 'composite-blend-0' as CommandId

  it('progress 0 → returns the starting values', () => {
    expect(interpolateSnapshot({ [A]: 0, [B]: 1 }, { [A]: 1, [B]: 0 }, 0)).toEqual({
      [A]: 0,
      [B]: 1,
    })
  })
  it('progress 1 → returns exactly the target', () => {
    expect(interpolateSnapshot({ [A]: 0, [B]: 1 }, { [A]: 1, [B]: 0 }, 1)).toEqual({
      [A]: 1,
      [B]: 0,
    })
  })
  it('progress 0.5 → midpoint per key', () => {
    expect(interpolateSnapshot({ [A]: 0 }, { [A]: 1 }, 0.5)).toEqual({ [A]: 0.5 })
  })
  it('key absent from start → starts from the target (no movement)', () => {
    expect(interpolateSnapshot({}, { [A]: 0.8 }, 0.5)).toEqual({ [A]: 0.8 })
  })
  it('key absent from the target → ignored (crossfader never driven)', () => {
    // A is present on both sides (interpolates normally); crossfader is present
    // only at the start (absent from the target) → it should be ignored, not
    // just "unchanged since the start".
    const out = interpolateSnapshot({ [A]: 0, ['crossfader' as CommandId]: 0.2 }, { [A]: 1 }, 0.5)
    expect(out).toEqual({ [A]: 0.5 })
    expect('crossfader' in out).toBe(false)
  })
})
