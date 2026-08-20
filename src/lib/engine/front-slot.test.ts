import { describe, it, expect } from 'vitest'
import { frontSlotIndex, frontSlotMood } from './front-slot.js'

describe('frontSlotIndex', () => {
  it('picks the slot with the highest opacity', () => {
    expect(frontSlotIndex([0.1, 0.9, 0, 0])).toBe(1)
  })

  it('picks the first slot on a tie', () => {
    expect(frontSlotIndex([0.5, 0.5, 0, 0])).toBe(0)
  })
})

describe('frontSlotMood', () => {
  it('returns the favorite color index of the preset loaded on the front slot', () => {
    const favColors = { 'preset-b.milk': 3 }
    const presets4 = ['preset-a.milk', 'preset-b.milk', '', '']
    expect(frontSlotMood(favColors, presets4, 1)).toBe(3)
  })

  it('returns null when the front slot preset has no mood color', () => {
    const favColors = {}
    const presets4 = ['preset-a.milk', '', '', '']
    expect(frontSlotMood(favColors, presets4, 0)).toBeNull()
  })
})
