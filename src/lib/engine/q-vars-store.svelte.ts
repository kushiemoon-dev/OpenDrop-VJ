/**
 * q-vars-store.svelte.ts — reactive wrapper around per-deck Q-var live
 * editing (Track 2). Extracted from +page.svelte, same shape as
 * overlay-store.svelte.ts — mutate the exported state object's fields, never
 * reassign the export.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import {
	type QVarParamsTuple, defaultQVarParams, getGlobalQVarParams, withQVarValue, withQVarWatch, withoutQVarWatch,
} from './q-vars.js';

export const qvarState = $state({
	params: [
		defaultQVarParams(), defaultQVarParams(), defaultQVarParams(), defaultQVarParams(),
	] as QVarParamsTuple,
});

export function updateQVarValue(slot: number, n: number, value: number): void {
	qvarState.params = withQVarValue(qvarState.params, slot, n, value);
	getGlobalQVarParams()[slot].value[n - 1] = value;
}

export function addQVarWatch(slot: number, n: number): void {
	qvarState.params = withQVarWatch(qvarState.params, slot, n);
	getGlobalQVarParams()[slot].enabled[n - 1] = true;
	getGlobalQVarParams()[slot].value[n - 1] = 0;
}

export function removeQVarWatch(slot: number, n: number): void {
	qvarState.params = withoutQVarWatch(qvarState.params, slot, n);
	getGlobalQVarParams()[slot].enabled[n - 1] = false;
}
