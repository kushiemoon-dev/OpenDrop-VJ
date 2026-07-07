/**
 * playback-store.svelte.ts — reactive wrapper around video-loop playback
 * state (enabled/opacity/advance-mode/reactions + user clip library) and the
 * beat/audio-reactive hooks that drive it. Extracted from +page.svelte.
 * Singleton module, same shape as cloud-presets-store.svelte.ts and
 * overlay-store.svelte.ts.
 *
 * `allClips`/`currentClip`/`videoPlaybackRateStep` stay as page-level
 * $derived values in +page.svelte (same reasoning as overlay-store: no
 * existing precedent in this codebase for module-level $derived in a
 * .svelte.ts store).
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import { builtinClips } from './index.js';
import { saveVideo, deleteVideo, type VideoClipMeta } from '$lib/engine/video-store.js';

export const videoState = $state({
	enabled: false,
	opacity: 0.6,
	advance: 'shuffle' as 'shuffle' | 'sequential' | 'manual',
	beatsPerCut: 8,
	reactCut: true,
	reactFlash: true,
	reactWarp: true,
	reactHue: false,
	userClips: [] as VideoClipMeta[],
	currentClipIndex: 0,
	playbackRate: 1,
});

// Non-reactive: internal cut-advance counter, same treatment as +page.svelte's
// other tick-loop memory (pausedSlots, lastFps) — never rendered, only read
// inside onVideoBeat itself.
let beatCount = 0;

export async function addVideoFromFile(file: File): Promise<void> {
	if (file.size > 50 * 1024 * 1024) return;
	const id = crypto.randomUUID();
	await saveVideo(id, file);
	videoState.userClips = [...videoState.userClips, { ref: { kind: 'user', id }, name: file.name.replace(/\.[^.]+$/, '') }];
	if (!videoState.enabled) videoState.enabled = true;
}

export async function onVideoFilePick(e: Event): Promise<void> {
	const files = (e.target as HTMLInputElement).files;
	if (!files) return;
	for (const f of Array.from(files)) await addVideoFromFile(f);
	(e.target as HTMLInputElement).value = '';
}

export async function removeVideoClip(index: number): Promise<void> {
	const clip = videoState.userClips[index - builtinClips.length];
	if (clip?.ref.kind === 'user') await deleteVideo(clip.ref.id);
	videoState.userClips = videoState.userClips.filter((_, i) => i !== index - builtinClips.length);
	const totalClips = builtinClips.length + videoState.userClips.length;
	if (videoState.currentClipIndex >= totalClips) videoState.currentClipIndex = 0;
}

/** Beat-driven clip cut (call from the page's clock.onBeat handler). */
export function onVideoBeat(): void {
	const totalClips = builtinClips.length + videoState.userClips.length;
	if (!(videoState.enabled && videoState.reactCut && videoState.advance !== 'manual' && totalClips > 1)) return;
	beatCount = (beatCount + 1) % videoState.beatsPerCut;
	if (beatCount === 0) {
		videoState.currentClipIndex = videoState.advance === 'shuffle'
			? Math.floor(Math.random() * totalClips)
			: (videoState.currentClipIndex + 1) % totalClips;
	}
}

/** Bass-driven speed warp (call from the page's per-frame VU meter tick). */
export function onVideoAudioTick(bass: number): void {
	if (videoState.enabled && videoState.reactWarp) {
		const target = 0.6 + bass * 1.4;
		videoState.playbackRate += (target - videoState.playbackRate) * 0.15;
	} else {
		videoState.playbackRate = 1;
	}
}
