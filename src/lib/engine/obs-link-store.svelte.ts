/**
 * obs-link-store.svelte.ts — OBS connection state + the user-configured
 * scene ⇄ {slot|mood} mapping table (Task 6's MappingEntry[]).
 */

import type { MappingEntry } from './obs-mapping.js';

export const obsLinkState = $state({
	connected: false,
	error: '',
	host: 'localhost',
	port: 4455,
	scenes: [] as string[],
	mapping: [] as MappingEntry[],
});
