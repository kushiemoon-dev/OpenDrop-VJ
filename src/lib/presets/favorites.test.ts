import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { loadMoodLabels, saveMoodLabels } from './favorites.js'

describe('mood labels', () => {
  // Node's vitest environment here is 'node' (no jsdom), so `localStorage` isn't
  // a real global — stub an in-memory implementation before each test, same
  // pattern as cloud-presets.test.ts / presets/index.test.ts.
  beforeEach(() => {
    const store = new Map<string, string>()
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
      setItem: (k: string, v: string) => {
        store.set(k, v)
      },
      removeItem: (k: string) => {
        store.delete(k)
      },
      clear: () => {
        store.clear()
      },
    })
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('returns an empty object when nothing is saved', () => {
    expect(loadMoodLabels()).toEqual({})
  })

  it('round-trips saved labels', () => {
    saveMoodLabels({ 1: 'Calme', 3: 'Hype' })
    expect(loadMoodLabels()).toEqual({ 1: 'Calme', 3: 'Hype' })
  })
})
