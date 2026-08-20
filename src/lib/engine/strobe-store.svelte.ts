/**
 * strobe-store.svelte.ts — reactive wrapper around the strobe effect's
 * on/off, rate, intensity and color. Extracted from +page.svelte, same shape
 * as color-store.svelte.ts — plain $state, mutated directly by command
 * wiring and template callbacks in +page.svelte, no dedicated setters.
 *
 * `_lastStrobeVal` (edge-detection bookkeeping for the VU-meter tick loop)
 * stays in +page.svelte as a plain non-reactive local — same category as
 * `pausedSlots`/`lastFps` in the Performance decks section.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

export const strobeState = $state({
  on: false,
  /** Beats per flash cycle. 0.25=1/4beat, 0.5=half, 1=beat, 2=half-tempo, 4=quarter-tempo */
  rate: 1,
  intensity: 0.8,
  color: '#ffffff',
  flash: false,
})
