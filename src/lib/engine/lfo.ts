import type { CommandId } from './commands.js'

type LfoShape = 'sine' | 'saw' | 'square' | 'sh'

export interface LfoSlot {
  enabled: boolean
  shape: LfoShape
  /** Multiplier of beat rate: 1 = once per beat, 2 = twice per beat, 0.5 = once per 2 beats */
  rate: number
  /** Phase offset 0..1 */
  offset: number
  /** Center of the modulation range 0..1 */
  center: number
  /** Modulation depth 0..1 (peak deviation from center) */
  amount: number
  /** Target command id (must be 'range' kind). null = no routing. */
  target: CommandId | null
}

export function defaultSlot(): LfoSlot {
  return {
    enabled: false,
    shape: 'sine',
    rate: 1,
    offset: 0,
    center: 0.5,
    amount: 0.5,
    target: null,
  }
}

const LFO_SLOTS = 4

export class LfoEngine {
  readonly slots: LfoSlot[]
  /** S&H values — randomized per-slot on each downbeat. */
  private readonly _shValues: number[]

  constructor() {
    this.slots = Array.from({ length: LFO_SLOTS }, defaultSlot)
    this._shValues = new Array(LFO_SLOTS).fill(0.5)
  }

  /** Call on each downbeat (beat 0 mod N) to refresh S&H samples. */
  randomizeSH(): void {
    for (let i = 0; i < LFO_SLOTS; i++) {
      this._shValues[i] = Math.random()
    }
  }

  /**
   * Compute all LFO values for the given clock phase (0..1 within a beat).
   * Returns an array of { target, value01 } for each enabled slot.
   */
  tick(clockPhase01: number): Array<{ target: CommandId | null; value01: number }> {
    return this.slots.map((slot, i) => ({
      target: slot.enabled ? slot.target : null,
      value01: slot.enabled ? this._compute(slot, clockPhase01, this._shValues[i]!) : slot.center,
    }))
  }

  private _compute(slot: LfoSlot, clockPhase: number, shValue: number): number {
    const p = (clockPhase * slot.rate + slot.offset) % 1
    let raw: number
    switch (slot.shape) {
      case 'sine':
        raw = (Math.sin(p * Math.PI * 2) + 1) / 2
        break
      case 'saw':
        raw = p
        break
      case 'square':
        raw = p < 0.5 ? 1 : 0
        break
      case 'sh':
        raw = shValue
        break
    }
    // amount is the half-range around center (amount=1 → 0..1 when center=0.5)
    const value = slot.center + (raw - 0.5) * slot.amount
    return Math.max(0, Math.min(1, value))
  }
}
