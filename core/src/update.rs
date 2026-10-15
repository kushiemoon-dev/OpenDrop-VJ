//! Deciding whether a newer release exists, as plain functions: how two
//! version strings compare, whether a check is due, and whether a link
//! reported by the network is safe to show.
//!
//! The network fetch lives in `opendrop_io::update_check`; nothing here does
//! I/O. The notice it produces is only ever informational: the app never
//! blocks on, downloads or installs an update.

/// Where release pages live. A link from outside it is replaced by the
/// releases page rather than shown.
pub const RELEASES_URL: &str = "https://github.com/kushiemoon-dev/OpenDrop-VJ/releases";
const TRUSTED_URL_PREFIX: &str = "https://github.com/kushiemoon-dev/OpenDrop-VJ/";

/// How often the app asks again, in seconds.
pub const CHECK_INTERVAL_SECONDS: u64 = 12 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseNotice {
    /// The newer version, without a leading `v`.
    pub version: String,
    pub url: String,
}

/// `major.minor.patch`, with an optional leading `v`. Anything else, a
/// pre-release suffix included, is not a release version.
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((major, minor, patch))
}

/// A notice when `latest_tag` is a release newer than `current`. `None` when
/// it is the same, older, or either is not a version, so a build with an odd
/// version string never claims an update.
pub fn notice_if_newer(current: &str, latest_tag: &str, reported_url: &str) -> Option<ReleaseNotice> {
    let (current, latest) = (parse_version(current)?, parse_version(latest_tag)?);
    if latest <= current {
        return None;
    }
    let url = if reported_url.starts_with(TRUSTED_URL_PREFIX) { reported_url } else { RELEASES_URL };
    Some(ReleaseNotice { version: format!("{}.{}.{}", latest.0, latest.1, latest.2), url: url.to_string() })
}

/// Whether enough time has passed since `last_check` (Unix seconds, 0 when
/// never) to ask again. A clock that went backwards counts as due.
pub fn check_is_due(now: u64, last_check: u64) -> bool {
    last_check == 0 || now < last_check || now - last_check >= CHECK_INTERVAL_SECONDS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_with_or_without_a_v_and_reject_everything_else() {
        assert_eq!(parse_version("0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("v1.10.3"), Some((1, 10, 3)));
        assert_eq!(parse_version(" v0.2.0 "), Some((0, 2, 0)));
        for bad in ["", "dev", "1.2", "1.2.3.4", "1.2.x", "0.2.0-rc1", "v", "-1.0.0"] {
            assert_eq!(parse_version(bad), None, "{bad:?} should not parse");
        }
    }

    #[test]
    fn only_a_strictly_newer_release_makes_a_notice() {
        let url = "https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/tag/v0.3.0";
        let notice = notice_if_newer("0.2.0", "v0.3.0", url).unwrap();
        assert_eq!(notice, ReleaseNotice { version: "0.3.0".to_string(), url: url.to_string() });

        assert!(notice_if_newer("0.2.0", "v0.2.0", url).is_none());
        assert!(notice_if_newer("0.2.0", "v0.1.9", url).is_none());
        assert!(notice_if_newer("0.9.0", "v0.10.0", url).is_some(), "compared numerically, not as text");
        assert!(notice_if_newer("0.2.0", "v1.0.0", url).is_some());
    }

    #[test]
    fn an_unparseable_version_never_claims_an_update() {
        let url = RELEASES_URL;
        assert!(notice_if_newer("dev", "v9.9.9", url).is_none());
        assert!(notice_if_newer("0.2.0", "nightly", url).is_none());
        assert!(notice_if_newer("0.2.0", "v0.3.0-beta", url).is_none());
    }

    #[test]
    fn a_link_from_outside_the_project_is_replaced_by_the_releases_page() {
        let notice = notice_if_newer("0.2.0", "v0.3.0", "https://evil.example/download").unwrap();
        assert_eq!(notice.url, RELEASES_URL);
        let notice = notice_if_newer("0.2.0", "v0.3.0", "https://github.com/someone-else/OpenDrop-VJ/x").unwrap();
        assert_eq!(notice.url, RELEASES_URL);
        let notice = notice_if_newer("0.2.0", "v0.3.0", "javascript:alert(1)").unwrap();
        assert_eq!(notice.url, RELEASES_URL);
    }

    #[test]
    fn a_check_is_due_when_never_done_stale_or_the_clock_went_backwards() {
        assert!(check_is_due(1_000_000, 0));
        assert!(!check_is_due(1_000_000, 1_000_000 - 60));
        assert!(check_is_due(1_000_000, 1_000_000 - CHECK_INTERVAL_SECONDS));
        assert!(check_is_due(1_000, 5_000));
    }
}
