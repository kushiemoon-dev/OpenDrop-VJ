import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { PlaylistEngine } from './playlist.js'
import {
  playlistState,
  setPlaylistEngines,
  destroyPlaylistEngines,
  addToPlaylist,
  removeFromPlaylist,
  togglePlaylist,
  playlistNext,
  playlistPrev,
  setPlaylistBeatSyncInterval,
  exportPlaylists,
  importPlaylists,
} from './playlist-store.svelte.js'

function resetState() {
  playlistState.intervalSec = 10
  playlistState.mode = 'sequential'
  playlistState.aPlaying = false
  playlistState.bPlaying = false
  playlistState.aItems = []
  playlistState.bItems = []
}

describe('playlist-store', () => {
  beforeEach(() => {
    resetState()
    vi.useFakeTimers()
  })
  afterEach(() => {
    destroyPlaylistEngines()
    vi.useRealTimers()
  })

  describe('addToPlaylist / removeFromPlaylist', () => {
    it('adds a preset to playlist A without duplicating it', () => {
      addToPlaylist('A', 'preset1')
      addToPlaylist('A', 'preset1')
      expect(playlistState.aItems).toEqual(['preset1'])
    })

    it('adds a preset to playlist B independently of A', () => {
      addToPlaylist('B', 'presetX')
      expect(playlistState.bItems).toEqual(['presetX'])
      expect(playlistState.aItems).toEqual([])
    })

    it('removes a preset from the targeted playlist', () => {
      addToPlaylist('A', 'p1')
      addToPlaylist('A', 'p2')
      removeFromPlaylist('A', 'p1')
      expect(playlistState.aItems).toEqual(['p2'])
    })

    it('propagates setItems to the active PlaylistEngine', () => {
      const cbA = vi.fn()
      const engineA = new PlaylistEngine([], 'sequential', 1000, cbA)
      const engineB = new PlaylistEngine([], 'sequential', 1000, vi.fn())
      setPlaylistEngines(engineA, engineB)
      addToPlaylist('A', 'p1')
      engineA.start()
      expect(cbA).toHaveBeenCalledWith('p1')
    })
  })

  describe('togglePlaylist', () => {
    it('starts then stops, and reflects playing in playlistState', () => {
      const engineA = new PlaylistEngine(['p1'], 'sequential', 1000, vi.fn())
      setPlaylistEngines(engineA, new PlaylistEngine([], 'sequential', 1000, vi.fn()))
      togglePlaylist('A')
      expect(playlistState.aPlaying).toBe(true)
      togglePlaylist('A')
      expect(playlistState.aPlaying).toBe(false)
    })

    it("does nothing if the engine hasn't been created yet", () => {
      expect(() => togglePlaylist('A')).not.toThrow()
      expect(playlistState.aPlaying).toBe(false)
    })

    it('applies the current intervalSec/mode before starting', () => {
      const cbB = vi.fn()
      const engineB = new PlaylistEngine(['x', 'y'], 'sequential', 5000, cbB)
      setPlaylistEngines(new PlaylistEngine([], 'sequential', 1000, vi.fn()), engineB)
      playlistState.mode = 'sequential'
      playlistState.intervalSec = 2 // 2000ms
      togglePlaylist('B')
      cbB.mockClear()
      vi.advanceTimersByTime(2000)
      expect(cbB).toHaveBeenCalledWith('y')
    })
  })

  describe('playlistNext / playlistPrev', () => {
    it('advances/goes back on the correct deck', () => {
      const cbA = vi.fn()
      const engineA = new PlaylistEngine(['p1', 'p2'], 'sequential', 1000, cbA)
      setPlaylistEngines(engineA, new PlaylistEngine([], 'sequential', 1000, vi.fn()))
      playlistNext('A')
      expect(cbA).toHaveBeenCalledWith('p2')
      playlistPrev('A')
      expect(cbA).toHaveBeenCalledWith('p1')
    })
  })

  describe('setPlaylistBeatSyncInterval', () => {
    it("calls setInterval on the targeted deck's engine, not the other one", () => {
      const engineA = new PlaylistEngine(['p1', 'p2'], 'sequential', 1000, vi.fn())
      const engineB = new PlaylistEngine([], 'sequential', 1000, vi.fn())
      setPlaylistEngines(engineA, engineB)
      const spyA = vi.spyOn(engineA, 'setInterval')
      const spyB = vi.spyOn(engineB, 'setInterval')

      setPlaylistBeatSyncInterval('A', Infinity)

      expect(spyA).toHaveBeenCalledWith(Infinity)
      expect(spyB).not.toHaveBeenCalled()
    })
  })

  describe('exportPlaylists / importPlaylists', () => {
    afterEach(() => {
      vi.unstubAllGlobals()
    })

    it('exports a downloadable blob named opendrop-playlists.json', () => {
      playlistState.aItems = ['p1']
      playlistState.bItems = ['p2']
      const clickSpy = vi.fn()
      const anchor = { href: '', download: '', click: clickSpy }
      vi.stubGlobal('document', { createElement: vi.fn(() => anchor) })
      vi.stubGlobal('URL', { createObjectURL: vi.fn(() => 'blob:mock'), revokeObjectURL: vi.fn() })
      vi.stubGlobal(
        'Blob',
        class {
          constructor(
            public parts: unknown[],
            public opts: unknown
          ) {}
        }
      )

      exportPlaylists()

      expect(anchor.download).toBe('opendrop-playlists.json')
      expect(clickSpy).toHaveBeenCalled()
    })

    it('imports a valid JSON and propagates it to the engines', async () => {
      const engineA = new PlaylistEngine([], 'sequential', 1000, vi.fn())
      const engineB = new PlaylistEngine([], 'sequential', 1000, vi.fn())
      setPlaylistEngines(engineA, engineB)

      class FakeFileReader {
        result: string | null = null
        onload: (() => void) | null = null
        readAsText(_file: unknown) {
          this.result = JSON.stringify({
            playlistA: ['x'],
            playlistB: ['y'],
            intervalSec: 7,
            mode: 'shuffle',
          })
          this.onload?.()
        }
      }
      vi.stubGlobal('FileReader', FakeFileReader)

      const input = { files: [{ name: 'playlists.json' }], value: 'C:\\fakepath\\playlists.json' }
      importPlaylists({ target: input } as unknown as Event)

      expect(playlistState.aItems).toEqual(['x'])
      expect(playlistState.bItems).toEqual(['y'])
      expect(playlistState.intervalSec).toBe(7)
      expect(playlistState.mode).toBe('shuffle')
      expect(input.value).toBe('')
    })

    it('does nothing if no file is selected', () => {
      const input = { files: undefined, value: '' }
      expect(() => importPlaylists({ target: input } as unknown as Event)).not.toThrow()
    })
  })
})
