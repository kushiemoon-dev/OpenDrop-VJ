#!/usr/bin/env node
/**
 * Génère static/presets/ à partir de :
 *   1. node_modules/butterchurn-presets/presets/converted/ (1754 presets de base,
 *      déjà au format Butterchurn — simple copie)
 *   2. MEGAPACK_DIR (optionnel, par défaut /tmp/milkdrop-megapack-extracted,
 *      un dossier de presets .milk/.prjm BRUTS — convertis ici via
 *      milkdrop-preset-converter, le même convertisseur que milk-import.ts
 *      utilise pour l'import drag-and-drop côté navigateur)
 *
 * Crée :
 *   - static/presets/<slug>.json  (un fichier par preset, slugifié URL-safe)
 *   - static/presets/manifest.json  ({ version, count, entries: [{slug, name}] })
 *
 * Usage :
 *   node scripts/build-presets.mjs
 *   MEGAPACK_DIR=/chemin/vers/presets-milk node scripts/build-presets.mjs
 */

import { readdir, readFile, mkdir, writeFile, access } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import milkdropPresetConverter from 'milkdrop-preset-converter';
import { isValid } from './lib/validate-preset.mjs';

const { convertPreset } = milkdropPresetConverter;

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');
const OUT_DIR = join(ROOT, 'static', 'presets');
const BASE_DIR = join(ROOT, 'node_modules', 'butterchurn-presets', 'presets', 'converted');
const MEGAPACK_DIR = process.env.MEGAPACK_DIR ?? '/tmp/milkdrop-megapack-extracted';
const CONCURRENCY = 32;

async function copyJson(src, dst) {
	const preset = JSON.parse(await readFile(src, 'utf8'));
	if (!isValid(preset)) throw new Error('invalid preset (fails to compile)');
	await writeFile(dst, JSON.stringify(preset));
}

// .milk EEL code has quirks that aren't legal JS as-is (e.g. `if(cond,a,b)`
// used as an expression — `if` is a reserved word, so a naive text copy of a
// pre-"converted" pack can leave that in verbatim and crash Butterchurn's
// `new Function(...)` with "Unexpected token 'if'"/"'return'" on load.
// convertPreset() runs an actual EEL parser and emits real JS (e.g. that
// `if(...)` becomes a ternary), so raw .milk sources must go through it
// rather than being copied as pre-converted JSON from elsewhere.
async function convertMilk(src, dst) {
	const text = await readFile(src, 'latin1');
	const preset = await convertPreset(text);
	if (!isValid(preset)) throw new Error('invalid preset (fails to compile)');
	await writeFile(dst, JSON.stringify(preset));
}

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

// Each task resolves to its manifest entry on success, or null if the
// preset was invalid/failed conversion — the caller filters nulls out
// before writing manifest.json, so a bad preset never gets listed pointing
// at a file that was never written.
async function runConcurrent(tasks, limit) {
	const results = new Array(tasks.length);
	let i = 0, failed = 0;
	async function worker() {
		while (i < tasks.length) {
			const idx = i++;
			try { results[idx] = await tasks[idx](); }
			catch { failed++; results[idx] = null; }
		}
	}
	await Promise.all(Array.from({ length: limit }, worker));
	return { results, failed };
}

async function processDir(dir, label, ext, convert, seenNorm, seenSlugs, copyTasks) {
	if (!(await exists(dir))) {
		console.log(`⏭  ${label}: dossier absent (${dir}), ignoré`);
		return;
	}
	const files = (await readdir(dir)).filter(f => f.endsWith(ext));
	let added = 0, duped = 0;

	for (const file of files) {
		const name = file.slice(0, -ext.length);
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
		copyTasks.push(async () => { await convert(src, dst); return { slug: final, name }; });
		added++;
	}
	console.log(`✓  ${label}: ${added} ajoutés, ${duped} dédupliqués`);
}

async function main() {
	console.log('🎵 build-presets: génération de static/presets/...');
	await mkdir(OUT_DIR, { recursive: true });

	const seenNorm = new Set();
	const seenSlugs = new Set();
	const copyTasks = [];

	await processDir(BASE_DIR, 'butterchurn-presets (base)', '.json', copyJson, seenNorm, seenSlugs, copyTasks);
	await processDir(MEGAPACK_DIR, 'megapack (raw .milk)', '.milk', convertMilk, seenNorm, seenSlugs, copyTasks);

	console.log(`📋 Traitement de ${copyTasks.length} fichiers (concurrence ${CONCURRENCY})...`);
	const { results, failed } = await runConcurrent(copyTasks, CONCURRENCY);
	const entries = results.filter(Boolean);
	if (failed > 0) console.log(`⚠️  ${failed} presets invalides/en échec, exclus de static/presets/`);

	// Si le megapack était absent, des fichiers JSON existants dans OUT_DIR
	// (d'un build précédent, ou d'un tar extrait directement — voir
	// cli/build.sh / release.yml) ne sont pas dans entries — on les préserve
	// dans le manifest plutôt que de les perdre. Le manifest.json déjà présent
	// (s'il existe) est la seule source des vrais noms : sans lui on ne peut
	// que retomber sur slug === name, ce qui a déjà écrasé silencieusement
	// 16k+ noms lisibles en un README-first-line-friendly slug lors d'un
	// `pnpm build` roulant ce script une 2e fois après un tar direct.
	const megapackAbsent = !(await exists(MEGAPACK_DIR));
	if (megapackAbsent) {
		const manifestPath = join(OUT_DIR, 'manifest.json');
		const oldNameBySlug = new Map();
		if (await exists(manifestPath)) {
			try {
				const old = JSON.parse(await readFile(manifestPath, 'utf8'));
				for (const e of old.entries ?? []) oldNameBySlug.set(e.slug, e.name);
			} catch { /* corrupt/missing manifest — fall through to slug-as-name below */ }
		}

		const existingFiles = (await readdir(OUT_DIR))
			.filter(f => f.endsWith('.json') && f !== 'manifest.json');
		const newSlugs = new Set(entries.map(e => e.slug));
		let preserved = 0, renamed = 0;
		for (const f of existingFiles) {
			const slug = f.slice(0, -5);
			if (newSlugs.has(slug)) continue;
			const name = oldNameBySlug.get(slug) ?? slug;
			if (!oldNameBySlug.has(slug)) renamed++;
			entries.push({ slug, name });
			preserved++;
		}
		if (preserved > 0) {
			console.log(`⚠️  MEGAPACK_DIR absent — ${preserved} presets existants préservés (${renamed} sans nom connu, slug utilisé)`);
			console.log(`   Pour des noms complets, relancer avec : MEGAPACK_DIR=/chemin/vers/presets-milk node scripts/build-presets.mjs`);
		}
	}

	const manifest = { version: 1, count: entries.length, entries };
	await writeFile(join(OUT_DIR, 'manifest.json'), JSON.stringify(manifest));
	console.log(`✅ Done: ${entries.length} presets → ${OUT_DIR}`);
}

main().catch(e => { console.error('❌', e); process.exit(1); });
