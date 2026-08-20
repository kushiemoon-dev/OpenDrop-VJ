import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { DEFAULT_KEYMAP } from './keymap.js'
import {
  midiMappingState,
  setMidiMapping,
  clearMidiMapping,
  removeKeyBinding,
  resetMidiKeymap,
} from './midi-mapping-store.svelte.js'

function resetState() {
  midiMappingState.midiMappings = {}
  midiMappingState.keymap = { ...DEFAULT_KEYMAP }
  midiMappingState.learningAction = null
}

describe('midi-mapping-store', () => {
  // Node's vitest environment here is 'node' (no jsdom), so `localStorage` isn't
  // a real global — resetKeymap() touches it (see keymap.ts), stub it.
  beforeEach(() => {
    vi.stubGlobal('localStorage', { removeItem: vi.fn(), getItem: vi.fn(), setItem: vi.fn() })
    resetState()
  })
  afterEach(() => vi.unstubAllGlobals())

  it('starts with no MIDI mappings, the default keymap, and no learn in progress', () => {
    expect(midiMappingState.midiMappings).toEqual({})
    expect(midiMappingState.keymap).toEqual(DEFAULT_KEYMAP)
    expect(midiMappingState.learningAction).toBeNull()
  })

  it('setMidiMapping assigns a key to an action without touching other mappings', () => {
    setMidiMapping('strobe-toggle', 'dev1:cc:0:20')
    setMidiMapping('playlist-toggle-a', 'dev1:cc:0:21')
    expect(midiMappingState.midiMappings['strobe-toggle']).toBe('dev1:cc:0:20')
    expect(midiMappingState.midiMappings['playlist-toggle-a']).toBe('dev1:cc:0:21')
  })

  it('clearMidiMapping removes only the given action', () => {
    setMidiMapping('strobe-toggle', 'dev1:cc:0:20')
    setMidiMapping('playlist-toggle-a', 'dev1:cc:0:21')
    clearMidiMapping('strobe-toggle')
    expect(midiMappingState.midiMappings['strobe-toggle']).toBeUndefined()
    expect(midiMappingState.midiMappings['playlist-toggle-a']).toBe('dev1:cc:0:21')
  })

  it('removeKeyBinding removes only the given key', () => {
    midiMappingState.keymap = { ...DEFAULT_KEYMAP, a: 'strobe-toggle', b: 'playlist-toggle-a' }
    removeKeyBinding('a')
    expect(midiMappingState.keymap.a).toBeUndefined()
    expect(midiMappingState.keymap.b).toBe('playlist-toggle-a')
  })

  it('resetMidiKeymap restores the default keymap', () => {
    midiMappingState.keymap = { ...DEFAULT_KEYMAP, a: 'strobe-toggle' }
    resetMidiKeymap()
    expect(midiMappingState.keymap).toEqual(DEFAULT_KEYMAP)
  })
})
