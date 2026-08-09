#!/usr/bin/env node
/**
 * Removes any static/presets/*.json that fails to compile (see
 * lib/validate-preset.mjs) and drops it from manifest.json. Needed after
 * extracting the official presets-megapack.tar.gz release asset directly
 * into static/ — that path (used by .github/workflows/release.yml and
 * cli/build.sh) bypasses build-presets.mjs entirely, so nothing else
 * validates those presets before they ship.
 *
 * Usage: node scripts/prune-invalid-presets.mjs
 */

import { readFile, unlink, writeFile } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isValid } from './lib/validate-preset.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const OUT_DIR = join(ROOT, 'static', 'presets');
const MANIFEST_PATH = join(OUT_DIR, 'manifest.json');

async function main() {
	const manifest = JSON.parse(await readFile(MANIFEST_PATH, 'utf8'));
	const kept = [];
	let removed = 0;

	for (const entry of manifest.entries) {
		const file = join(OUT_DIR, `${entry.slug}.json`);
		let preset;
		try {
			preset = JSON.parse(await readFile(file, 'utf8'));
		} catch {
			removed++;
			continue;
		}
		if (isValid(preset)) {
			kept.push(entry);
		} else {
			removed++;
			await unlink(file).catch(() => {});
		}
	}

	manifest.entries = kept;
	manifest.count = kept.length;
	await writeFile(MANIFEST_PATH, JSON.stringify(manifest));
	console.log(`✅ ${kept.length} presets kept, ${removed} removed (failed to compile)`);
}

main().catch(e => { console.error('❌', e); process.exit(1); });
