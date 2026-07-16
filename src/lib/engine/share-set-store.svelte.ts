/**
 * share-set-store.svelte.ts — reactive wrapper around the share-set-by-URL
 * panel: the name to embed, the copy-link button's transient label, and a
 * pending set decoded from an incoming #share= URL awaiting confirmation.
 * Extracted from +page.svelte, same shape as overlay-store.svelte.ts —
 * mutate the exported state object's fields, never reassign the export.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import type { SharedSet } from './share-set.js';

export const shareSetState = $state({
	name: '',
	copyLabel: 'Copier le lien',
	pending: null as SharedSet | null,
});
