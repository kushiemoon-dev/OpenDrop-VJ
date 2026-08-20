import { describe, it, expect } from 'vitest'
import { midiConnectionState } from './midi-connection-store.svelte.js'

describe('midi-connection-store', () => {
  it('starts disconnected, with no devices and no MIDI clock BPM', () => {
    expect(midiConnectionState.connected).toBe(false)
    expect(midiConnectionState.deviceNames).toEqual([])
    expect(midiConnectionState.clockBpm).toBe(0)
  })
})
