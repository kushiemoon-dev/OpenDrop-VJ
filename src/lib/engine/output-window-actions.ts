/**
 * output-window-actions.ts — open the output window fullscreen on a chosen
 * display (Electron) or fullscreen the visualizer area (web fallback), and
 * resize the deck canvases/compositor to match the current layout.
 * Extracted from +page.svelte — pure orchestration touching DOM/Electron
 * APIs, never unit tested in this codebase (same precedent as the other
 * *-actions.ts modules).
 *
 * `openOutput` (the "open in a new window" path, as opposed to fullscreen-
 * on-display) stays in +page.svelte — its outputWinRef/outputCloseTimer
 * handles are also read by onDestroy's cleanup, which isn't moving, and
 * threading both through get/set pairs here wasn't worth it for ~25 lines.
 *
 * `busPresetA`/`busPresetB` are $derived values read inside the setTimeout
 * callback below (800ms later) — passed as getters, not frozen snapshots,
 * so a preset change in that window is still reflected (same reasoning as
 * visualizer-startup.ts).
 */

import type { DeckManager } from './deck-manager.js'
import type { Compositor } from './compositor.js'
import type { MainSync } from './sync.js'
import { getQualitySettings } from './quality.js'
import { deckState } from './deck-store.svelte.js'
import { audioSourceState } from './audio-source-store.svelte.js'
import { perfState } from './perf-store.svelte.js'
import { runStatusState } from './run-status-store.svelte.js'

export async function openOutputFullscreen(
  isElectron: boolean,
  selectedDisplayId: number | null,
  sync: MainSync | null,
  getBusPresetA: () => string,
  getBusPresetB: () => string,
  setOutputOpen: (v: boolean) => void
): Promise<void> {
  if (!isElectron) {
    // Web fallback: fullscreen the visualizer area
    const el = document.querySelector('.visualizer-wrap') as HTMLElement | null
    el?.requestFullscreen?.()
    return
  }
  const res = await window.electronAPI!.openOutputOnDisplay(selectedDisplayId)
  if (res?.ok) {
    setOutputOpen(true)
    // Push current state after the window loads
    setTimeout(() => {
      sync?.sendPreset('A', getBusPresetA())
      sync?.sendPreset('B', getBusPresetB())
      sync?.sendCrossfader(deckState.crossfader)
      if (audioSourceState.currentDeviceId) sync?.sendSource(audioSourceState.currentDeviceId)
    }, 800)
  }
}

export function onResize(
  canvases: (HTMLCanvasElement | undefined)[],
  compositorCanvas: HTMLCanvasElement | undefined,
  manager: DeckManager,
  compositor: Compositor | null
): void {
  if (runStatusState.status !== 'running') return
  for (let i = 0; i < 4; i++) {
    const c = canvases[i]
    if (c) manager.resize(i, c.clientWidth, c.clientHeight)
  }
  if (compositorCanvas) {
    compositor?.resize(
      compositorCanvas.clientWidth,
      compositorCanvas.clientHeight,
      getQualitySettings(perfState.quality).pixelRatio
    )
  }
}
