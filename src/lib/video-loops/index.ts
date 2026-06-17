import { base } from '$app/paths';
import type { VideoClipMeta } from '$lib/engine/video-store.js';

interface ManifestEntry { slug: string; name: string; }

let _initialized = false;

export let builtinClips: VideoClipMeta[] = [];

export async function initVideoLoops(): Promise<void> {
	if (_initialized) return;
	_initialized = true;
	try {
		const res = await fetch(`${base}/video-loops/manifest.json`);
		if (!res.ok) return;
		const m = await res.json() as { entries: ManifestEntry[] };
		builtinClips = (m.entries ?? []).map((e) => ({
			ref: { kind: 'builtin' as const, src: `${base}/video-loops/${encodeURIComponent(e.slug)}` },
			name: e.name,
		}));
	} catch { /* degrade to empty — manifest may not exist yet */ }
}
