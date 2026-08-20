import { describe, it, expect, beforeEach } from 'vitest'
import { defaultBeatTriggerConfig, applyBeatTriggerPatch } from './beat-trigger.js'
import { beatSyncState, updateBeatTriggerA, updateBeatTriggerB } from './beat-sync-store.svelte.js'

function resetState() {
  beatSyncState.beatSyncA = false
  beatSyncState.beatSyncB = false
  beatSyncState.lockA = false
  beatSyncState.lockB = false
  beatSyncState.autoXfade = false
  beatSyncState.beatsPerChange = 8
  beatSyncState.beatTriggerA = defaultBeatTriggerConfig()
  beatSyncState.beatTriggerB = defaultBeatTriggerConfig()
  beatSyncState.beat = false
}

describe('beat-sync-store', () => {
  beforeEach(resetState)

  it('starts with beat-sync/lock/auto-crossfade off, 8 beats per change, default triggers', () => {
    expect(beatSyncState.beatSyncA).toBe(false)
    expect(beatSyncState.beatSyncB).toBe(false)
    expect(beatSyncState.lockA).toBe(false)
    expect(beatSyncState.lockB).toBe(false)
    expect(beatSyncState.autoXfade).toBe(false)
    expect(beatSyncState.beatsPerChange).toBe(8)
    expect(beatSyncState.beatTriggerA).toEqual(defaultBeatTriggerConfig())
    expect(beatSyncState.beatTriggerB).toEqual(defaultBeatTriggerConfig())
    expect(beatSyncState.beat).toBe(false)
  })

  it("updateBeatTriggerA patches only deck A's trigger config", () => {
    updateBeatTriggerA({ beatsPerChange: 4 })
    expect(beatSyncState.beatTriggerA).toEqual(
      applyBeatTriggerPatch(defaultBeatTriggerConfig(), { beatsPerChange: 4 })
    )
    expect(beatSyncState.beatTriggerB).toEqual(defaultBeatTriggerConfig())
  })

  it("updateBeatTriggerB patches only deck B's trigger config", () => {
    updateBeatTriggerB({ beatsPerChange: 2 })
    expect(beatSyncState.beatTriggerB).toEqual(
      applyBeatTriggerPatch(defaultBeatTriggerConfig(), { beatsPerChange: 2 })
    )
  })
})
