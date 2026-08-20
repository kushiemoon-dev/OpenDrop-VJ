import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Mock $app/paths before importing the module
vi.mock('$app/paths', () => ({ base: '' }))

// index.ts now imports cloud-presets.ts, which imports $env/static/public —
// this mock is needed so that ALL imports in this file (including those in
// describe('getSlug', ...) below) resolve, even without direct use of the cloud.
vi.mock('$env/static/public', () => ({ PUBLIC_CLOUD_PRESETS_API: '' }))

// Mock fetch before importing initPresets
const FAKE_MANIFEST = {
  entries: [
    { slug: 'category/cool-preset', name: 'Cool Category - Cool Preset' },
    { slug: 'category/other-preset', name: 'Cool Category - Other Preset' },
    { slug: 'another/preset', name: 'Another - Preset' },
  ],
}

describe('getSlug', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.doMock('$app/paths', () => ({ base: '' }))
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('returns the slug for a known name after initPresets', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValueOnce({
        ok: true,
        json: async () => FAKE_MANIFEST,
      })
    )
    const { initPresets: init, getSlug: slug } = await import('./index.js')

    await init()

    expect(slug('Cool Category - Cool Preset')).toBe('category/cool-preset')
    expect(slug('Cool Category - Other Preset')).toBe('category/other-preset')
    expect(slug('Another - Preset')).toBe('another/preset')
  })

  it('returns undefined for an unknown name', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValueOnce({
        ok: true,
        json: async () => FAKE_MANIFEST,
      })
    )
    const { initPresets: init, getSlug: slug } = await import('./index.js')

    await init()

    expect(slug('inexistant/preset')).toBeUndefined()
    expect(slug('Unknown - Preset')).toBeUndefined()
  })

  it('returns undefined before initPresets', async () => {
    // Without calling initPresets, _nameToSlug is empty
    const { getSlug: slug } = await import('./index.js')
    expect(slug('Cool Category - Cool Preset')).toBeUndefined()
  })
})

describe('loadPresetData — cloud fallback', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.doMock('$app/paths', () => ({ base: '' }))
    // Node's vitest environment here is 'node' (no jsdom), so `localStorage` isn't
    // a real global — stub an in-memory implementation, same as cloud-presets.test.ts.
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

  it('a name absent from the static manifest and without a cloud prefix -> null (unchanged behavior)', async () => {
    const { loadPresetData } = await import('./index.js')
    expect(await loadPresetData('Nom Inconnu')).toBeNull()
  })

  it('a name prefixed with ☁ without a token in localStorage -> null', async () => {
    const { loadPresetData } = await import('./index.js')
    expect(await loadPresetData('☁ Mon Preset')).toBeNull()
  })

  it('a name prefixed with ☁ with a token -> resolves via loadCloudPresetData', async () => {
    localStorage.setItem('od-cloud-token', 'tok1')
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce({
          ok: true,
          json: async () => [{ id: 'a', name: '☁ Mon Preset', sizeBytes: 10, uploadedAt: 1 }],
        })
        .mockResolvedValueOnce({ ok: true, json: async () => ({ frame_eqs_str: 'a.zoom=1;' }) })
    )
    vi.doMock('$env/static/public', () => ({
      PUBLIC_CLOUD_PRESETS_API: 'https://presets-cloud.example',
    }))
    const { loadPresetData } = await import('./index.js')
    expect(await loadPresetData('☁ Mon Preset')).toEqual({ frame_eqs_str: 'a.zoom=1;' })
  })
})
