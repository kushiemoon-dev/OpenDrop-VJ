/**
 * thumbnailer.svelte.ts — Lazy Butterchurn thumbnail renderer.
 *
 * Uses a single offscreen canvas + AudioContext (browser-only, lazy-inited)
 * to render preset thumbnails one at a time. Results are stored in IndexedDB
 * via thumb-cache.js and exposed as reactive $state for Svelte components.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import _butterchurn from 'butterchurn'
import { loadPresetData } from './index.js'
import { getThumbUrl, putThumbBlob, cacheUrl } from './thumb-cache.js'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const bc = (_butterchurn as any).createVisualizer
	? _butterchurn
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	: ((_butterchurn as any).default ?? _butterchurn)

// ─── Exported constants ───────────────────────────────────────────────────────

export const THUMB_W = 192
export const THUMB_H = 108
export const WARMUP_FRAMES = 30
export const WEBP_QUALITY = 0.7

// ─── Reactive state ───────────────────────────────────────────────────────────

// Extension .svelte.ts required for $state
// Svelte 5 $state Map — .set()/.delete() sont les mutations correctes sur un proxy réactif
export const thumbUrls = $state(new Map<string, string>())

// ─── Internal types ───────────────────────────────────────────────────────────

interface ThumbJob { slug: string; name: string }

// ─── Pure queue functions (exported for tests) ────────────────────────────────

/**
 * Add job to the front of the queue, deduplicating by slug.
 * Pure function — returns a new array, does not mutate input.
 */
export function enqueueFront(queue: ThumbJob[], job: ThumbJob): ThumbJob[] {
	const filtered = queue.filter(j => j.slug !== job.slug)
	return [job, ...filtered]
}

/**
 * Remove and return the first job from the queue.
 * Pure function — returns [job|null, remaining].
 */
export function dequeueJob(queue: ThumbJob[]): [ThumbJob | null, ThumbJob[]] {
	if (queue.length === 0) return [null, []]
	return [queue[0], queue.slice(1)]
}

// ─── Browser-only singleton (lazy-init) ───────────────────────────────────────

let _viz: import('butterchurn').Visualizer | null = null
let _canvas: HTMLCanvasElement | null = null
let _audioCtx: AudioContext | null = null

function ensureInit(): boolean {
	if (typeof window === 'undefined') return false
	if (_viz) return true

	// Offscreen canvas — not attached to DOM
	_canvas = document.createElement('canvas')
	_canvas.width = THUMB_W
	_canvas.height = THUMB_H

	// Dedicated AudioContext for the thumbnailer
	_audioCtx = new AudioContext()

	// White noise loop (≈2s) — gain 0.4, NOT connected to destination (silent)
	const sampleRate = _audioCtx.sampleRate
	const bufLen = sampleRate * 2
	const buffer = _audioCtx.createBuffer(1, bufLen, sampleRate)
	const data = buffer.getChannelData(0)
	for (let i = 0; i < bufLen; i++) data[i] = Math.random() * 2 - 1
	const noiseSource = _audioCtx.createBufferSource()
	noiseSource.buffer = buffer
	noiseSource.loop = true
	const noiseGain = _audioCtx.createGain()
	noiseGain.gain.value = 0.4
	noiseSource.connect(noiseGain)
	// ⚠️ intentionally NOT connecting noiseGain to _audioCtx.destination
	noiseSource.start()

	// Create Butterchurn visualizer
	_viz = (bc as typeof _butterchurn).createVisualizer(_audioCtx, _canvas, {
		width: THUMB_W,
		height: THUMB_H,
		meshWidth: 24,
		meshHeight: 18,
		pixelRatio: 1,
		textureRatio: 1,
		outputFXAA: false,
	})
	_viz.connectAudio(noiseGain)

	// Resume opportunistically (user has already interacted)
	_audioCtx.resume().catch(() => {})

	return true
}

// ─── Queue state ──────────────────────────────────────────────────────────────

let _queue: ThumbJob[] = []
let _pumping = false
const _inFlight = new Set<string>()

// ─── Public API ───────────────────────────────────────────────────────────────

/**
 * Request a thumbnail for the given preset slug/name.
 * No-op on the server or if the thumbnail is already in memory cache.
 * Enqueues at the front (most recently visible presets have priority).
 */
export function requestThumb(slug: string, name: string): void {
	if (typeof window === 'undefined') return
	if (thumbUrls.get(slug)) return

	getThumbUrl(slug).then(url => {
		if (url) {
			thumbUrls.set(slug, url)
			return
		}
		if (!_inFlight.has(slug)) {
			_inFlight.add(slug)
			_queue = enqueueFront(_queue, { slug, name })
			kickPump()
		}
	}).catch(() => {
		if (!_inFlight.has(slug)) {
			_inFlight.add(slug)
			_queue = enqueueFront(_queue, { slug, name })
			kickPump()
		}
	})
}

/**
 * Remove a job from the pending queue (e.g. preset scrolled off screen).
 * Does not cancel a job already in progress.
 */
export function releaseThumb(slug: string): void {
	_queue = _queue.filter(j => j.slug !== slug)
}

// ─── Internal pump ────────────────────────────────────────────────────────────

function kickPump(): void {
	if (_pumping) return
	_pumping = true
	pumpNext()
}

function rAF(): Promise<void> {
	return new Promise(res => requestAnimationFrame(() => res()))
}

async function pumpNext(): Promise<void> {
	if (!ensureInit()) { _pumping = false; return }

	const [job, rest] = dequeueJob(_queue)
	if (!job) { _pumping = false; return }
	_queue = rest

	try {
		const data = await loadPresetData(job.name)
		if (!data) { _inFlight.delete(job.slug); setTimeout(pumpNext, 0); return }

		_viz!.loadPreset(data, 0)

		// Warmup — render WARMUP_FRAMES frames so the preset settles
		for (let i = 0; i < WARMUP_FRAMES; i++) {
			await rAF()
			_viz!.render()
		}

		// ⚠️ Final frame + capture in the SAME rAF tick — no await between render and toBlob
		await rAF()
		_viz!.render()
		_canvas!.toBlob(
			(blob) => {
				if (!blob) { _inFlight.delete(job.slug); setTimeout(pumpNext, 0); return }
				putThumbBlob(job.slug, blob).catch(() => {})
				const url = cacheUrl(job.slug, blob)
				thumbUrls.set(job.slug, url)
				_inFlight.delete(job.slug)
				setTimeout(pumpNext, 0)
			},
			'image/webp',
			WEBP_QUALITY
		)
	} catch {
		_inFlight.delete(job.slug)
		setTimeout(pumpNext, 0)
	}
}
