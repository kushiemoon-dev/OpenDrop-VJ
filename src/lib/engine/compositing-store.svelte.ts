/**
 * compositing-store.svelte.ts — reactive wrapper around per-slot compositing
 * config (blend mode + LumaKey/ColorKey). Extracted from +page.svelte, same
 * shape as overlay-store.svelte.ts — mutate the exported state object's
 * fields, never reassign the export.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import { type SlotComposite, DEFAULT_SLOT_COMPOSITE } from './sync.js';
import { type SlotComposites, withSlotComposite } from './compositor.js';

export const compositingState = $state({
	slotComposites: [
		{ ...DEFAULT_SLOT_COMPOSITE }, { ...DEFAULT_SLOT_COMPOSITE }, { ...DEFAULT_SLOT_COMPOSITE }, { ...DEFAULT_SLOT_COMPOSITE },
	] as SlotComposites,
});

export function updateComposite(slot: number, patch: Partial<SlotComposite>): void {
	compositingState.slotComposites = withSlotComposite(compositingState.slotComposites, slot, patch);
}
