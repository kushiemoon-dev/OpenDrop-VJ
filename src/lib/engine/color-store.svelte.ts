/**
 * color-store.svelte.ts — reactive wrapper around per-deck color controls
 * (M3: hue/saturation/brightness/contrast/invert). Extracted from
 * +page.svelte, same shape as overlay-store.svelte.ts — mutate the exported
 * state object's fields, never reassign the export.
 *
 * Command wiring (COLOR_CMDS) and the CSS-filter $derived values stay in
 * +page.svelte — no existing precedent in this codebase for module-level
 * $derived in a .svelte.ts store (same reasoning as playback-store.svelte.ts).
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import { type ColorParams, DEFAULT_COLOR_PARAMS } from './sync.js'

export const colorState = $state({
  a: { ...DEFAULT_COLOR_PARAMS } as ColorParams,
  b: { ...DEFAULT_COLOR_PARAMS } as ColorParams,
})
