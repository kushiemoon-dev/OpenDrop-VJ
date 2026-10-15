//! The library scan: renders every preset in a child process (see
//! `scan_child`), keeps the measured brightness and motion, and remembers
//! them in a file so a scan can be interrupted and resumed. Nothing is ever
//! deleted or blocked: the results only feed an optional filter and the
//! energy auto-pilot.
//!
//! Several children run at once; each is killed if it takes too long, which
//! is how a preset that hangs projectM is recorded as failed instead of
//! stalling the scan.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opendrop_core::preset_analysis::{PresetAnalysis, ScanTable};

/// A child that has not finished by then is killed and the preset recorded
/// as failed.
const CHILD_TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(20);
/// Results are written to disk every this many presets.
const SAVE_EVERY: usize = 50;

pub(crate) fn scan_file_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "opendrop-native").map(|dirs| dirs.cache_dir().join("preset-scan.json"))
}

/// Where the children write their result before the app reads it: a directory
/// inside the user's own cache, not `/tmp`, whose predictable names another
/// local user could pre-create as symlinks.
pub(crate) fn scan_scratch_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "opendrop-native").map(|dirs| dirs.cache_dir().join("scan-tmp"))
}

/// `{ "<preset>": [mean_luma, motion] | null }`, `null` meaning the render
/// failed.
pub(crate) fn table_to_json(table: &ScanTable) -> String {
    let map: HashMap<&String, Option<[f32; 2]>> =
        table.entries().iter().map(|(name, result)| (name, result.map(|a| [a.mean_luma, a.motion]))).collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

/// A missing, malformed or partly bad file degrades to what could be read.
pub(crate) fn table_from_json(json: &str) -> ScanTable {
    let Ok(map) = serde_json::from_str::<HashMap<String, Option<[f32; 2]>>>(json) else { return ScanTable::default() };
    ScanTable::from_entries(
        map.into_iter()
            .map(|(name, result)| {
                let analysis = result
                    .filter(|[mean, motion]| mean.is_finite() && motion.is_finite())
                    .map(|[mean_luma, motion]| PresetAnalysis { mean_luma, motion });
                (name, analysis)
            })
            .collect(),
    )
}

pub(crate) fn load_table(path: &Path) -> ScanTable {
    std::fs::read_to_string(path).map(|json| table_from_json(&json)).unwrap_or_default()
}

/// Writes already serialized results. Serializing happens under the lock but
/// the disk write does not, so the render thread, which reads the same table
/// every frame, is never held up by a slow disk.
fn save_json(path: &Path, json: &str) {
    if let Err(e) = crate::session::write_atomically(path, json) {
        opendrop_core::log_error!("[scan] failed to save {}: {e}", path.display());
    }
}

/// How one analysis ended.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Outcome {
    /// A measurement.
    Measured(PresetAnalysis),
    /// The renderer rejected the preset or crashed on it: it will do so again,
    /// so it is remembered as failed.
    Rejected,
    /// The child could not be started or took too long, which can just mean
    /// the machine was busy. Nothing is remembered, so a later scan tries again.
    Inconclusive,
}

/// Runs one analysis child to completion and reads its result.
fn run_analysis(mut command: Command, out_path: &Path, timeout: Duration, cancel: &AtomicBool) -> Outcome {
    let Ok(mut child) = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
        return Outcome::Inconclusive;
    };
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            // Cancelling does not wait for the preset being measured.
            Ok(None) if started.elapsed() >= timeout || cancel.load(Ordering::Relaxed) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(out_path);
                return Outcome::Inconclusive;
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(_) => return Outcome::Inconclusive,
        }
    };
    let analysis = if status.success() { std::fs::read_to_string(out_path).ok().and_then(|t| PresetAnalysis::from_line(&t)) } else { None };
    let _ = std::fs::remove_file(out_path);
    match analysis {
        Some(analysis) => Outcome::Measured(analysis),
        None => Outcome::Rejected,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ScanProgress {
    pub running: bool,
    pub done: usize,
    pub total: usize,
}

struct Shared {
    table: Mutex<ScanTable>,
    /// Bumped after every recorded result, so cached views can tell they are stale.
    revision: AtomicU64,
    done: AtomicUsize,
    total: AtomicUsize,
    running: AtomicBool,
    cancel: AtomicBool,
}

#[derive(Clone)]
pub(crate) struct ScanHandle {
    shared: Arc<Shared>,
}

impl ScanHandle {
    pub(crate) fn new(initial: ScanTable) -> Self {
        Self {
            shared: Arc::new(Shared {
                table: Mutex::new(initial),
                revision: AtomicU64::new(0),
                done: AtomicUsize::new(0),
                total: AtomicUsize::new(0),
                running: AtomicBool::new(false),
                cancel: AtomicBool::new(false),
            }),
        }
    }

    pub(crate) fn progress(&self) -> ScanProgress {
        ScanProgress {
            running: self.shared.running.load(Ordering::Relaxed),
            done: self.shared.done.load(Ordering::Relaxed),
            total: self.shared.total.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.shared.revision.load(Ordering::Relaxed)
    }

    /// Reads the results without copying them.
    pub(crate) fn with_table<R>(&self, f: impl FnOnce(&ScanTable) -> R) -> R {
        f(&self.shared.table.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Writes the results now, for the moment the app quits mid-scan.
    pub(crate) fn save(&self, path: &Path) {
        let json = table_to_json(&self.shared.table.lock().unwrap_or_else(|e| e.into_inner()));
        save_json(path, &json);
    }

    pub(crate) fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }

    /// Starts scanning `jobs` (name and file) that have no result yet, with
    /// `workers` children at a time. Does nothing while a scan is running.
    pub(crate) fn start(
        &self,
        jobs: Vec<(String, PathBuf)>,
        workers: usize,
        program: PathBuf,
        scratch_dir: PathBuf,
        save_to: Option<PathBuf>,
    ) {
        if let Err(e) = std::fs::create_dir_all(&scratch_dir) {
            opendrop_core::log_error!("[scan] cannot create {}: {e}", scratch_dir.display());
            return;
        }
        if self.shared.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let todo: Vec<(String, PathBuf)> = self.with_table(|t| jobs.into_iter().filter(|(name, _)| !t.contains(name)).collect());
        self.shared.cancel.store(false, Ordering::Relaxed);
        self.shared.done.store(0, Ordering::Relaxed);
        self.shared.total.store(todo.len(), Ordering::Relaxed);
        let shared = self.shared.clone();
        let todo = Arc::new(todo);
        let next = Arc::new(AtomicUsize::new(0));
        std::thread::spawn(move || {
            let workers: Vec<_> = (0..workers.max(1))
                .map(|worker| {
                    let (shared, todo, next, program, scratch_dir, save_to) =
                        (shared.clone(), todo.clone(), next.clone(), program.clone(), scratch_dir.clone(), save_to.clone());
                    std::thread::spawn(move || {
                        scan_worker(worker, &shared, &todo, &next, &program, &scratch_dir, save_to.as_deref())
                    })
                })
                .collect();
            for worker in workers {
                let _ = worker.join();
            }
            if let Some(path) = save_to.as_deref() {
                let json = table_to_json(&shared.table.lock().unwrap_or_else(|e| e.into_inner()));
                save_json(path, &json);
            }
            shared.running.store(false, Ordering::SeqCst);
        });
    }
}

fn scan_worker(
    worker: usize,
    shared: &Shared,
    todo: &[(String, PathBuf)],
    next: &AtomicUsize,
    program: &Path,
    scratch_dir: &Path,
    save_to: Option<&Path>,
) {
    let scratch = scratch_dir.join(format!("{}-{worker}.txt", std::process::id()));
    while !shared.cancel.load(Ordering::Relaxed) {
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some((name, path)) = todo.get(index) else { break };
        let mut command = Command::new(program);
        command.arg("--analyze-preset").arg(path).arg(&scratch);
        let outcome = run_analysis(command, &scratch, CHILD_TIMEOUT, &shared.cancel);
        let done = {
            let mut table = shared.table.lock().unwrap_or_else(|e| e.into_inner());
            match outcome {
                Outcome::Measured(analysis) => table.record(name, Some(analysis)),
                Outcome::Rejected => table.record(name, None),
                Outcome::Inconclusive => {}
            }
            shared.revision.fetch_add(1, Ordering::Relaxed);
            shared.done.fetch_add(1, Ordering::Relaxed) + 1
        };
        if let (true, Some(path)) = (done % SAVE_EVERY == 0, save_to) {
            let json = table_to_json(&shared.table.lock().unwrap_or_else(|e| e.into_inner()));
            save_json(path, &json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_survives_a_json_round_trip_including_failures() {
        let mut table = ScanTable::default();
        table.record("good", Some(PresetAnalysis { mean_luma: 90.0, motion: 7.5 }));
        table.record("rejected", None);

        let restored = table_from_json(&table_to_json(&table));

        assert_eq!(restored, table);
    }

    #[test]
    fn garbage_and_non_finite_entries_degrade_gracefully() {
        assert!(table_from_json("not json").is_empty());
        assert!(table_from_json("[1,2,3]").is_empty());
        let restored = table_from_json(r#"{"ok": [10.0, 2.0], "bad": [null, 1.0]}"#);
        assert!(restored.is_empty(), "a malformed entry rejects the whole file rather than half loading it");
    }

    #[cfg(unix)]
    fn script(body: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(body).arg("sh");
        command
    }

    #[cfg(unix)]
    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("opendrop-scan-test-{tag}-{}", std::process::id()))
    }

    #[cfg(unix)]
    #[test]
    fn a_child_that_writes_a_result_is_read_back() {
        let out = scratch("ok");
        let mut command = script(&format!("echo '42.5 3.25' > {}", out.display()));
        command.env("X", "1");
        let outcome = run_analysis(command, &out, Duration::from_secs(5), &AtomicBool::new(false));
        assert_eq!(outcome, Outcome::Measured(PresetAnalysis { mean_luma: 42.5, motion: 3.25 }));
        assert!(!out.exists(), "the scratch file is cleaned up");
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_or_garbled_child_yields_no_result() {
        let out = scratch("fail");
        assert_eq!(run_analysis(script("exit 3"), &out, Duration::from_secs(5), &AtomicBool::new(false)), Outcome::Rejected);
        let garbled = script(&format!("echo 'hello' > {}", out.display()));
        assert_eq!(run_analysis(garbled, &out, Duration::from_secs(5), &AtomicBool::new(false)), Outcome::Rejected);
    }

    #[cfg(unix)]
    #[test]
    fn a_child_that_hangs_is_killed_and_not_held_against_the_preset() {
        let out = scratch("hang");
        let started = Instant::now();
        assert_eq!(run_analysis(script("sleep 30"), &out, Duration::from_millis(200), &AtomicBool::new(false)), Outcome::Inconclusive);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn cancelling_kills_the_running_child_at_once() {
        let out = scratch("cancel");
        let cancel = AtomicBool::new(true);
        let started = Instant::now();
        assert_eq!(run_analysis(script("sleep 30"), &out, Duration::from_secs(30), &cancel), Outcome::Inconclusive);
        assert!(started.elapsed() < Duration::from_secs(5), "did not wait for the child");
    }

    #[cfg(unix)]
    #[test]
    fn a_scan_records_every_job_skips_known_ones_and_reports_progress() {
        let handle = ScanHandle::new(ScanTable::default());
        handle.with_table(|t| assert!(t.is_empty()));
        // `true` exits 0 without writing a result: the renderer "rejected" them, so both are recorded as failed.
        let jobs = vec![
            ("a".to_string(), PathBuf::from("/nonexistent/a.milk")),
            ("b".to_string(), PathBuf::from("/nonexistent/b.milk")),
        ];
        let scratch_dir = scratch("dir");
        handle.start(jobs.clone(), 2, PathBuf::from("true"), scratch_dir.clone(), None);
        for _ in 0..200 {
            if !handle.progress().running {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let progress = handle.progress();
        assert!(!progress.running);
        assert_eq!((progress.done, progress.total), (2, 2));
        handle.with_table(|t| assert!(t.is_flagged("a") && t.is_flagged("b")));
        assert!(handle.revision() >= 2);

        handle.start(jobs, 2, PathBuf::from("true"), scratch_dir, None);
        for _ in 0..200 {
            if !handle.progress().running {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(handle.progress().total, 0, "presets with a result are not scanned again");
    }
}
