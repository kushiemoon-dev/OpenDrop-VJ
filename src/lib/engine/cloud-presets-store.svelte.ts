/**
 * cloud-presets-store.svelte.ts — reactive wrapper around the cloud presets
 * API (cloud-presets.ts). Extracted from +page.svelte: this subsystem never
 * touches decks/manager/sync, so it's fully self-contained. Singleton module,
 * same shape as thumbnailer.svelte.ts — mutate the exported state object's
 * fields, never reassign the export itself.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import {
	type CloudPresetEntry, getOrCreateCloudToken, setCloudToken, parsePresetFile,
	getCloudPresetIndex, uploadPreset, deleteCloudPreset, renameCloudPreset,
} from './cloud-presets.js';

export const cloudPresetsState = $state({
	token: '',
	presets: [] as CloudPresetEntry[],
	error: null as string | null,
	copyLabel: 'Copy my token',
});

/** Get/create the device's cloud token and load its preset list. Call once on mount. */
export async function initCloudPresets(): Promise<void> {
	cloudPresetsState.token = getOrCreateCloudToken();
	await refreshCloudPresets();
}

export async function refreshCloudPresets(): Promise<void> {
	cloudPresetsState.presets = await getCloudPresetIndex(cloudPresetsState.token);
}

export async function onCloudPresetFilePick(e: Event): Promise<void> {
	const input = e.target as HTMLInputElement;
	const file = input.files?.[0];
	input.value = '';
	if (!file) return;
	cloudPresetsState.error = null;
	try {
		const text = await file.text();
		const data = parsePresetFile(text);
		const name = file.name.replace(/\.json$/i, '');
		const result = await uploadPreset(cloudPresetsState.token, name, data);
		if ('error' in result) {
			cloudPresetsState.error = result.error;
			return;
		}
		await refreshCloudPresets();
	} catch (err) {
		cloudPresetsState.error = err instanceof Error ? err.message : 'Invalid preset file';
	}
}

export function copyCloudToken(): void {
	navigator.clipboard.writeText(cloudPresetsState.token);
	cloudPresetsState.copyLabel = 'Copied!';
	setTimeout(() => { cloudPresetsState.copyLabel = 'Copy my token'; }, 1500);
}

export async function linkCloudDevice(token: string): Promise<void> {
	setCloudToken(token);
	cloudPresetsState.token = token;
	await refreshCloudPresets();
}

export async function renameCloudPresetEntry(id: string, name: string): Promise<void> {
	await renameCloudPreset(cloudPresetsState.token, id, name);
	await refreshCloudPresets();
}

export async function deleteCloudPresetEntry(id: string): Promise<void> {
	await deleteCloudPreset(cloudPresetsState.token, id);
	await refreshCloudPresets();
}
