export type PlaylistMode = 'sequential' | 'shuffle'

export class PlaylistEngine {
  private timerId: ReturnType<typeof setTimeout> | null = null
  private _index = 0
  private _playing = false

  constructor(
    private items: string[],
    private mode: PlaylistMode,
    private intervalMs: number,
    private onPreset: (name: string) => void
  ) {}

  get playing() {
    return this._playing
  }
  get currentIndex() {
    return this._index
  }

  start() {
    if (this._playing || this.items.length === 0) return
    this._playing = true
    this.onPreset(this.items[this._index]!) // load the current preset immediately
    this._schedule()
  }

  stop() {
    this._playing = false
    if (this.timerId !== null) {
      clearTimeout(this.timerId)
      this.timerId = null
    }
  }

  next() {
    if (this.items.length === 0) return
    this._index =
      this.mode === 'shuffle' ? this._randomIndex() : (this._index + 1) % this.items.length
    this.onPreset(this.items[this._index]!)
    if (this._playing) {
      this.stop()
      this._playing = true
      this._schedule()
    }
  }

  prev() {
    if (this.items.length === 0) return
    this._index = (this._index - 1 + this.items.length) % this.items.length
    this.onPreset(this.items[this._index]!)
  }

  setItems(items: string[]) {
    this.items = items
    if (this._index >= items.length) this._index = 0
  }

  setInterval(ms: number) {
    this.intervalMs = ms
  }

  setMode(mode: PlaylistMode) {
    this.mode = mode
  }

  destroy() {
    this.stop()
  }

  private _schedule() {
    // Infinity is the beat-sync sentinel (advance is driven externally by
    // beat triggers, not this timer). setTimeout clamps Infinity to ~0ms,
    // which would otherwise fire almost continuously.
    if (!Number.isFinite(this.intervalMs)) return
    this.timerId = setTimeout(() => {
      if (!this._playing) return
      this.next()
    }, this.intervalMs)
  }

  private _randomIndex(): number {
    if (this.items.length <= 1) return 0
    let idx: number
    do {
      idx = Math.floor(Math.random() * this.items.length)
    } while (idx === this._index)
    return idx
  }
}
