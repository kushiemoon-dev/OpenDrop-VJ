/**
 * Preset registry — all 1754 Milkdrop presets from butterchurn-presets.
 *
 * Names are available immediately (derived from file paths via import.meta.glob).
 * Preset data is loaded lazily on first use and cached.
 */

export interface PresetMeta {
	name: string;
	category: string;
}

// Vite resolves all 1754 JSON paths at build time; actual data loads lazily per preset.
const _modules = import.meta.glob(
	'/node_modules/butterchurn-presets/presets/converted/*.json'
);

// name → module loader
const _loaders = new Map<string, () => Promise<unknown>>();
// name → cached preset data
const _cache = new Map<string, object>();

function pathToName(path: string): string {
	return path.replace(/^.*\/converted\//, '').replace(/\.json$/, '');
}

for (const [path, loader] of Object.entries(_modules)) {
	_loaders.set(pathToName(path), loader as () => Promise<unknown>);
}

/** All preset names, sorted alphabetically. Available synchronously. */
export const allPresetNames: string[] = [..._loaders.keys()].sort(
	(a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' })
);

/** Load a single preset's data (cached). */
export async function loadPresetData(name: string): Promise<object | null> {
	if (_cache.has(name)) return _cache.get(name)!;
	const loader = _loaders.get(name);
	if (!loader) return null;
	const mod = await loader() as { default: object };
	const data = mod.default ?? mod;
	_cache.set(name, data);
	return data;
}

/** Build preset metadata list (synchronous — no data loading needed). */
export function buildPresetList(): PresetMeta[] {
	return allPresetNames.map((name) => ({ name, category: getCategory(name) }));
}

/** Extract author/category from preset name (part before " - "). */
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
