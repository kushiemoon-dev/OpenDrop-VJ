//! Disk + GPU-texture cache for preset thumbnails, driven by `core::
//! thumb_queue`'s pure job queue. Port of `thumbnailer.svelte.ts`'s
//! `onVisible`/`onHidden`-driven pipeline (Step 15 of the plan); the actual
//! call site: pumping this once per tick from `about_to_wait`, gated on
//! the preset-browser panel's visibility, plus the `enqueue_front` calls
//! that feed jobs in as presets scroll into view: is Step 17. Nothing here
//! is called yet.
//!
//! Disk cache entries are raw RGBA8 (`ThumbnailRenderer::render_thumbnail`'s
//! exact output), no header: the size is always known ahead of time
//! (`THUMB_W * THUMB_H * 4`), so a length check on read is enough to detect
//! a stale/corrupt entry.

#![allow(dead_code)] // wired up to `about_to_wait` by Task 17

use opendrop_core::thumb_queue::{dequeue_job, ThumbJob};
use opendrop_engine::thumbnail::{ThumbnailRenderer, THUMB_H, THUMB_W};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const RGBA_BYTES: usize = (THUMB_W * THUMB_H * 4) as usize;

/// Cache filename for `preset_name` under `cache_dir`: a hash of the name
/// (sidesteps any filesystem-illegal-character handling for arbitrary
/// preset names) plus a fixed `.rgba` extension.
fn cache_path(cache_dir: &Path, preset_name: &str) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&preset_name, &mut hasher);
    cache_dir.join(format!("{:016x}.rgba", std::hash::Hasher::finish(&hasher)))
}

/// Reads `preset_name`'s disk cache entry, if present and exactly
/// `RGBA_BYTES` long. Anything else (missing, wrong size, read error) is
/// treated as a cache miss: the caller re-renders.
fn read_cached(cache_dir: &Path, preset_name: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(cache_path(cache_dir, preset_name)).ok()?;
    (bytes.len() == RGBA_BYTES).then_some(bytes)
}

/// Cache hit: read straight from disk. Miss: render via `renderer` (Step
/// 11) and write the result through to disk for next time. A write failure
/// is logged, not propagated: the freshly rendered pixels are still good,
/// only the caching optimization was lost.
fn load_or_render(cache_dir: &Path, renderer: &mut ThumbnailRenderer, preset_path: &Path, preset_name: &str) -> Result<Vec<u8>, String> {
    if let Some(cached) = read_cached(cache_dir, preset_name) {
        return Ok(cached);
    }
    let pixels = renderer.render_thumbnail(preset_path)?;
    let dest = cache_path(cache_dir, preset_name);
    let _ = std::fs::create_dir_all(cache_dir);
    if let Err(e) = std::fs::write(&dest, &pixels) {
        eprintln!("[thumbnails] failed to write cache {}: {e}", dest.display());
    }
    Ok(pixels)
}

/// Converts raw RGBA8 `pixels` (must be `RGBA_BYTES` long) into a loaded
/// egui texture. `name` is only the debug label egui attaches to the
/// texture, not a cache key.
fn rgba_to_texture(ctx: &egui::Context, name: &str, pixels: &[u8]) -> egui::TextureHandle {
    let image = egui::ColorImage::from_rgba_unmultiplied([THUMB_W as usize, THUMB_H as usize], pixels);
    ctx.load_texture(name, image, egui::TextureOptions::default())
}

/// Pumps at most one job off `queue`: called once per tick so a burst of
/// newly-visible preset tiles never stalls the live render loop. On a hit,
/// loads/renders the thumbnail (see `load_or_render`) and stores the
/// resulting `TextureHandle` in `textures`, keyed by `job.slot_key`: the
/// handle keeps the GPU texture alive until it's dropped (egui never frees
/// it earlier), so replacing or removing map entries is how a caller
/// reclaims that memory. Ok(()) with no queue activity, or once a job
/// without a resolvable path is skipped, are not errors.
pub fn pump_thumbnail_queue(
    queue: &mut Vec<ThumbJob>,
    cache_dir: &Path,
    renderer: &mut ThumbnailRenderer,
    path_by_name: &HashMap<String, PathBuf>,
    ctx: &egui::Context,
    textures: &mut HashMap<String, egui::TextureHandle>,
) -> Result<(), String> {
    let (job, rest) = dequeue_job(std::mem::take(queue));
    *queue = rest;
    let Some(job) = job else { return Ok(()) };
    let Some(preset_path) = path_by_name.get(&job.name) else {
        return Ok(()); // stale job (e.g. catalog changed under it): nothing to render
    };
    let pixels = load_or_render(cache_dir, renderer, preset_path, &job.name)?;
    textures.insert(job.slot_key.clone(), rgba_to_texture(ctx, &job.name, &pixels));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    mod cache_path_tests {
        use super::*;

        #[test]
        fn is_deterministic_for_the_same_name() {
            let dir = Path::new("/tmp/opendrop-thumbs");
            assert_eq!(cache_path(dir, "Psychedelic - Swirl"), cache_path(dir, "Psychedelic - Swirl"));
        }

        #[test]
        fn differs_for_different_names() {
            let dir = Path::new("/tmp/opendrop-thumbs");
            assert_ne!(cache_path(dir, "Preset A"), cache_path(dir, "Preset B"));
        }

        #[test]
        fn stays_under_cache_dir_with_an_rgba_extension() {
            let dir = Path::new("/tmp/opendrop-thumbs");
            let path = cache_path(dir, "Wave - Shimmer");
            assert_eq!(path.parent(), Some(dir));
            assert_eq!(path.extension(), Some(std::ffi::OsStr::new("rgba")));
        }
    }
}
