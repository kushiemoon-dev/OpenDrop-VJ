//! Saves the running show to `session.json` next to `ui.json` and puts it
//! back on the next launch: mix, snapshots, timeline, overlays, playlists,
//! strobe, LFOs and the preset loaded on each deck. It is written when the
//! app exits and every [`AUTOSAVE_INTERVAL`], so a crash costs at most that
//! much work. Best effort throughout: a missing or unreadable file never
//! stops the app from starting.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use opendrop_core::session::Session;
use opendrop_io::session_codec::{decode_session, encode_session};

use crate::preset_load::request_preset_load;
use crate::AppState;

const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

/// Set by "Forget saved session" so the exit save does not write it straight back.
static FORGOTTEN: AtomicBool = AtomicBool::new(false);

pub(crate) fn session_file_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "opendrop-native").map(|dirs| dirs.config_dir().join("session.json"))
}

/// Writes through a temporary file and a rename, so a crash mid-write
/// leaves the previous session intact instead of a truncated file.
pub(crate) fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

pub(crate) fn save_session(path: &Path, session: &Session) {
    match encode_session(session) {
        Ok(json) => {
            if let Err(e) = write_atomically(path, &json) {
                eprintln!("[session] failed to write {}: {e}", path.display());
            }
        }
        Err(e) => eprintln!("[session] {e}"),
    }
}

/// `None` when there is no saved session. A file that cannot be read back
/// (hand-edited, or written by a newer build) is moved aside to
/// `session.json.bad` so the next save does not overwrite it unseen.
pub(crate) fn load_session(path: &Path) -> Option<Session> {
    let json = std::fs::read_to_string(path).ok()?;
    match decode_session(&json) {
        Ok(session) => Some(session),
        Err(e) => {
            let aside = path.with_extension("json.bad");
            eprintln!("[session] ignoring {}: {e}; kept as {}", path.display(), aside.display());
            let _ = std::fs::rename(path, aside);
            None
        }
    }
}

fn capture(state: &AppState) -> Session {
    state.show.capture_session(state.deck_preset_names.clone(), state.transition_seconds)
}

/// Applies the saved session and queues each deck's preset through the
/// normal validated load path. A preset that no longer exists is skipped and
/// the deck keeps the one bootstrap gave it.
pub(crate) fn restore(state: &mut AppState) {
    let Some(session) = session_file_path().and_then(|p| load_session(&p)) else { return };
    state.show.apply_session(&session);
    state.transition_seconds = session.shared.transition_time.max(0.0);
    for (slot, name) in session.deck_presets.iter().enumerate() {
        if !name.is_empty() && state.path_by_name.contains_key(name) {
            request_preset_load(state, slot, name.clone());
        }
    }
}

pub(crate) fn save_now(state: &AppState) {
    if FORGOTTEN.load(Ordering::Relaxed) {
        return;
    }
    if let Some(path) = session_file_path() {
        save_session(&path, &capture(state));
    }
}

/// Called once per rendered frame.
pub(crate) fn autosave_if_due(state: &mut AppState, now: Instant) {
    if now.duration_since(state.last_session_save) >= AUTOSAVE_INTERVAL {
        state.last_session_save = now;
        save_now(state);
    }
}

/// Deletes the saved session and stops writing a new one for the rest of
/// this run, so the next launch starts clean.
pub(crate) fn forget_saved_session() {
    FORGOTTEN.store(true, Ordering::Relaxed);
    if let Some(path) = session_file_path() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrop_core::show::Show;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opendrop-session-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_saved_session_loads_back() {
        let dir = scratch_dir("roundtrip");
        let path = dir.join("session.json");
        let mut show = Show::default();
        show.crossfader = 0.4;

        save_session(&path, &show.capture_session(Default::default(), 1.0));
        let loaded = load_session(&path).expect("session should load");

        assert_eq!(loaded.shared.crossfader, 0.4);
        assert!(!path.with_extension("json.tmp").exists(), "temporary file must be renamed away");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_file_is_simply_no_session() {
        let dir = scratch_dir("missing");
        assert!(load_session(&dir.join("session.json")).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unreadable_file_is_kept_aside_not_overwritten() {
        let dir = scratch_dir("bad");
        let path = dir.join("session.json");
        std::fs::write(&path, "{ not a session").unwrap();

        assert!(load_session(&path).is_none());

        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(path.with_extension("json.bad")).unwrap(), "{ not a session");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
