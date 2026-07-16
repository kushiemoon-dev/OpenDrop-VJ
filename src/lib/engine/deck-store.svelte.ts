/**
 * deck-store.svelte.ts — reactive wrapper around the core deck/mixer state:
 * which slot the preset browser targets, each slot's loaded preset name,
 * the A/B/off bus assignment per slot, the crossfader position, and the
 * preset transition time. Extracted from +page.svelte, same shape as
 * overlay-store.svelte.ts — mutate the exported state object's fields,
 * never reassign the export.
 *
 * `slotEpoch` is a manual reactive-tracking counter (see primaryPreset() in
 * +page.svelte, which does `void deckState.slotEpoch` to force $derived
 * re-evaluation after a preset loads without itself being one of that
 * function's normal dependencies) — bump it, don't read it for meaning.
 *
 * All `$derived` values built on top of this state (activeDeck, activePreset,
 * presets4, opacities, presetIdxA/B, busPresetA/B) and primaryPreset() itself
 * stay in +page.svelte — no existing precedent in this codebase for
 * module-level $derived in a .svelte.ts store, and primaryPreset() also
 * needs the page-local `manager` instance.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

export const deckState = $state({
	activeSlot: 0, // 0=A 1=B 2=C 3=D — cible du preset browser
	presetA: '',
	presetB: '',
	preset2: '',
	preset3: '',
	deckBus: ['A', 'B', 'off', 'off'] as Array<'A' | 'B' | 'off'>,
	crossfader: 0, // 0 = 100% A, 1 = 100% B
	transitionTime: 2.0, // secondes de fondu preset (0 = hard cut)
	slotEpoch: 0,
});
