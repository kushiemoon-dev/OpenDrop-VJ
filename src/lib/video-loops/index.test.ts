import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('$app/paths', () => ({ base: '' }));
vi.mock('$env/static/public', () => ({ PUBLIC_VIDEO_CDN: '' }));

describe('_loadManifest', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('returns empty array when fetch fails', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValueOnce(new Error('network')));
    const { _loadManifest } = await import('./index.js');
    const result = await _loadManifest('https://cdn.example');
    expect(result).toEqual([]);
  });

  it('returns empty array when response is not ok', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({ ok: false }));
    const { _loadManifest } = await import('./index.js');
    const result = await _loadManifest('https://cdn.example');
    expect(result).toEqual([]);
  });

  it('maps entries to VideoClipMeta with correct src and kind', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        entries: [{ slug: 'neon-city-01.webm', name: 'Neon City 01' }],
      }),
    }));
    const { _loadManifest } = await import('./index.js');
    const result = await _loadManifest('https://cdn.example');
    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({
      ref: { kind: 'builtin', src: 'https://cdn.example/neon-city-01.webm' },
      name: 'Neon City 01',
    });
  });

  it('URL-encodes slugs with special characters', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        entries: [{ slug: 'my clip 01.webm', name: 'My Clip' }],
      }),
    }));
    const { _loadManifest } = await import('./index.js');
    const result = await _loadManifest('https://cdn.example');
    expect(result[0].ref).toMatchObject({ src: 'https://cdn.example/my%20clip%2001.webm' });
  });
});

describe('initVideoLoops', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.doMock('$app/paths', () => ({ base: '' }));
    vi.doMock('$env/static/public', () => ({ PUBLIC_VIDEO_CDN: '' }));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('populates builtinClips from bundled manifest', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        entries: [
          { slug: 'neon-city-01.webm', name: 'Neon City 01' },
          { slug: 'glitch-01.webm', name: 'Glitch 01' },
        ],
      }),
    }));
    const mod = await import('./index.js');
    await mod.initVideoLoops();
    expect(mod.builtinClips).toHaveLength(2);
    expect(mod.builtinClips[0].ref).toEqual({
      kind: 'builtin',
      src: '/video-loops/neon-city-01.webm',
    });
  });

  it('is idempotent — second call is a no-op', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ entries: [{ slug: 'a.webm', name: 'A' }] }),
    });
    vi.stubGlobal('fetch', fetchMock);
    const mod = await import('./index.js');
    await mod.initVideoLoops();
    await mod.initVideoLoops();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('degrades gracefully when bundled manifest is absent (404)', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({ ok: false }));
    const mod = await import('./index.js');
    await mod.initVideoLoops();
    expect(mod.builtinClips).toEqual([]);
  });
});

describe('initVideoLoops with CDN', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.doMock('$app/paths', () => ({ base: '' }));
    vi.doMock('$env/static/public', () => ({
      PUBLIC_VIDEO_CDN: 'https://loops.example',
    }));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('merges CDN clips after bundled, deduplicating by name', async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          entries: [{ slug: 'bundled.webm', name: 'Shared Name' }],
        }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          entries: [
            { slug: 'cdn-a.webm', name: 'Shared Name' },   // duplicate, must be dropped
            { slug: 'cdn-b.webm', name: 'CDN Exclusive' }, // new, must be kept
          ],
        }),
      });
    vi.stubGlobal('fetch', fetchMock);
    const mod = await import('./index.js');
    await mod.initVideoLoops();
    expect(mod.builtinClips).toHaveLength(2);
    expect(mod.builtinClips.map((c) => c.name)).toEqual(['Shared Name', 'CDN Exclusive']);
    expect(mod.builtinClips[0].ref).toMatchObject({ src: '/video-loops/bundled.webm' }); // bundled wins
    expect(mod.builtinClips[1].ref).toMatchObject({ src: 'https://loops.example/cdn-b.webm' });
  });

  it('CDN failure does not remove bundled clips', async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ entries: [{ slug: 'a.webm', name: 'A' }] }),
      })
      .mockRejectedValueOnce(new Error('CDN unreachable'));
    vi.stubGlobal('fetch', fetchMock);
    const mod = await import('./index.js');
    await mod.initVideoLoops();
    expect(mod.builtinClips).toHaveLength(1);
    expect(mod.builtinClips[0].name).toBe('A');
  });
});
