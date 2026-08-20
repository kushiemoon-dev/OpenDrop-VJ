/**
 * playlist-store.svelte.ts — reactive wrapper around playlist A/B item lists,
 * mode/interval, and the auto-cycle PlaylistEngine instances. Extracted from
 * +page.svelte. Singleton module, same shape as the other *-store.svelte.ts
 * files in this codebase.
 *
 * The two PlaylistEngine instances are created by +page.svelte's
 * startVisualizer (their per-deck onPreset callback touches manager/sync/
 * presetA/presetB, which are core deck-orchestration concerns that stay in
 * the page) and registered here via setPlaylistEngines() — everything else
 * (item CRUD, mode/interval, start/stop/next/prev, export/import) lives here.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import { PlaylistEngine, type PlaylistMode } from './playlist.js'

export const playlistState = $state({
  intervalSec: 10,
  mode: 'sequential' as PlaylistMode,
  aPlaying: false,
  bPlaying: false,
  aItems: [] as string[],
  bItems: [] as string[],
})

let engineA: PlaylistEngine | null = null
let engineB: PlaylistEngine | null = null

/** Register the two engine instances once they've been constructed (startVisualizer). */
export function setPlaylistEngines(a: PlaylistEngine, b: PlaylistEngine): void {
  engineA = a
  engineB = b
}

export function destroyPlaylistEngines(): void {
  engineA?.destroy()
  engineB?.destroy()
  engineA = null
  engineB = null
}

export function addToPlaylist(deck: 'A' | 'B', name: string): void {
  if (deck === 'A') {
    if (playlistState.aItems.includes(name)) return
    playlistState.aItems = [...playlistState.aItems, name]
    engineA?.setItems(playlistState.aItems)
  } else {
    if (playlistState.bItems.includes(name)) return
    playlistState.bItems = [...playlistState.bItems, name]
    engineB?.setItems(playlistState.bItems)
  }
}

export function removeFromPlaylist(deck: 'A' | 'B', name: string): void {
  if (deck === 'A') {
    playlistState.aItems = playlistState.aItems.filter((n) => n !== name)
    engineA?.setItems(playlistState.aItems)
  } else {
    playlistState.bItems = playlistState.bItems.filter((n) => n !== name)
    engineB?.setItems(playlistState.bItems)
  }
}

export function togglePlaylist(deck: 'A' | 'B'): void {
  const pl = deck === 'A' ? engineA : engineB
  if (!pl) return
  pl.setInterval(playlistState.intervalSec * 1000)
  pl.setMode(playlistState.mode)
  if (pl.playing) {
    pl.stop()
  } else {
    pl.start()
  }
  if (deck === 'A') playlistState.aPlaying = pl.playing
  else playlistState.bPlaying = pl.playing
}

export function playlistNext(deck: 'A' | 'B'): void {
  ;(deck === 'A' ? engineA : engineB)?.next()
}

export function playlistPrev(deck: 'A' | 'B'): void {
  ;(deck === 'A' ? engineA : engineB)?.prev()
}

/** Used by beat-sync toggles (Infinity = fully beat-driven, no own timer). */
export function setPlaylistBeatSyncInterval(deck: 'A' | 'B', ms: number): void {
  ;(deck === 'A' ? engineA : engineB)?.setInterval(ms)
}

export function exportPlaylists(): void {
  const data = JSON.stringify(
    {
      version: 1,
      playlistA: playlistState.aItems,
      playlistB: playlistState.bItems,
      intervalSec: playlistState.intervalSec,
      mode: playlistState.mode,
    },
    null,
    2
  )
  const blob = new Blob([data], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'opendrop-playlists.json'
  a.click()
  URL.revokeObjectURL(url)
}

export function importPlaylists(e: Event): void {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  const reader = new FileReader()
  reader.onload = () => {
    try {
      const data = JSON.parse(reader.result as string)
      if (Array.isArray(data.playlistA)) playlistState.aItems = data.playlistA
      if (Array.isArray(data.playlistB)) playlistState.bItems = data.playlistB
      if (typeof data.intervalSec === 'number') playlistState.intervalSec = data.intervalSec
      if (data.mode === 'sequential' || data.mode === 'shuffle') playlistState.mode = data.mode
      engineA?.setItems(playlistState.aItems)
      engineB?.setItems(playlistState.bItems)
    } catch {
      /* ignore corrupt import file */
    }
  }
  reader.readAsText(file)
  ;(e.target as HTMLInputElement).value = ''
}
