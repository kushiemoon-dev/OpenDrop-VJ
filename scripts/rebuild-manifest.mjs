#!/usr/bin/env node
/**
 * Reconstruit static/presets/manifest.json depuis les fichiers JSON existants.
 * À lancer quand le manifest est désynchronisé (ex: build sans megapack).
 *
 *   node scripts/rebuild-manifest.mjs
 */
import { readdir, writeFile } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');
const OUT_DIR = join(ROOT, 'static', 'presets');
const BASE_DIR = join(ROOT, 'node_modules', 'butterchurn-presets', 'presets', 'converted');

function slugify(name) {
	return name
		.toLowerCase()
		.replace(/\//g, '-')
		.replace(/[^a-z0-9.\-]/g, '-')
		.replace(/-+/g, '-')
		.replace(/^-+|-+$/g, '')
		.slice(0, 180) || 'preset';
}

// Build slug→name map from butterchurn-presets (exact original names)
const bcFiles = (await readdir(BASE_DIR)).filter(f => f.endsWith('.json'));
const slugToName = new Map();
for (const f of bcFiles) {
	const name = f.slice(0, -5);
	slugToName.set(slugify(name), name);
}

// Scan all preset files
const allFiles = (await readdir(OUT_DIR))
	.filter(f => f.endsWith('.json') && f !== 'manifest.json')
	.sort();

const entries = [];
const seenSlugs = new Set();

for (const f of allFiles) {
	const slug = f.slice(0, -5);
	if (seenSlugs.has(slug)) continue;
	seenSlugs.add(slug);
	// Prefer exact butterchurn-presets name; fall back to slug as display name
	const name = slugToName.get(slug) ?? slug;
	entries.push({ slug, name });
}

const manifest = { version: 1, count: entries.length, entries };
await writeFile(join(OUT_DIR, 'manifest.json'), JSON.stringify(manifest));
console.log(`✅ Manifest reconstruit: ${entries.length} presets`);
console.log(`   (${bcFiles.length} butterchurn-presets avec noms exacts, ${entries.length - bcFiles.length} megapack via slug)`);
