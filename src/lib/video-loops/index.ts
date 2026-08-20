import { base } from '$app/paths'
import { PUBLIC_VIDEO_CDN } from '$env/static/public'
import type { VideoClipMeta } from '$lib/engine/video-store.js'

interface ManifestEntry {
  slug: string
  name: string
}

let _initialized = false
export let builtinClips: VideoClipMeta[] = []

export async function _loadManifest(baseUrl: string): Promise<VideoClipMeta[]> {
  try {
    const res = await fetch(`${baseUrl}/manifest.json`)
    if (!res.ok) return []
    const m = (await res.json()) as { entries: ManifestEntry[] }
    return (m.entries ?? []).map((e) => ({
      ref: { kind: 'builtin' as const, src: `${baseUrl}/${encodeURIComponent(e.slug)}` },
      name: e.name,
    }))
  } catch {
    return []
  }
}

// Returns builtinClips so callers can mirror it into a reactive $state variable —
// this module is a plain .ts file (not .svelte.ts), so `builtinClips` itself is
// not tracked by Svelte; a $derived reading it directly would never re-run once
// this async load resolves after the initial render.
export async function initVideoLoops(): Promise<VideoClipMeta[]> {
  if (_initialized) return builtinClips
  _initialized = true
  const bundled = await _loadManifest(`${base}/video-loops`)
  const cdn = PUBLIC_VIDEO_CDN ? await _loadManifest(PUBLIC_VIDEO_CDN) : []
  const seen = new Set(bundled.map((c) => c.name))
  builtinClips = [...bundled, ...cdn.filter((c) => !seen.has(c.name))]
  return builtinClips
}
