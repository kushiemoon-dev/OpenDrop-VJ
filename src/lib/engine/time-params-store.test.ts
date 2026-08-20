import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { defaultTimeParams, getGlobalTimeParams } from './time-params.js'
import { timeParamsState, updateTimeParams } from './time-params-store.svelte.js'

function resetState() {
  timeParamsState.params = [
    defaultTimeParams(),
    defaultTimeParams(),
    defaultTimeParams(),
    defaultTimeParams(),
  ]
}

describe('time-params-store', () => {
  beforeEach(() => {
    vi.stubGlobal('window', {})
    resetState()
  })
  afterEach(() => vi.unstubAllGlobals())

  it('starts with 4 slots of default time params', () => {
    expect(timeParamsState.params).toHaveLength(4)
    expect(timeParamsState.params[0]).toEqual(defaultTimeParams())
  })

  it('updates one slot without touching the others', () => {
    updateTimeParams(1, { zoomMult: 1.5 })
    expect(timeParamsState.params[1].zoomMult).toBe(1.5)
    expect(timeParamsState.params[0].zoomMult).toBe(1)
    expect(timeParamsState.params[2].zoomMult).toBe(1)
  })

  it('writes the patch through to the window-backed global Butterchurn reads', () => {
    updateTimeParams(2, { rotMult: 0.5 })
    expect(getGlobalTimeParams()[2]!.rotMult).toBe(0.5)
  })
})
