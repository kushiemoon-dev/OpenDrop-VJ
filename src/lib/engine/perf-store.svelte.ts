/**
 * perf-store.svelte.ts — reactive wrapper around render quality and
 * invisible-deck performance settings. Extracted from +page.svelte, same
 * shape as color-store.svelte.ts — plain $state, mutated directly by
 * command wiring and template callbacks in +page.svelte, no dedicated
 * setters.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import { type QualityTier, type InvisibleMode, DEFAULT_TIER, DEFAULT_PERF } from './quality.js';

export const perfState = $state({
	quality: DEFAULT_TIER as QualityTier,
	targetFps: DEFAULT_PERF.targetFps,
	invisibleMode: DEFAULT_PERF.invisibleMode as InvisibleMode,
	invisibleFps: DEFAULT_PERF.invisibleFps,
});
