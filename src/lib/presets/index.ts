/**
 * Preset registry — 16,375 presets served as static files.
 *
 * Call `await initPresets()` once before any other usage.
 * After that, all the functions below are synchronous, except loadPresetData.
 */

import { base } from '$app/paths';
import { loadCloudPresetData, CLOUD_PRESET_PREFIX } from '../engine/cloud-presets.js';

export interface PresetMeta {
	name: string;
	category: string;
}

interface ManifestEntry { slug: string; name: string; }

let _nameToSlug = new Map<string, string>();
const _cache = new Map<string, object>();
let _initialized = false;

/** Names of all presets, available after initPresets(). */
export let allPresetNames: string[] = [];

/** Load the manifest. Call once in onMount before any other usage. */
export async function initPresets(): Promise<void> {
	if (_initialized) return;
	const res = await fetch(`${base}/presets/manifest.json`);
	if (!res.ok) throw new Error(`Manifest fetch failed: ${res.status}`);
	const manifest: { entries: ManifestEntry[] } = await res.json();
	for (const { slug, name } of manifest.entries) {
		_nameToSlug.set(name, slug);
	}
	allPresetNames = manifest.entries.map(e => e.name);
	_initialized = true;
}

/** Load a preset's data (cached). */
export async function loadPresetData(name: string): Promise<object | null> {
	if (_cache.has(name)) return _cache.get(name)!;
	const slug = _nameToSlug.get(name);
	if (!slug) {
		if (!name.startsWith(CLOUD_PRESET_PREFIX)) return null;
		const token = localStorage.getItem('od-cloud-token');
		if (!token) return null;
		const data = await loadCloudPresetData(token, name);
		if (data) _cache.set(name, data);
		return data;
	}
	try {
		const res = await fetch(`${base}/presets/${encodeURIComponent(slug)}.json`);
		if (!res.ok) return null;
		const data = await res.json() as object;
		_cache.set(name, data);
		return data;
	} catch {
		return null;
	}
}

/** Build the full preset list with category. */
export function buildPresetList(): PresetMeta[] {
	return allPresetNames.map((name) => ({ name, category: getCategory(name) }));
}

/** Extract the category/author from the name (the part before " - "). */
export function getCategory(name: string): string {
	const dash = name.indexOf(' - ');
	return dash > 0 ? name.slice(0, dash).trim() : 'Other';
}

/** Filter presets by search query. */
export function searchPresets(list: PresetMeta[], query: string): PresetMeta[] {
	if (!query) return list;
	const q = query.toLowerCase();
	return list.filter((p) => p.name.toLowerCase().includes(q));
}

/** Get a preset's slug by its name. */
export function getSlug(name: string): string | undefined {
	return _nameToSlug.get(name);
}
