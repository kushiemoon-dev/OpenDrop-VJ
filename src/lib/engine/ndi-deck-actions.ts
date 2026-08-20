/**
 * ndi-deck-actions.ts — start/stop per-slot NDI senders and the canvas-readback
 * loop feeding them. Extends the electron-features-actions.ts pattern — pure
 * orchestration over window.electronAPI and the Canvas 2D API, both browser
 * boundaries never unit tested in this codebase (no DOM in this suite's
 * `environment: 'node'` vitest config) — verified instead by Task 4's live
 * validation in the real running app.
 */

import { ndiDeckState } from './ndi-deck-store.svelte.js'

/** ndiDeckState.slots always has one entry per deck slot (0-3, see the store's header comment). */
function slotState(slot: number) {
  return ndiDeckState.slots[slot]!
}

const SLOT_NAMES = [
  'OpenDrop — Deck A',
  'OpenDrop — Deck B',
  'OpenDrop — Deck C',
  'OpenDrop — Deck D',
]

// Independent from the 60fps compositor loop — per-deck NDI is a secondary output,
// sampled at a lower rate to bound the added readback cost (see spec's risk note).
const SAMPLE_INTERVAL_MS = 1000 / 15

const loopHandles: Record<number, number> = {}
const lastSampleAt: Record<number, number> = {}
const helperCanvases: Record<number, HTMLCanvasElement> = {}

// Per-slot in-flight guard, separate from `active` — closes the start/stop
// interleaving race (Task 3 review carryover): `active` flips synchronously
// while the IPC round-trip (ndiDeckStart/ndiDeckStop) is still pending, so a
// second toggle arriving mid-transition must be ignored instead of racing the
// first call's await. Pure array, no DOM/window access.
const pendingSlots: boolean[] = [false, false, false, false]

/** Marks `slot` as mid-transition. Returns false (no-op) if already pending. */
export function beginNdiTransition(slot: number): boolean {
  if (pendingSlots[slot]) return false
  pendingSlots[slot] = true
  return true
}

/** Clears the in-flight guard for `slot`. */
export function endNdiTransition(slot: number): void {
  pendingSlots[slot] = false
}

/**
 * Copies `source` into a scratch 2D canvas and reads it back as RGBA bytes.
 * Works regardless of source's own context type (2D or WebGL) — drawImage()
 * accepts any canvas element as an image source, the same way compositor.ts
 * already treats deck canvases as opaque texture sources.
 */
function sampleCanvasRGBA(
  source: HTMLCanvasElement,
  helper: HTMLCanvasElement
): { width: number; height: number; buffer: ArrayBuffer } | null {
  const width = source.width
  const height = source.height
  if (!width || !height) return null

  helper.width = width
  helper.height = height
  const ctx = helper.getContext('2d')
  if (!ctx) return null

  ctx.drawImage(source, 0, 0, width, height)
  const imageData = ctx.getImageData(0, 0, width, height)
  return { width, height, buffer: imageData.data.buffer }
}

function tick(slot: number, canvas: HTMLCanvasElement): void {
  loopHandles[slot] = requestAnimationFrame(() => tick(slot, canvas))

  const now = performance.now()
  if ((lastSampleAt[slot] ?? 0) + SAMPLE_INTERVAL_MS > now) return
  lastSampleAt[slot] = now

  const helper = helperCanvases[slot] ?? (helperCanvases[slot] = document.createElement('canvas'))
  const sample = sampleCanvasRGBA(canvas, helper)
  if (!sample) return

  window.electronAPI?.sendDeckFrame(slot, sample.width, sample.height, sample.buffer)
}

export async function startNdiDeck(slot: number, canvas: HTMLCanvasElement): Promise<void> {
  if (slotState(slot).active) return
  if (!beginNdiTransition(slot)) return
  try {
    // Set synchronously, before the await below — this is what actually closes the
    // re-entrancy window. A second call arriving while the IPC round-trip is in
    // flight must see `active` already true; setting it only after `res.ok` (as
    // before) would leave `active` false for the whole await, so the guard above
    // would never trip and two tick() chains could still start.
    slotState(slot).active = true
    slotState(slot).error = ''
    const res = await window.electronAPI?.ndiDeckStart(slot, SLOT_NAMES[slot]!)
    if (!res?.ok) {
      slotState(slot).active = false
      slotState(slot).error =
        res?.error ?? 'NDI SDK not found — install the NDI Runtime from ndi.video.'
      return
    }
    tick(slot, canvas)
  } finally {
    endNdiTransition(slot)
  }
}

export async function stopNdiDeck(slot: number): Promise<void> {
  if (!beginNdiTransition(slot)) return
  try {
    if (loopHandles[slot] !== undefined) {
      cancelAnimationFrame(loopHandles[slot])
      delete loopHandles[slot]
    }
    await window.electronAPI?.ndiDeckStop(slot)
    slotState(slot).active = false
  } finally {
    endNdiTransition(slot)
  }
}
