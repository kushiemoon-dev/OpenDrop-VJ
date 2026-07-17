/**
 * deck-preset-actions.ts — load a preset (from the cloud/local library, or a
 * freshly-imported .milk/.prjm file) into the currently active deck slot.
 * Extracted from +page.svelte — pure orchestration touching the DeckManager/
 * MainSync browser-facing instances, never unit tested in this codebase
 * (same precedent as the other *-actions.ts modules).
 *
 * `primaryPreset` is passed in rather than imported — it stays in
 * +page.svelte because it needs `manager`/`isRunning` and the page-level
 * `presets4` $derived.
 */

import type { DeckManager } from './deck-manager.js';
import type { MainSync } from './sync.js';
import { loadPresetData } from '../presets/index.js';
import { convertMilkPreset } from '../presets/milk-import.js';
import { deckState } from './deck-store.svelte.js';

function setSlotPreset(slot: number, name: string): void {
	if (slot === 0) deckState.presetA = name;
	else if (slot === 1) deckState.presetB = name;
	else if (slot === 2) deckState.preset2 = name;
	else deckState.preset3 = name;
}

export async function selectPreset(
	slot: number, name: string, manager: DeckManager, sync: MainSync | null, primaryPreset: (bus: 'A' | 'B') => string,
): Promise<void> {
	const d = await loadPresetData(name);
	if (!d) return;
	setSlotPreset(slot, name);
	manager.loadPreset(slot, d, deckState.transitionTime);
	deckState.slotEpoch++;
	const bus = deckState.deckBus[slot];
	if (bus === 'A') sync?.sendPreset('A', primaryPreset('A'), deckState.transitionTime);
	else if (bus === 'B') sync?.sendPreset('B', primaryPreset('B'), deckState.transitionTime);
}

/** Import a dropped .milk/.prjm preset directly into activeSlot (mirrors selectPreset,
 * minus the loadPresetData lookup — the converted data is already in hand). */
export async function loadImportedMilkPreset(
	file: File, manager: DeckManager, sync: MainSync | null, primaryPreset: (bus: 'A' | 'B') => string,
): Promise<void> {
	let data: object;
	try {
		data = await convertMilkPreset(await file.text());
	} catch {
		return; // not a valid MilkDrop preset — silently skipped, like other unrecognized drop types
	}
	const name = file.name.replace(/\.(milk|prjm)$/i, '');
	const slot = deckState.activeSlot;
	setSlotPreset(slot, name);
	manager.loadPreset(slot, data, deckState.transitionTime);
	deckState.slotEpoch++;
	const bus = deckState.deckBus[slot];
	// Attach `data` only when this import is the bus's current primary preset —
	// otherwise the synced name refers to some other, normally-resolvable preset.
	if (bus === 'A') { const p = primaryPreset('A'); sync?.sendPreset('A', p, deckState.transitionTime, p === name ? data : undefined); }
	else if (bus === 'B') { const p = primaryPreset('B'); sync?.sendPreset('B', p, deckState.transitionTime, p === name ? data : undefined); }
}
