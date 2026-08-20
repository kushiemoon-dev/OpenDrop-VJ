import { describe, it, expect } from 'vitest'
import { DEFAULT_COLOR_PARAMS } from './sync.js'
import { colorState } from './color-store.svelte.js'

describe('color-store', () => {
  it('starts both decks at the default color params', () => {
    expect(colorState.a).toEqual(DEFAULT_COLOR_PARAMS)
    expect(colorState.b).toEqual(DEFAULT_COLOR_PARAMS)
  })
})
