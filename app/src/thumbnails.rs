//! Disk + GPU-texture cache for preset thumbnails, driven by `core::
//! thumb_queue`'s pure job queue. Port of `thumbnailer.svelte.ts`'s
//! `onVisible`/`onHidden`-driven pipeline. `main.rs`'s
//! `about_to_wait` pumps `pump_thumbnail_queue` once per tick, gated on the
//! preset-browser panel's visibility; `ui::preset_browser` feeds jobs in via
//! `enqueue_front` as tiles scroll into view.
//!
//! Disk cache entries are raw RGBA8 (`THUMB_W * THUMB_H * 4` bytes), no
//! header: the size is always known ahead of time, so a length check on
//! read is enough to detect a stale, truncated, or corrupt entry.
//!
//! Nothing here renders anything. A cache miss spawns a `--render-thumbnail`
//! child process (`thumbnail_child`), which writes the cache file itself;
//! this module only ever reads that file. That keeps a preset that crashes
//! projectM from taking down the app on nothing more than a scroll, and
//! keeps the 31-frame render and its blocking `glReadPixels` off the
//! event-loop thread. At most one child is outstanding at a time: a fast
//! scroll through a ~9800-tile grid must not turn into dozens of concurrent
//! subprocesses, and the pump does at most one unit of work per tick, as it
//! always has.

use opendrop_core::thumb_queue::{dequeue_job, ThumbJob};
use opendrop_engine::thumbnail::{THUMB_H, THUMB_W};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const RGBA_BYTES: usize = (THUMB_W * THUMB_H * 4) as usize;

/// Max GPU textures kept resident in
/// `thumbnail_textures` at once. Nothing previously removed an entry once
/// created, so every tile ever scrolled past kept its texture alive for
/// the whole session, up to ~812MB VRAM for the full ~9795-preset catalog
/// (`9795 * THUMB_W * THUMB_H * 4` bytes). 500 textures is several
/// screens' worth of scroll in either direction (a typical control-window
/// grid shows on the order of 100 tiles at once) while capping worst-case
/// memory at `500 * THUMB_W * THUMB_H * 4` bytes, ~41MB, roughly 20x
/// smaller than the unbounded worst case.
const MAX_RESIDENT_THUMBNAILS: usize = 500;

/// Wall-clock budget for one `--render-thumbnail` child before it is killed
/// and its preset written off. Longer than `preflight`'s 5s: that child
/// loads a preset and renders 5 frames, this one renders 31 and reads them
/// back. Nothing waits on this deadline: it is checked from the pump's
/// per-tick poll, so a hung child costs the UI nothing but one stalled
/// thumbnail slot.
const RENDER_TIMEOUT: Duration = Duration::from_secs(10);

/// The one `--render-thumbnail` child currently outstanding, if any, and
/// the job it was spawned for. Owned by `AppState`; created and consumed
/// only by `pump_thumbnail_queue`.
pub struct InFlightThumb {
    child: Child,
    job: ThumbJob,
    started_at: Instant,
}

enum RenderPoll {
    Running,
    /// The child exited cleanly. Its output still has to be read and
    /// length-checked before it counts as a real thumbnail.
    Exited,
    /// The child exited or died on its own, and `try_wait` has already
    /// reaped it.
    Failed(String),
    /// The child overran `RENDER_TIMEOUT` and has been sent SIGKILL, but
    /// deliberately not waited on. The caller must hand it to `reap_killed`
    /// instead of dropping it.
    TimedOut,
}

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

/// Converts raw RGBA8 `pixels` (must be `RGBA_BYTES` long) into a loaded
/// egui texture. Rows arrive top-first: the `--render-thumbnail` child
/// reverses `glReadPixels`' bottom-first order before writing the cache
/// file, so what comes off disk is already in the order
/// `ColorImage::from_rgba_unmultiplied` expects. `name` is only the debug
/// label egui attaches to the texture, not a cache key.
fn rgba_to_texture(ctx: &egui::Context, name: &str, pixels: &[u8]) -> egui::TextureHandle {
    let image = egui::ColorImage::from_rgba_unmultiplied([THUMB_W as usize, THUMB_H as usize], pixels);
    ctx.load_texture(name, image, egui::TextureOptions::default())
}

/// Re-invokes this same binary as `--render-thumbnail <preset> <out>`.
/// Mirrors `preflight::spawn_preflight`'s child setup, minus the thread:
/// nothing here blocks, so there is nothing for a thread to get off the
/// event loop.
fn spawn_render(preset_path: &Path, out_path: &Path) -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe() failed: {e}"))?;
    Command::new(exe)
        .arg("--render-thumbnail")
        .arg(preset_path)
        .arg(out_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn failed: {e}"))
}

/// Non-blocking check on the outstanding child, including the timeout:
/// same `try_wait` shape as `spawn_preflight`'s loop, but polled once per
/// tick from the event loop instead of slept on in a thread.
fn poll_render(in_flight: &mut InFlightThumb) -> RenderPoll {
    match in_flight.child.try_wait() {
        Ok(Some(status)) if status.success() => RenderPoll::Exited,
        // Covers death-by-signal too (a projectM segfault or abort), which
        // is exactly the case this whole out-of-process detour exists for.
        Ok(Some(status)) => RenderPoll::Failed(format!("thumbnail renderer exited with {status}")),
        Ok(None) if in_flight.started_at.elapsed() > RENDER_TIMEOUT => {
            // SIGKILL, and then nothing: reaping is `reap_killed`'s job.
            // `spawn_preflight` follows kill() with a blocking wait() here,
            // which is correct there because it runs on its own thread. This
            // runs on the event-loop thread, where wait() would be a stall
            // of unbounded length, and a child that has already been
            // unresponsive for RENDER_TIMEOUT is the likeliest of all to be
            // stuck in an uninterruptible kernel wait (a wedged GPU ioctl, a
            // slow network-mounted cache directory) where even SIGKILL does
            // not land promptly. That stall is the exact failure this whole
            // out-of-process change exists to prevent.
            let _ = in_flight.child.kill();
            RenderPoll::TimedOut
        }
        Ok(None) => RenderPoll::Running,
        Err(e) => RenderPoll::Failed(format!("wait failed: {e}")),
    }
}

/// Reaps children killed by `poll_render`'s timeout branch, without ever
/// blocking. Each one is polled with `try_wait` on as many ticks as it
/// takes, and dropped from the list once it is gone (or once the wait
/// itself errors, which leaves nothing further to reap).
///
/// Called unconditionally from `about_to_wait`, deliberately not from
/// `pump_thumbnail_queue`: the pump is gated on the preset browser being
/// visible, and a killed child has to be reaped whether or not the user is
/// still looking at the panel that spawned it.
pub fn reap_killed(killed: &mut Vec<Child>) {
    killed.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
}

/// Inserts `(key, tex)` into `textures`, tracking insertion order in
/// `order` so the oldest entry can be evicted once
/// `MAX_RESIDENT_THUMBNAILS` is exceeded.
/// FIFO, not true LRU (no re-touch-on-view bump): the simplest structure
/// that bounds memory. A tile evicted while still on screen just gets
/// re-queued and re-rendered next time it's visible and missing (a cheap
/// disk-cache hit via `read_cached`, not a re-render), which only happens
/// after `MAX_RESIDENT_THUMBNAILS` other distinct tiles have been shown
/// since. `order` and `textures` are kept in lockstep: a key is pushed to
/// `order` only the first time it's inserted, since `pump_thumbnail_queue`
/// never overwrites an already-resident key (see its `textures.contains_key`
/// check); an overwrite would otherwise grow `order` past `textures`' size.
fn insert_bounded(textures: &mut HashMap<String, egui::TextureHandle>, order: &mut VecDeque<String>, key: String, tex: egui::TextureHandle) {
    if textures.insert(key.clone(), tex).is_none() {
        order.push_back(key);
    }
    while textures.len() > MAX_RESIDENT_THUMBNAILS {
        let Some(oldest) = order.pop_front() else { break };
        textures.remove(&oldest);
    }
}

/// Does at most one unit of work per tick, so a burst of newly-visible
/// preset tiles never stalls the live render loop: either poll the
/// outstanding render child, or take one job off `queue`. A job that hits
/// the disk cache is finished on the spot (a ~83KB read plus a texture
/// upload); a miss spawns a child and finishes on a later tick. The
/// resulting `TextureHandle` is stored in `textures`, keyed by
/// `job.slot_key`: the handle keeps the GPU texture alive until it's
/// dropped (egui never frees it earlier), so replacing or removing map
/// entries is how a caller reclaims that memory. Ok(()) with no queue
/// activity, or once a job without a resolvable path is skipped, are not
/// errors.
///
/// A preset whose render fails, whether that's a non-zero exit, death by
/// signal, timeout, or a missing/wrong-size output file, is recorded in `failed` and never
/// retried: the tile that asked for it is still on screen and still has no
/// texture, so it would re-enqueue the same job on the very next tick,
/// turning one failure into an endless respawn loop. `ui::preset_browser`
/// reads the same set to stop enqueueing at the source.
#[allow(clippy::too_many_arguments)]
pub fn pump_thumbnail_queue(
    queue: &mut Vec<ThumbJob>,
    in_flight: &mut Option<InFlightThumb>,
    killed: &mut Vec<Child>,
    cache_dir: &Path,
    path_by_name: &HashMap<String, PathBuf>,
    ctx: &egui::Context,
    textures: &mut HashMap<String, egui::TextureHandle>,
    texture_order: &mut VecDeque<String>,
    failed: &mut HashSet<String>,
) -> Result<(), String> {
    // A child spawned on an earlier tick takes priority, and every branch
    // below returns: never poll a child and start another job in the same
    // tick. This is also what lets `main.rs` keep pumping purely to drain
    // an outstanding child after the user has switched away from the
    // browser panel, without that draining the rest of the queue.
    if let Some(mut pending) = in_flight.take() {
        match poll_render(&mut pending) {
            RenderPoll::Running => {
                *in_flight = Some(pending);
                return Ok(());
            }
            RenderPoll::Failed(e) => {
                failed.insert(pending.job.name.clone());
                return Err(format!("{}: {e}", pending.job.name));
            }
            RenderPoll::TimedOut => {
                failed.insert(pending.job.name.clone());
                // Minor #12: best-effort cleanup of the temp file the
                // child may have been mid-write on when SIGKILLed:
                // `write_atomically` names it `<hash>.tmp<pid>` next to
                // the final `.rgba` path, so both are derivable here. May
                // not exist yet (killed before the write started) or may
                // already be gone (renamed into place moments before the
                // kill landed); either way a missing-file error is
                // ignored, this is cleanup, not a correctness path.
                let tmp = cache_path(cache_dir, &pending.job.name).with_extension(format!("tmp{}", pending.child.id()));
                let _ = std::fs::remove_file(tmp);
                // SIGKILLed but not waited on: see `poll_render`. Hand it
                // to the non-blocking reaper rather than dropping it, which
                // would leave a zombie for the rest of the session.
                killed.push(pending.child);
                return Err(format!("{}: thumbnail render timed out", pending.job.name));
            }
            RenderPoll::Exited => {
                // The child's clean exit is a claim, not proof. `read_cached`
                // is the verification: a missing or wrong-length file counts
                // as a failed render, same as a non-zero exit would.
                let Some(pixels) = read_cached(cache_dir, &pending.job.name) else {
                    failed.insert(pending.job.name.clone());
                    return Err(format!("{}: renderer exited cleanly but wrote no usable thumbnail", pending.job.name));
                };
                insert_bounded(textures, texture_order, pending.job.slot_key, rgba_to_texture(ctx, &pending.job.name, &pixels));
                return Ok(());
            }
        }
    }

    let (job, rest) = dequeue_job(std::mem::take(queue));
    *queue = rest;
    let Some(job) = job else { return Ok(()) };
    if failed.contains(&job.name) {
        // Already known bad. `ui::preset_browser` stops enqueueing a name
        // once it lands in `failed`, but a job for it can already be
        // sitting in the queue: the tile re-enqueues on every frame its
        // render child is still running, and the failure is only recorded
        // when that child exits. Without this the name gets exactly one
        // extra spawn; with it, none.
        return Ok(());
    }
    if textures.contains_key(&job.slot_key) {
        // The mirror image of the check above, for the success case: the
        // same stale re-enqueued job survives a child that *succeeded*, and
        // would otherwise cost a redundant 83KB read plus a second texture
        // upload that immediately replaces the one just built. Reading this
        // as "already done" rather than "slot busy" is only correct because
        // `ui::preset_browser` uses the preset name as its slot key, so a
        // filled slot always holds this exact preset; revisit if slot keys
        // ever become reusable across presets.
        return Ok(());
    }
    let Some(preset_path) = path_by_name.get(&job.name) else {
        return Ok(()); // stale job (e.g. catalog changed under it): nothing to render
    };

    if let Some(pixels) = read_cached(cache_dir, &job.name) {
        insert_bounded(textures, texture_order, job.slot_key, rgba_to_texture(ctx, &job.name, &pixels));
        return Ok(());
    }

    match spawn_render(preset_path, &cache_path(cache_dir, &job.name)) {
        Ok(child) => {
            *in_flight = Some(InFlightThumb { child, job, started_at: Instant::now() });
            Ok(())
        }
        Err(e) => {
            failed.insert(job.name.clone());
            Err(format!("{}: {e}", job.name))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod reap_killed_tests {
        use super::*;

        fn long_lived_child() -> Child {
            Command::new("sleep").arg("30").stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("failed to spawn `sleep`")
        }

        #[test]
        fn keeps_a_child_that_is_still_running_and_does_not_block_on_it() {
            let mut killed = vec![long_lived_child()];
            let started = Instant::now();
            reap_killed(&mut killed);
            // The polarity that matters: `Ok(None)` means "still running",
            // so the entry has to survive. Inverting this predicate would
            // drop a live child and leak it for good.
            assert_eq!(killed.len(), 1);
            // And the call has to have returned without waiting for a child
            // with 30 seconds left to live. This runs on the event-loop
            // thread; blocking here is the whole bug this reaper exists to
            // avoid. The bound is deliberately loose: it is checking for a
            // blocking wait, not measuring scheduler latency.
            assert!(started.elapsed() < Duration::from_secs(1), "reap_killed blocked on a live child");
            let _ = killed[0].kill();
            let _ = killed[0].wait(); // test cleanup, not the event-loop path
        }

        #[test]
        fn drops_a_child_once_it_has_exited() {
            let mut killed = vec![long_lived_child()];
            let _ = killed[0].kill();
            // Polled, never waited on, exactly as `about_to_wait` does it:
            // the entry clears within a few ticks rather than in one call.
            let deadline = Instant::now() + Duration::from_secs(5);
            while !killed.is_empty() && Instant::now() < deadline {
                reap_killed(&mut killed);
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(killed.is_empty(), "a killed child was never reaped");
        }

        #[test]
        fn on_an_empty_list_is_a_no_op() {
            let mut killed: Vec<Child> = Vec::new();
            reap_killed(&mut killed);
            assert!(killed.is_empty());
        }
    }

    /// The bounded thumbnail-texture cache.
    /// `egui::TextureHandle` needs no GL context to construct: its
    /// texture manager is CPU-side bookkeeping only, so this is a real
    /// unit test of the eviction logic, not just a description of it.
    mod insert_bounded_tests {
        use super::*;

        fn tex(ctx: &egui::Context, tag: &str) -> egui::TextureHandle {
            let image = egui::ColorImage::new([1, 1], vec![egui::Color32::WHITE]);
            ctx.load_texture(tag, image, egui::TextureOptions::default())
        }

        #[test]
        fn stays_under_the_cap_evicting_the_oldest_first() {
            let ctx = egui::Context::default();
            let mut textures = HashMap::new();
            let mut order = VecDeque::new();
            for i in 0..(MAX_RESIDENT_THUMBNAILS + 3) {
                insert_bounded(&mut textures, &mut order, i.to_string(), tex(&ctx, &i.to_string()));
            }
            // Genuinely bounded, not just "less bad": never exceeds the cap.
            assert_eq!(textures.len(), MAX_RESIDENT_THUMBNAILS);
            assert_eq!(order.len(), MAX_RESIDENT_THUMBNAILS);
            // The 3 oldest (inserted first) are gone...
            assert!(!textures.contains_key("0"));
            assert!(!textures.contains_key("1"));
            assert!(!textures.contains_key("2"));
            // ...and the most recent survives.
            let last = (MAX_RESIDENT_THUMBNAILS + 2).to_string();
            assert!(textures.contains_key(&last));
        }

        #[test]
        fn under_the_cap_keeps_everything() {
            let ctx = egui::Context::default();
            let mut textures = HashMap::new();
            let mut order = VecDeque::new();
            insert_bounded(&mut textures, &mut order, "a".to_string(), tex(&ctx, "a"));
            insert_bounded(&mut textures, &mut order, "b".to_string(), tex(&ctx, "b"));
            assert_eq!(textures.len(), 2);
            assert_eq!(order.len(), 2);
        }
    }

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
