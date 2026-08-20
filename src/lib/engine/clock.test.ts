import { describe, it, expect, vi } from 'vitest'
import { Clock } from './clock.js'

describe('Clock — setBpm + phase advancement', () => {
  it('phase advances proportionally to BPM', () => {
    const clock = new Clock()
    clock.setBpm(120) // 2 beats/sec, phase += 2*dt
    clock._stepForTest(0.25) // dt=0.25s → phase += 0.5
    expect(clock.phase01).toBeCloseTo(0.5)
  })

  it('phase stays at 0 if bpm=0', () => {
    const clock = new Clock()
    clock._stepForTest(1)
    expect(clock.phase01).toBe(0)
    expect(clock.beatCount).toBe(0)
  })

  it('emits a beat when phase exceeds 1', () => {
    const clock = new Clock()
    clock.setBpm(120)
    const cb = vi.fn()
    clock.onBeat(cb)
    clock._stepForTest(0.5) // phase → 1.0 → 0.0, beat emitted
    expect(cb).toHaveBeenCalledTimes(1)
    expect(clock.beatCount).toBe(1)
    expect(clock.phase01).toBeCloseTo(0)
  })

  it('emits multiple beats in a single large step', () => {
    const clock = new Clock()
    clock.setBpm(120) // 2 beats/sec
    const cb = vi.fn()
    clock.onBeat(cb)
    clock._stepForTest(2.5) // 2.5s × 2bps = 5 beats, phase stays at 0
    expect(cb).toHaveBeenCalledTimes(5)
    expect(clock.beatCount).toBe(5)
    expect(clock.phase01).toBeCloseTo(0)
  })

  it('setBpm clamp 0-300', () => {
    const clock = new Clock()
    clock.setBpm(-10)
    expect(clock.bpm).toBe(0)
    clock.setBpm(500)
    expect(clock.bpm).toBe(300)
  })
})

describe('Clock — pulse', () => {
  it('pulse(bpm) updates the BPM and resets phase to 0', () => {
    const clock = new Clock()
    clock.setBpm(120)
    clock._stepForTest(0.3)
    expect(clock.phase01).toBeGreaterThan(0)
    clock.pulse(140)
    expect(clock.bpm).toBe(140)
    expect(clock.phase01).toBe(0)
  })

  it('pulse without bpm in bpm=0 mode emits an immediate beat', () => {
    const clock = new Clock()
    const cb = vi.fn()
    clock.onBeat(cb)
    clock.pulse() // bpm=0 → emits immediately
    expect(cb).toHaveBeenCalledTimes(1)
  })

  it('pulse with bpm>0 does not double-emit (the RAF emits)', () => {
    const clock = new Clock()
    clock.setBpm(120)
    const cb = vi.fn()
    clock.onBeat(cb)
    clock.pulse(120) // phase resync only, no immediate emission
    expect(cb).not.toHaveBeenCalled()
  })
})

describe('Clock — onBeat unsubscribe', () => {
  it('unsub removes the listener', () => {
    const clock = new Clock()
    clock.setBpm(120)
    const cb = vi.fn()
    const unsub = clock.onBeat(cb)
    unsub()
    clock._stepForTest(0.5)
    expect(cb).not.toHaveBeenCalled()
  })
})

describe('Clock — onTick', () => {
  it('onTick is called on every step with phase and beatCount', () => {
    const clock = new Clock()
    clock.setBpm(60) // 1 beat/sec
    const ticks: Array<[number, number]> = []
    clock.onTick((p, b) => ticks.push([p, b]))
    clock._stepForTest(0.5) // phase = 0.5, 0 beats
    clock._stepForTest(0.5) // phase = 0.0, 1 beat
    expect(ticks).toHaveLength(2)
    expect(ticks[0]![0]).toBeCloseTo(0.5)
    expect(ticks[0]![1]).toBe(0)
    expect(ticks[1]![0]).toBeCloseTo(0)
    expect(ticks[1]![1]).toBe(1)
  })
})
