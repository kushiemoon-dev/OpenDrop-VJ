/**
 * Preset registry — wraps butterchurn-presets and provides
 * search / category / favorites helpers.
 *
 * butterchurn-presets ships 1754 pre-converted Milkdrop presets.
 * getPresets() returns the top 100 by default; we use those for now.
 * Dynamic loading of all 1754 will be added in Phase 3.
 */

export interface PresetMeta {
	name: string;
	category: string;
}

let _presets: Record<string, object> | null = null;

/** Load preset map. Safe to call multiple times (cached). */
export async function loadPresets(): Promise<Record<string, object>> {
	if (_presets) return _presets;
	// Dynamic import keeps butterchurn-presets out of the initial bundle
	// and browser-only (never runs server-side because ssr: false).
	const { default: bcp } = await import('butterchurn-presets');
	_presets = bcp.getPresets();
	return _presets as Record<string, object>;
}

/** Extract a rough category from the preset name (author prefix). */
export function getCategory(name: string): string {
	// Many presets are named "Author - Preset Title"
	const dash = name.indexOf(' - ');
	return dash > 0 ? name.slice(0, dash).trim() : 'Other';
}

/** Build a list of preset metadata for the browser panel. */
export function buildPresetList(presets: Record<string, object>): PresetMeta[] {
	return Object.keys(presets)
		.sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
		.map((name) => ({ name, category: getCategory(name) }));
}

/** Filter presets by search query. */
export function searchPresets(list: PresetMeta[], query: string): PresetMeta[] {
	if (!query) return list;
	const q = query.toLowerCase();
	return list.filter((p) => p.name.toLowerCase().includes(q));
}

/** Get unique categories sorted alphabetically. */
export function getCategories(list: PresetMeta[]): string[] {
	return [...new Set(list.map((p) => p.category))].sort();
}
