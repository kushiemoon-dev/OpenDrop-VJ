//! The update notice: when to ask GitHub for the latest release, what to do
//! with the answer, and what to show. Informational only: nothing is ever
//! downloaded, installed or blocked from here.
//!
//! The newest release seen is remembered in `ui.json`, so a pending update
//! stays visible on launches that do not ask again (at most one request every
//! twelve hours), and it is shown only while the check is enabled.

use std::time::{SystemTime, UNIX_EPOCH};

use opendrop_core::update::{check_is_due, notice_if_newer, ReleaseNotice};
use opendrop_io::update_check::{spawn, LatestRelease, UpdateCheckHandle, UpdateCheckState};

use crate::AppState;

/// The version of this build.
pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Starts a check when they are enabled and one is due.
pub(crate) fn start_if_due(enabled: bool, now: u64, last_check: u64) -> Option<UpdateCheckHandle> {
    (enabled && check_is_due(now, last_check)).then(|| spawn(APP_VERSION))
}

/// The notice to show for the newest release remembered, if checks are on.
pub(crate) fn notice_for(enabled: bool, known: &Option<(String, String)>) -> Option<ReleaseNotice> {
    let (tag, url) = known.as_ref().filter(|_| enabled)?;
    notice_if_newer(APP_VERSION, tag, url)
}

/// Folds a finished check into what is remembered. A failed one changes
/// nothing, not even the time, so the next launch tries again.
pub(crate) fn settle(result: &Result<LatestRelease, String>, known: &mut Option<(String, String)>, last_check: &mut u64, now: u64) {
    match result {
        Ok(release) => {
            *known = Some((release.tag.clone(), release.url.clone()));
            *last_check = now;
        }
        Err(e) => opendrop_core::log_error!("[update] {e}"),
    }
}

/// Called every frame: collects a finished check and refreshes the notice.
pub(crate) fn poll(state: &mut AppState) {
    if let Some(handle) = &state.update_check {
        if let UpdateCheckState::Done(result) = &*handle.latest() {
            settle(result, &mut state.known_release, &mut state.last_update_check, now_unix());
            state.update_check = None;
        }
    }
    state.update_notice = notice_for(state.check_for_updates, &state.known_release);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn newer_tag() -> String {
        // A version above any this crate will reach in a test run.
        "v99.0.0".to_string()
    }

    #[test]
    fn a_remembered_newer_release_shows_only_while_checks_are_enabled() {
        let known = Some((newer_tag(), "https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/tag/v99.0.0".to_string()));
        let notice = notice_for(true, &known).expect("a notice");
        assert_eq!(notice.version, "99.0.0");
        assert!(notice_for(false, &known).is_none());
        assert!(notice_for(true, &None).is_none());
    }

    #[test]
    fn the_current_or_an_older_release_shows_nothing() {
        let current = Some((format!("v{APP_VERSION}"), String::new()));
        assert!(notice_for(true, &current).is_none());
        let older = Some(("v0.0.1".to_string(), String::new()));
        assert!(notice_for(true, &older).is_none());
    }

    #[test]
    fn a_successful_check_is_remembered_and_a_failed_one_changes_nothing() {
        let mut known = None;
        let mut last = 0;
        settle(&Err("offline".to_string()), &mut known, &mut last, 500);
        assert_eq!((known.clone(), last), (None, 0));

        let release = LatestRelease { tag: "v0.5.0".to_string(), url: "u".to_string() };
        settle(&Ok(release), &mut known, &mut last, 900);
        assert_eq!(known, Some(("v0.5.0".to_string(), "u".to_string())));
        assert_eq!(last, 900);

        settle(&Err("offline".to_string()), &mut known, &mut last, 1_000);
        assert_eq!(last, 900, "a failure keeps the earlier check time");
        assert!(known.is_some(), "and the earlier release");
    }

    #[test]
    fn nothing_starts_when_disabled_or_not_due() {
        assert!(start_if_due(false, 1_000_000, 0).is_none());
        assert!(start_if_due(true, 1_000_000, 999_990).is_none());
    }
}
