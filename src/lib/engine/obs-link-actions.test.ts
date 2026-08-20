import { describe, it, expect } from 'vitest'
import { isOwnEcho } from './obs-link-actions.js'

describe('isOwnEcho', () => {
  it('is not an echo when nothing has been sent outbound yet', () => {
    expect(isOwnEcho('Scene A', null)).toBe(false)
  })

  it('is an echo when the incoming scene matches the last outbound scene', () => {
    expect(isOwnEcho('Scene A', 'Scene A')).toBe(true)
  })

  it('is not an echo when the incoming scene differs from the last outbound scene', () => {
    expect(isOwnEcho('Scene B', 'Scene A')).toBe(false)
  })

  it('keeps recognizing the same echo repeatedly (not one-shot)', () => {
    // Unlike the old flag-based guard, this is a plain comparison: it stays true
    // for as long as the incoming scene matches, and never "consumes" itself —
    // there's nothing to starve a later, unrelated change.
    expect(isOwnEcho('Scene A', 'Scene A')).toBe(true)
    expect(isOwnEcho('Scene A', 'Scene A')).toBe(true)
  })
})
