import { describe, it, expect } from 'vitest'
import { strobeState } from './strobe-store.svelte.js'

describe('strobe-store', () => {
  it('starts off, at 1x rate, 0.8 intensity, white, not flashing', () => {
    expect(strobeState.on).toBe(false)
    expect(strobeState.rate).toBe(1)
    expect(strobeState.intensity).toBe(0.8)
    expect(strobeState.color).toBe('#ffffff')
    expect(strobeState.flash).toBe(false)
  })
})
