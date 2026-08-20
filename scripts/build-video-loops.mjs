#!/usr/bin/env node
/**
 * Télécharge et transcode des loops vidéo VJ depuis Pexels/Pixabay.
 *
 * Produit :
 *   cdn-video-loops/              ← tous les clips + manifest.json (pour rsync CDN)
 *   static/video-loops/           ← clips bundle:true + manifest partiel (committé)
 *
 * Prérequis : ffmpeg dans le PATH, PEXELS_API_KEY et/ou PIXABAY_API_KEY dans l'env.
 *
 * Usage :
 *   node scripts/build-video-loops.mjs
 *   node scripts/build-video-loops.mjs --dry-run   (structure seule, sans API ni ffmpeg)
 */

import { readFile, mkdir, writeFile, copyFile, access, readdir } from 'node:fs/promises'
import { createWriteStream } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { pipeline } from 'node:stream/promises'
import { spawn } from 'node:child_process'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = join(__dirname, '..')
const CDN_DIR = join(ROOT, 'cdn-video-loops')
const BUNDLE_DIR = join(ROOT, 'static', 'video-loops')
const SOURCES_FILE = join(__dirname, 'video-sources.json')
const DRY_RUN = process.argv.includes('--dry-run')

// ── Helpers ────────────────────────────────────────────────────────────────

function slugify(term, index) {
  const base = term
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 60)
  return `${base}-${String(index + 1).padStart(2, '0')}.webm`
}

async function exists(p) {
  try {
    await access(p)
    return true
  } catch {
    return false
  }
}

function ffmpegTranscode(input, output) {
  return new Promise((resolve, reject) => {
    const args = [
      '-y',
      '-i',
      input,
      '-an',
      '-vf',
      'scale=-2:720,fps=30',
      '-t',
      '8',
      '-c:v',
      'libvpx-vp9',
      '-crf',
      '33',
      '-b:v',
      '0',
      '-row-mt',
      '1',
      output,
    ]
    const proc = spawn('ffmpeg', args, { stdio: ['ignore', 'ignore', 'pipe'] })
    let stderr = ''
    proc.stderr.on('data', (d) => {
      stderr += d.toString()
    })
    proc.on('close', (code) => {
      if (code === 0) resolve()
      else reject(new Error(`ffmpeg exited ${code}: ${stderr.slice(-200)}`))
    })
    proc.on('error', reject)
  })
}

async function downloadFile(url, dest) {
  const res = await fetch(url)
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`)
  await pipeline(res.body, createWriteStream(dest))
}

// ── Providers ──────────────────────────────────────────────────────────────

async function fetchPexels(term, count, apiKey) {
  const url = `https://api.pexels.com/videos/search?query=${encodeURIComponent(term)}&per_page=${count}&orientation=landscape`
  const res = await fetch(url, { headers: { Authorization: apiKey } })
  if (!res.ok) throw new Error(`Pexels API ${res.status}`)
  const data = await res.json()
  return (data.videos ?? [])
    .slice(0, count)
    .map((v) => {
      const files = (v.video_files ?? []).filter((f) => f.quality === 'hd' || f.quality === 'sd')
      return files[0]?.link ?? v.video_files?.[0]?.link
    })
    .filter(Boolean)
}

async function fetchPixabay(term, count, apiKey) {
  const url = `https://pixabay.com/api/videos/?key=${apiKey}&q=${encodeURIComponent(term)}&per_page=${count}`
  const res = await fetch(url)
  if (!res.ok) throw new Error(`Pixabay API ${res.status}`)
  const data = await res.json()
  return (data.hits ?? [])
    .slice(0, count)
    .map((h) => {
      const v = h.videos
      return v.medium?.url ?? v.small?.url ?? v.large?.url
    })
    .filter(Boolean)
}

// ── Main ───────────────────────────────────────────────────────────────────

async function main() {
  console.log(DRY_RUN ? '🔍 build-video-loops: DRY RUN' : '🎬 build-video-loops: démarrage...')

  const sources = JSON.parse(await readFile(SOURCES_FILE, 'utf8'))
  const PEXELS_KEY = process.env[sources.providers.pexels]
  const PIXABAY_KEY = process.env[sources.providers.pixabay]

  await mkdir(CDN_DIR, { recursive: true })
  await mkdir(BUNDLE_DIR, { recursive: true })
  const tmpDir = join(ROOT, '.video-tmp')
  if (!DRY_RUN) await mkdir(tmpDir, { recursive: true })

  const cdnEntries = []
  const bundleEntries = []

  for (const query of sources.queries) {
    const { provider, term, count, bundle } = query
    console.log(`\n📥 ${provider} — "${term}" (${count} clips${bundle ? ', bundled' : ''})`)

    let videoUrls = []
    if (!DRY_RUN) {
      try {
        if (provider === 'pexels') {
          if (!PEXELS_KEY) {
            console.warn('  ⚠️  PEXELS_API_KEY non défini — skipped')
            continue
          }
          videoUrls = await fetchPexels(term, count, PEXELS_KEY)
        } else if (provider === 'pixabay') {
          if (!PIXABAY_KEY) {
            console.warn('  ⚠️  PIXABAY_API_KEY non défini — skipped')
            continue
          }
          videoUrls = await fetchPixabay(term, count, PIXABAY_KEY)
        }
      } catch (err) {
        console.error(`  ❌ Erreur API: ${err.message}`)
        continue
      }
    } else {
      videoUrls = Array.from({ length: count }, (_, i) => `dry-run-${i}`)
    }

    for (let i = 0; i < videoUrls.length; i++) {
      const slug = slugify(term, i)
      const cdnOut = join(CDN_DIR, slug)
      const name = `${term
        .split(' ')
        .map((w) => w[0].toUpperCase() + w.slice(1))
        .join(' ')} ${String(i + 1).padStart(2, '0')}`

      if (!DRY_RUN) {
        if (await exists(cdnOut)) {
          console.log(`  ✓ ${slug} (déjà présent, skipped)`)
        } else {
          const tmpFile = join(tmpDir, `raw-${Date.now()}-${i}`)
          try {
            process.stdout.write(`  ⬇  ${slug}...`)
            await downloadFile(videoUrls[i], tmpFile)
            await ffmpegTranscode(tmpFile, cdnOut)
            console.log(' ✓')
          } catch (err) {
            console.error(` ❌ ${err.message}`)
            continue
          }
        }
      } else {
        console.log(`  [dry] ${slug}`)
      }

      cdnEntries.push({ slug, name })
      if (bundle) bundleEntries.push({ slug, name })
    }
  }

  // Clips manuels (dossier video-sources/manual/)
  const manualDir = join(ROOT, sources.manual ?? 'video-sources/manual')
  if (await exists(manualDir)) {
    const manualFiles = (await readdir(manualDir)).filter((f) => /\.(webm|mp4|mov)$/i.test(f))
    console.log(`\n📁 Manual: ${manualFiles.length} fichiers`)
    for (const f of manualFiles) {
      const slug = f.endsWith('.webm') ? f : f.replace(/\.[^.]+$/, '.webm')
      const cdnOut = join(CDN_DIR, slug)
      const name = slug.replace(/\.webm$/, '').replace(/-/g, ' ')
      if (!DRY_RUN) {
        if (!(await exists(cdnOut))) {
          if (f.endsWith('.webm')) {
            await copyFile(join(manualDir, f), cdnOut)
          } else {
            await ffmpegTranscode(join(manualDir, f), cdnOut)
          }
        }
      }
      cdnEntries.push({ slug, name })
    }
  }

  // Écrire les manifests
  const cdnManifest = { version: 1, count: cdnEntries.length, entries: cdnEntries }
  const bundleManifest = { version: 1, count: bundleEntries.length, entries: bundleEntries }

  if (!DRY_RUN) {
    await writeFile(join(CDN_DIR, 'manifest.json'), JSON.stringify(cdnManifest, null, 2))
    // Copier les clips bundlés dans static/video-loops/
    for (const { slug } of bundleEntries) {
      const src = join(CDN_DIR, slug)
      if (await exists(src)) await copyFile(src, join(BUNDLE_DIR, slug))
    }
    await writeFile(join(BUNDLE_DIR, 'manifest.json'), JSON.stringify(bundleManifest, null, 2))
  } else {
    console.log(`\n[dry] CDN manifest: ${cdnEntries.length} entries`)
    console.log(`[dry] Bundle manifest: ${bundleEntries.length} entries`)
    console.log('\n✅ Dry run terminé — aucun fichier écrit')
  }

  if (!DRY_RUN) {
    console.log(`\n✅ Done:`)
    console.log(`   CDN  : ${cdnEntries.length} clips → ${CDN_DIR}`)
    console.log(`   Bundle: ${bundleEntries.length} clips → ${BUNDLE_DIR}`)
    console.log('\n📋 Prochaine étape :')
    console.log('   rsync -av --delete cdn-video-loops/ root@<ip-lxc>:/srv/video-loops/')
    console.log('   Puis renseigner PUBLIC_VIDEO_CDN dans .env et rebuilder.')
  }
}

main().catch((e) => {
  console.error('❌', e)
  process.exit(1)
})
