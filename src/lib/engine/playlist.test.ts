import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { PlaylistEngine } from './playlist.js'

beforeEach(() => {
  vi.useFakeTimers()
})
afterEach(() => {
  vi.useRealTimers()
})

describe('PlaylistEngine', () => {
  it('start() loads the current preset immediately', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb)
    pl.start()
    expect(cb).toHaveBeenCalledOnce()
    expect(cb).toHaveBeenCalledWith('A')
  })

  it('start() schedules the next one after intervalMs', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb)
    pl.start()
    cb.mockClear()
    vi.advanceTimersByTime(5000)
    expect(cb).toHaveBeenCalledWith('B')
  })

  it('sequential cycle loops around', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B'], 'sequential', 1000, cb)
    pl.start()
    cb.mockClear()
    vi.advanceTimersByTime(1000) // B
    vi.advanceTimersByTime(1000) // A
    vi.advanceTimersByTime(1000) // B
    expect(cb.mock.calls.map((c) => c[0])).toEqual(['B', 'A', 'B'])
  })

  it('stop() stops the cycle', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 1000, cb)
    pl.start()
    pl.stop()
    cb.mockClear()
    vi.advanceTimersByTime(5000)
    expect(cb).not.toHaveBeenCalled()
  })

  it('next() advances manually', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb)
    pl.next()
    expect(cb).toHaveBeenCalledWith('B')
  })

  it('prev() goes back manually', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb)
    pl.next() // → B
    pl.prev() // → A
    expect(cb).toHaveBeenLastCalledWith('A')
  })

  it('prev() from index 0 goes to the last one', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 5000, cb)
    pl.prev()
    expect(cb).toHaveBeenCalledWith('C')
  })

  it('does not start if the list is empty', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine([], 'sequential', 1000, cb)
    pl.start()
    expect(pl.playing).toBe(false)
    expect(cb).not.toHaveBeenCalled()
  })

  it('setItems() resets index if out of bounds', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', 1000, cb)
    pl.next() // idx=1
    pl.next() // idx=2
    pl.setItems(['X']) // idx should reset to 0
    pl.start()
    expect(cb).toHaveBeenCalledWith('X')
  })

  it('setInterval() updates the cycle duration', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B'], 'sequential', 5000, cb)
    pl.setInterval(1000)
    pl.start()
    cb.mockClear()
    vi.advanceTimersByTime(1000)
    expect(cb).toHaveBeenCalledWith('B')
  })

  it('setInterval(Infinity) disables automatic advance (beat-sync mode)', () => {
    const cb = vi.fn()
    const pl = new PlaylistEngine(['A', 'B', 'C'], 'sequential', Infinity, cb)
    pl.start()
    cb.mockClear()
    vi.advanceTimersByTime(100000)
    expect(cb).not.toHaveBeenCalled()
  })

  it('playing correctly reflects start/stop', () => {
    const pl = new PlaylistEngine(['A', 'B'], 'sequential', 1000, vi.fn())
    expect(pl.playing).toBe(false)
    pl.start()
    expect(pl.playing).toBe(true)
    pl.stop()
    expect(pl.playing).toBe(false)
  })
})
