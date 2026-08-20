import { Deck, type DeckOptions } from './deck.js'
import { injectTimeParams } from './time-params.js'
import { injectQVarParams } from './q-vars.js'

type SlotEntry = { deck: Deck; canvas: HTMLCanvasElement } | null

/**
 * DeckManager — manages N Butterchurn slots with lazy init and pause/resume.
 *
 * A slot is instantiated on its first start() call and never releases its
 * WebGL context (pause/resume instead). Capped at 4 slots to stay under the
 * browser's WebGL context limit.
 */
export class DeckManager {
  private slots: SlotEntry[] = [null, null, null, null]
  private canvases: (HTMLCanvasElement | null)[] = [null, null, null, null]
  private audioNode: AudioNode | null = null
  private _targetFps = 0 // 0 = unlimited

  attachCanvas(slot: number, canvas: HTMLCanvasElement): void {
    this.canvases[slot] = canvas
  }

  /**
   * Starts or resumes a slot.
   * - First call: creates and initializes a Deck (expensive).
   * - Subsequent calls: calls deck.resume() (instant, no WebGL leak).
   */
  async start(
    slot: number,
    audioCtx: AudioContext,
    audioNode: AudioNode,
    quality: DeckOptions,
    presetData: object | null
  ): Promise<void> {
    this.audioNode = audioNode
    const existing = this.slots[slot]
    if (existing) {
      existing.deck.resume()
      return
    }
    const canvas = this.canvases[slot]
    if (!canvas) throw new Error(`DeckManager: no canvas attached for slot ${slot}`)
    const deck = new Deck(canvas, `deck-${slot}`)
    const w = canvas.clientWidth || 1280
    const h = canvas.clientHeight || 720
    await deck.init(audioCtx, { width: w, height: h, ...quality })
    deck.connectAudio(audioNode)
    if (presetData) deck.loadPreset(injectQVarParams(injectTimeParams(presetData, slot), slot), 0.0)
    deck.startRenderLoop()
    deck.setTargetFps(this._targetFps)
    this.slots[slot] = { deck, canvas }
  }

  pause(slot: number): void {
    this.slots[slot]?.deck.pause()
  }

  resume(slot: number): void {
    const entry = this.slots[slot]
    if (entry && entry.deck.state === 'idle') entry.deck.resume()
  }

  isRunning(slot: number): boolean {
    return this.slots[slot]?.deck.state === 'running'
  }

  loadPreset(slot: number, data: object, blend = 2.0): void {
    this.slots[slot]?.deck.loadPreset(injectQVarParams(injectTimeParams(data, slot), slot), blend)
  }

  /** Re-routes audio to ALL initialized slots (e.g. switching source/loopback). */
  connectAudio(node: AudioNode): void {
    this.audioNode = node
    for (const slot of this.slots) {
      slot?.deck.connectAudio(node)
    }
  }

  applyQuality(opts: {
    meshWidth: number
    meshHeight: number
    pixelRatio: number
    textureRatio: number
    outputFXAA: boolean
  }): void {
    for (const slot of this.slots) {
      slot?.deck.applyQuality(opts)
    }
  }

  /**
   * Set a global FPS cap applied to all current and future slots.
   * @param fps  Target frames per second. 0 = unlimited.
   */
  setTargetFps(fps: number): void {
    this._targetFps = fps
    for (const slot of this.slots) slot?.deck.setTargetFps(fps)
  }

  /**
   * Set a per-slot FPS cap without changing the global default.
   * @param slot  Slot index (0–3).
   * @param fps   Target frames per second. 0 = unlimited.
   */
  setSlotTargetFps(slot: number, fps: number): void {
    this.slots[slot]?.deck.setTargetFps(fps)
  }

  resize(slot: number, w: number, h: number): void {
    this.slots[slot]?.deck.resize(w, h)
  }

  getRenderCount(slot: number): number {
    return this.slots[slot]?.deck.renderCount ?? 0
  }

  runningCount(): number {
    return this.slots.filter((s) => s?.deck.state === 'running').length
  }

  destroyAll(): void {
    for (const slot of this.slots) {
      slot?.deck.destroy()
    }
    this.slots = [null, null, null, null]
  }
}
