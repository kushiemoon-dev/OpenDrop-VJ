//! A process-wide log of what went wrong, kept in memory for the Log tab and
//! appended to a file so a crash report can include it.
//!
//! Every crate records through [`log_error!`], which also prints to stderr as
//! before. Nothing here can fail loudly: a log that cannot be written is
//! skipped, never a reason to stop the app.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Entries kept in memory; the oldest are dropped first.
const MAX_ENTRIES: usize = 500;
/// The file is moved aside to `<name>.old` once it grows past this.
const MAX_FILE_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Seconds since the Unix epoch.
    pub at: u64,
    pub message: String,
}

#[derive(Default)]
struct Log {
    entries: VecDeque<Entry>,
    file: Option<PathBuf>,
}

fn log() -> &'static Mutex<Log> {
    static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(Log::default()))
}

/// Starts appending to `path` (its folder is created if needed). Call once
/// from the main process; helper processes leave it unset so they only keep
/// their own memory.
pub fn set_file(path: PathBuf) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut log) = log().lock() {
        log.file = Some(path);
    }
}

/// Adds one message, in memory and (when a file is set) on disk.
pub fn record(message: &str) {
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let Ok(mut log) = log().lock() else { return };
    if log.entries.len() == MAX_ENTRIES {
        log.entries.pop_front();
    }
    log.entries.push_back(Entry { at, message: message.to_string() });
    if let Some(path) = &log.file {
        append_line(path, &format_line(at, message));
    }
}

/// Oldest first.
pub fn entries() -> Vec<Entry> {
    log().lock().map(|log| log.entries.iter().cloned().collect()).unwrap_or_default()
}

pub fn clear() {
    if let Ok(mut log) = log().lock() {
        log.entries.clear();
    }
}

/// The log path, once [`set_file`] has been called.
pub fn file_path() -> Option<PathBuf> {
    log().lock().ok().and_then(|log| log.file.clone())
}

/// `HH:MM:SS` (UTC) of a Unix timestamp.
pub fn clock_time(at: u64) -> String {
    let day = at % 86_400;
    format!("{:02}:{:02}:{:02}", day / 3600, day % 3600 / 60, day % 60)
}

/// One line as written to the file and copied from the Log tab.
pub fn format_line(at: u64, message: &str) -> String {
    format!("{} {}", clock_time(at), message)
}

fn append_line(path: &PathBuf, line: &str) {
    if std::fs::metadata(path).map(|m| m.len() > MAX_FILE_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

/// `eprintln!` that also lands in the log.
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {{
        let message = format!($($arg)*);
        eprintln!("{message}");
        $crate::error_log::record(&message);
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_wraps_at_midnight() {
        assert_eq!(clock_time(0), "00:00:00");
        assert_eq!(clock_time(86_400 + 3_661), "01:01:01");
    }

    #[test]
    fn a_line_has_the_time_then_the_message() {
        assert_eq!(format_line(45_296, "[ndi] failed"), "12:34:56 [ndi] failed");
    }

    // One test: the log is global, so separate tests would race on it.
    #[test]
    fn recording_keeps_order_caps_the_length_and_writes_the_file() {
        clear();
        record("first");
        record("second");
        let kept: Vec<String> = entries().into_iter().map(|e| e.message).collect();
        assert_eq!(kept, ["first", "second"]);

        for i in 0..MAX_ENTRIES + 10 {
            record(&format!("n{i}"));
        }
        let all = entries();
        assert_eq!(all.len(), MAX_ENTRIES);
        assert_eq!(all.last().unwrap().message, format!("n{}", MAX_ENTRIES + 9));
        assert_eq!(all[0].message, "n10");

        let dir = std::env::temp_dir().join(format!("opendrop-error-log-{}", std::process::id()));
        let path = dir.join("opendrop.log");
        set_file(path.clone());
        crate::log_error!("[test] {}", 42);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.trim_end().ends_with("[test] 42"));
        assert_eq!(file_path(), Some(path));
        let _ = std::fs::remove_dir_all(dir);
        clear();
    }
}
