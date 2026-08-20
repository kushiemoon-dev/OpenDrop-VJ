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

import { builtinClips } from './index.js'
import { saveVideo, deleteVideo, type VideoClipMeta } from '$lib/engine/video-store.js'

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
  // Which clips (builtin + user, keyed by clipKey()) participate in onVideoBeat()'s
  // auto-cut rotation. Empty = no filter, every clip participates (default,
  // matches pre-selection behavior — existing users see no change).
  selectedClipKeys: [] as string[],
  currentClipIndex: 0,
  playbackRate: 1,
  liveDeviceId: null as string | null,
  liveLabel: '',
  ndiSourceName: null as string | null,
  ndiUrlAddress: '',
})

// Non-reactive: internal cut-advance counter, same treatment as +page.svelte's
// other tick-loop memory (pausedSlots, lastFps) — never rendered, only read
// inside onVideoBeat itself.
let beatCount = 0

/** Stable identity for a clip, independent of its position in allClips (which shifts
 * as clips are added/removed) — used to key selectedClipKeys. */
export function clipKey(clip: VideoClipMeta): string {
  return clip.ref.kind === 'builtin' ? clip.ref.src : clip.ref.id
}

/** Returns the new clip's key, or null if the file was skipped (over the size cap). */
export async function addVideoFromFile(file: File): Promise<string | null> {
  if (file.size > 50 * 1024 * 1024) return null
  const id = crypto.randomUUID()
  await saveVideo(id, file)
  videoState.userClips = [
    ...videoState.userClips,
    { ref: { kind: 'user', id }, name: file.name.replace(/\.[^.]+$/, '') },
  ]
  if (!videoState.enabled) videoState.enabled = true
  return id
}

/** Marks the given clip keys as selected for onVideoBeat()'s auto-cut rotation
 * (additive — existing selection is preserved). */
export function selectClips(keys: string[]): void {
  videoState.selectedClipKeys = [...new Set([...videoState.selectedClipKeys, ...keys])]
}

export function toggleClipSelection(key: string): void {
  videoState.selectedClipKeys = videoState.selectedClipKeys.includes(key)
    ? videoState.selectedClipKeys.filter((k) => k !== key)
    : [...videoState.selectedClipKeys, key]
}

export function clearClipSelection(): void {
  videoState.selectedClipKeys = []
}

export async function onVideoFilePick(e: Event): Promise<void> {
  const files = Array.from((e.target as HTMLInputElement).files ?? [])
  if (files.length === 0) return
  // Sequential, not Promise.all — addVideoFromFile does a read-then-spread
  // (`[...videoState.userClips, newClip]`) that isn't safe to run concurrently,
  // concurrent calls would race and silently drop an added clip.
  const addedIds: string[] = []
  for (const f of files) {
    const id = await addVideoFromFile(f)
    if (id !== null) addedIds.push(id)
  }
  // Importing 2+ clips at once is exactly the "prepare a playlist on import" case —
  // group them into the auto-cut rotation immediately instead of leaving the user
  // to select each one by hand afterward.
  if (addedIds.length > 1) selectClips(addedIds)
  ;(e.target as HTMLInputElement).value = ''
}

export async function removeVideoClip(index: number): Promise<void> {
  const clip = videoState.userClips[index - builtinClips.length]
  if (clip?.ref.kind === 'user') {
    const removedId = clip.ref.id
    await deleteVideo(removedId)
    videoState.selectedClipKeys = videoState.selectedClipKeys.filter((k) => k !== removedId)
  }
  videoState.userClips = videoState.userClips.filter((_, i) => i !== index - builtinClips.length)
  const totalClips = builtinClips.length + videoState.userClips.length
  if (videoState.currentClipIndex >= totalClips) videoState.currentClipIndex = 0
}

/** Beat-driven clip cut (call from the page's clock.onBeat handler). */
/** Indices into [...builtinClips, ...userClips] that participate in the auto-cut
 * rotation — every clip if selectedClipKeys is empty (no filter applied yet). */
function activeClipIndices(): number[] {
  const all = [...builtinClips, ...videoState.userClips]
  if (videoState.selectedClipKeys.length === 0) return all.map((_, i) => i)
  return all.reduce<number[]>((acc, clip, i) => {
    if (videoState.selectedClipKeys.includes(clipKey(clip))) acc.push(i)
    return acc
  }, [])
}

export function onVideoBeat(): void {
  // A live camera or NDI source is a single feed, not a cycling library —
  // also avoids currentClipIndex drifting while live, which would jump the
  // clip on exit.
  if (videoState.liveDeviceId || videoState.ndiSourceName) return
  const active = activeClipIndices()
  if (!(
    videoState.enabled &&
    videoState.reactCut &&
    videoState.advance !== 'manual' &&
    active.length > 1
  ))
    return
  beatCount = (beatCount + 1) % videoState.beatsPerCut
  if (beatCount === 0) {
    const pos = active.indexOf(videoState.currentClipIndex)
    const nextPos =
      videoState.advance === 'shuffle'
        ? Math.floor(Math.random() * active.length)
        : pos === -1
          ? 0
          : (pos + 1) % active.length
    videoState.currentClipIndex = active[nextPos]!
  }
}

/** Switch the video layer to a live camera device. Same auto-enable-if-off behavior as addVideoFromFile.
 * Mutually exclusive with an NDI source — only one external feed can drive the layer at a time. */
export function setLiveCamera(deviceId: string, label: string): void {
  videoState.ndiSourceName = null
  videoState.ndiUrlAddress = ''
  videoState.liveDeviceId = deviceId
  videoState.liveLabel = label
  if (!videoState.enabled) videoState.enabled = true
}

/** Drop the live camera and fall back to the clip library. Leaves `enabled` untouched. */
export function clearLiveCamera(): void {
  videoState.liveDeviceId = null
  videoState.liveLabel = ''
}

/** Switch the video layer to a received NDI source. Same auto-enable-if-off behavior as addVideoFromFile.
 * Mutually exclusive with a live camera — only one external feed can drive the layer at a time. */
export function setNdiSource(sourceName: string, urlAddress: string): void {
  videoState.liveDeviceId = null
  videoState.liveLabel = ''
  videoState.ndiSourceName = sourceName
  videoState.ndiUrlAddress = urlAddress
  if (!videoState.enabled) videoState.enabled = true
}

/** Drop the NDI source and fall back to the clip library. Leaves `enabled` untouched. */
export function clearNdiSource(): void {
  videoState.ndiSourceName = null
  videoState.ndiUrlAddress = ''
}

/** Bass-driven speed warp (call from the page's per-frame VU meter tick). */
export function onVideoAudioTick(bass: number): void {
  // playbackRate is inert on a live MediaStream — treat live/NDI the same as warp-off.
  if (
    videoState.enabled &&
    videoState.reactWarp &&
    !videoState.liveDeviceId &&
    !videoState.ndiSourceName
  ) {
    const target = 0.6 + bass * 1.4
    videoState.playbackRate += (target - videoState.playbackRate) * 0.15
  } else {
    videoState.playbackRate = 1
  }
}
