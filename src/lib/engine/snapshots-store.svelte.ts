/**
 * snapshots-store.svelte.ts — reactive wrapper around snapshot/macro slots
 * (1.3). Extracted from +page.svelte, same shape as overlay-store.svelte.ts
 * — mutate the exported state object's fields, never reassign the export.
 *
 * `saveSnapshot` (which reads live command values via the page-level command
 * registry) and the `SnapshotEngine` RAF-driven recall instance stay in
 * +page.svelte — no existing precedent in this codebase for a browser-API/
 * RAF-owning class living in a .svelte.ts store (same reasoning as
 * Compositor/DeckManager staying local). `setSnapshotValues` is the pure
 * state-write half of the old saveSnapshot; +page.svelte computes the values
 * then calls it.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import type { CommandId } from './commands.js';
import type { Snapshot } from './snapshot.js';

export const snapshotsState = $state({
	snapshots: new Array(8).fill(null) as (Snapshot | null)[],
	recallDuration: 2, // seconds, global, shared by all slots
});

export function setSnapshotValues(slot: number, values: Partial<Record<CommandId, number>>): void {
	const existing = snapshotsState.snapshots[slot];
	snapshotsState.snapshots[slot] = { name: existing?.name ?? `Slot ${slot}`, values };
}

export function renameSnapshot(slot: number, name: string): void {
	const s = snapshotsState.snapshots[slot];
	if (s) snapshotsState.snapshots[slot] = { ...s, name };
}

export function clearSnapshot(slot: number): void {
	snapshotsState.snapshots[slot] = null;
}
