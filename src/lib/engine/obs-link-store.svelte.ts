/**
 * obs-link-store.svelte.ts — OBS connection state + the user-configured
 * scene ⇄ {slot|mood} mapping table (Task 6's MappingEntry[]). host/port/
 * mapping persist across restarts (od-obs-config) — connected/error/scenes
 * are session-only, re-derived on each real connection.
 */

import type { MappingEntry } from './obs-mapping.js';

const CONFIG_KEY = 'od-obs-config';

interface PersistedObsConfig {
	host: string;
	port: number;
	mapping: MappingEntry[];
}

function loadPersistedConfig(): PersistedObsConfig {
	try {
		const raw = localStorage.getItem(CONFIG_KEY);
		if (!raw) return { host: 'localhost', port: 4455, mapping: [] };
		const parsed = JSON.parse(raw);
		return {
			host: typeof parsed.host === 'string' ? parsed.host : 'localhost',
			port: typeof parsed.port === 'number' ? parsed.port : 4455,
			mapping: Array.isArray(parsed.mapping) ? parsed.mapping : [],
		};
	} catch {
		return { host: 'localhost', port: 4455, mapping: [] };
	}
}

const persisted = loadPersistedConfig();

export const obsLinkState = $state({
	connected: false,
	error: '',
	host: persisted.host,
	port: persisted.port,
	scenes: [] as string[],
	mapping: persisted.mapping,
	recording: false,
	recordError: '',
});

export function saveObsConfig(): void {
	const config: PersistedObsConfig = {
		host: obsLinkState.host,
		port: obsLinkState.port,
		mapping: obsLinkState.mapping,
	};
	localStorage.setItem(CONFIG_KEY, JSON.stringify(config));
}
