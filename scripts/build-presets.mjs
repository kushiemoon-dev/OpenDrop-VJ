#!/usr/bin/env node
/**
 * Génère static/presets/ à partir de :
 *   1. node_modules/butterchurn-presets/presets/converted/ (1754 presets de base)
 *   2. MEGAPACK_DIR (optionnel, par défaut /tmp/milkdrop-megapack-extracted/converted)
 *
 * Crée :
 *   - static/presets/<slug>.json  (un fichier par preset, slugifié URL-safe)
 *   - static/presets/manifest.json  ({ version, count, entries: [{slug, name}] })
 *
 * Usage :
 *   node scripts/build-presets.mjs
 *   MEGAPACK_DIR=/chemin/vers/converted node scripts/build-presets.mjs
 */

import { readdir, copyFile, mkdir, writeFile, access } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');
const OUT_DIR = join(ROOT, 'static', 'presets');
const BASE_DIR = join(ROOT, 'node_modules', 'butterchurn-presets', 'presets', 'converted');
const MEGAPACK_DIR = process.env.MEGAPACK_DIR ?? '/tmp/milkdrop-megapack-extracted/converted';
const CONCURRENCY = 32;

function slugify(name) {
	return name
		.toLowerCase()
		.replace(/\//g, '-')
		.replace(/[^a-z0-9.\-]/g, '-')
		.replace(/-+/g, '-')
		.replace(/^-+|-+$/g, '')
		.slice(0, 180) || 'preset';
}

async function exists(p) {
	try { await access(p); return true; } catch { return false; }
}

async function runConcurrent(tasks, limit) {
	let i = 0;
	async function worker() {
		while (i < tasks.length) {
			const task = tasks[i++];
			try { await task(); } catch {}
		}
	}
	await Promise.all(Array.from({ length: limit }, worker));
}

async function processDir(dir, label, seenNorm, seenSlugs, entries, copyTasks) {
	if (!(await exists(dir))) {
		console.log(`⏭  ${label}: dossier absent (${dir}), ignoré`);
		return;
	}
	const files = (await readdir(dir)).filter(f => f.endsWith('.json'));
	let added = 0, duped = 0;

	for (const file of files) {
		const name = file.slice(0, -5);
		const norm = name.toLowerCase().trim().replace(/\s+/g, ' ');
		if (seenNorm.has(norm)) { duped++; continue; }
		seenNorm.add(norm);

		let slug = slugify(name);
		let final = slug;
		let n = 2;
		while (seenSlugs.has(final)) final = `${slug}-${n++}`;
		seenSlugs.add(final);

		const src = join(dir, file);
		const dst = join(OUT_DIR, `${final}.json`);
		copyTasks.push(() => copyFile(src, dst));
		entries.push({ slug: final, name });
		added++;
	}
	console.log(`✓  ${label}: ${added} ajoutés, ${duped} dédupliqués`);
}

async function main() {
	console.log('🎵 build-presets: génération de static/presets/...');
	await mkdir(OUT_DIR, { recursive: true });

	const seenNorm = new Set();
	const seenSlugs = new Set();
	const entries = [];
	const copyTasks = [];

	await processDir(BASE_DIR, 'butterchurn-presets (base)', seenNorm, seenSlugs, entries, copyTasks);
	await processDir(MEGAPACK_DIR, 'megapack ansorre', seenNorm, seenSlugs, entries, copyTasks);

	console.log(`📋 Copie de ${copyTasks.length} fichiers (concurrence ${CONCURRENCY})...`);
	await runConcurrent(copyTasks, CONCURRENCY);

	// Si le megapack était absent, des fichiers JSON existants dans OUT_DIR
	// (d'un build précédent) ne sont pas dans entries — on les préserve dans le manifest
	// plutôt que de les perdre.
	const megapackAbsent = !(await exists(MEGAPACK_DIR));
	if (megapackAbsent) {
		const existingFiles = (await readdir(OUT_DIR))
			.filter(f => f.endsWith('.json') && f !== 'manifest.json');
		const newSlugs = new Set(entries.map(e => e.slug));
		let preserved = 0;
		for (const f of existingFiles) {
			const slug = f.slice(0, -5);
			if (!newSlugs.has(slug)) {
				entries.push({ slug, name: slug });
				preserved++;
			}
		}
		if (preserved > 0) {
			console.log(`⚠️  MEGAPACK_DIR absent — ${preserved} presets existants préservés via slug`);
			console.log(`   Pour des noms complets, relancer avec : MEGAPACK_DIR=/chemin/vers/converted node scripts/build-presets.mjs`);
		}
	}

	const manifest = { version: 1, count: entries.length, entries };
	await writeFile(join(OUT_DIR, 'manifest.json'), JSON.stringify(manifest));
	console.log(`✅ Done: ${entries.length} presets → ${OUT_DIR}`);
}

main().catch(e => { console.error('❌', e); process.exit(1); });
