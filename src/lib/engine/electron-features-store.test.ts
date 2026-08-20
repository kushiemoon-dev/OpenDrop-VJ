import { describe, it, expect } from 'vitest'
import { electronFeaturesState } from './electron-features-store.svelte.js'

describe('electron-features-store', () => {
  it('starts with every Electron feature off and no errors', () => {
    expect(electronFeaturesState.ndi).toEqual({ active: false, error: '' })
    expect(electronFeaturesState.osc).toEqual({ active: false, port: 7000, error: '' })
    expect(electronFeaturesState.remote).toEqual({ active: false, url: '', error: '' })
    expect(electronFeaturesState.link).toEqual({ active: false, peers: 0, error: '' })
    expect(electronFeaturesState.v4l2).toEqual({ active: false, error: '' })
    expect(electronFeaturesState.spout).toEqual({ active: false, error: '' })
  })
})
