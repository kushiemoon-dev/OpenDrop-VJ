/**
 * time-params-store.svelte.ts — reactive wrapper around per-deck Time param
 * sliders (1.4). Extracted from +page.svelte, same shape as
 * overlay-store.svelte.ts — mutate the exported state object's fields, never
 * reassign the export.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import {
	type DeckTimeParams, type TimeParamsTuple, defaultTimeParams, getGlobalTimeParams, withTimeParams,
} from './time-params.js';

export const timeParamsState = $state({
	params: [
		defaultTimeParams(), defaultTimeParams(), defaultTimeParams(), defaultTimeParams(),
	] as TimeParamsTuple,
});

export function updateTimeParams(slot: number, patch: Partial<DeckTimeParams>): void {
	timeParamsState.params = withTimeParams(timeParamsState.params, slot, patch);
	// Write-through: this is what Butterchurn's injected preset code actually
	// reads every frame — the $state above is only for the UI to bind to.
	Object.assign(getGlobalTimeParams()[slot], patch);
}
