use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) fn preset_dir() -> Result<PathBuf, String> {
    let configured = crate::config::config_file_path()
        .map(|path| crate::config::load_config(&path))
        .and_then(|config| config.preset_dir)
        .filter(|dir| !dir.trim().is_empty())
        .map(PathBuf::from);
    preset_dir_from(
        std::env::var_os("OPENDROP_PRESET_DIR"),
        configured,
        std::env::var_os("APPDIR"),
        std::env::current_exe().ok(),
    )
}

/// The env-reading half of `preset_dir`, split out so the fallback order is
/// testable without mutating process-global environment state.
///
/// Resolution order: `OPENDROP_PRESET_DIR` wins unconditionally if set; then
/// the folder chosen in the app, if it exists; then a platform-specific packaged location, only if it actually exists on
/// disk; else the same error `cargo run` has always raised for a missing
/// dev override.
fn preset_dir_from(
    env_override: Option<OsString>,
    configured: Option<PathBuf>,
    appdir: Option<OsString>,
    exe_path: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(raw) = env_override {
        return Ok(PathBuf::from(raw));
    }

    // The folder chosen in the app. One that has gone missing (a drive that
    // is not mounted) falls through to the packaged presets instead of
    // stopping the app.
    if let Some(dir) = configured {
        if dir.is_dir() {
            return Ok(dir);
        }
        opendrop_core::log_error!("[app] the preset folder {} does not exist, using the default", dir.display());
    }

    // Linux: AppImage's own runtime sets APPDIR at launch (not something
    // this repo sets). Must match the layout the AppImage build creates in the AppDir.
    if let Some(appdir) = appdir {
        let candidate = PathBuf::from(appdir).join("usr/share/opendrop/presets");
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }

    // Windows: sibling-of-exe, flat layout (no `usr/bin`/`usr/share`
    // AppImage-style structure). Must match the layout the packaging script creates in
    // the portable zip.
    #[cfg(target_os = "windows")]
    if let Some(exe_path) = exe_path {
        if let Some(exe_dir) = exe_path.parent() {
            let candidate = exe_dir.join("presets");
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = exe_path;

    // macOS packaged fallback: not implemented, no build/test machine
    // available

    Err(
        "OPENDROP_PRESET_DIR is not set. Point it at a directory of .milk presets, e.g.:\n  \
         OPENDROP_PRESET_DIR=/srv/http/opendrop-presets cargo run -p opendrop-app"
            .to_string(),
    )
}

/// What `OPENDROP_TEXTURE_DIR` should hold: the variable if already set,
/// else the folder chosen in the app. `None` leaves it unset.
pub(crate) fn texture_dir_env_value(current: Option<OsString>, configured: Option<String>) -> Option<String> {
    if current.is_some_and(|v| !v.is_empty()) {
        return None;
    }
    configured.map(|dir| dir.trim().to_string()).filter(|dir| !dir.is_empty())
}

/// Display name for a preset file: its path relative to `preset_dir`, with
/// the extension stripped and `/` replaced by ` - `. Stable and
/// manifest-free, and it doubles as the categorization key
/// `category_from_name` expects. The full catalog scan and the 4-deck
/// bootstrap load both derive names through here, so a deck card shows
/// exactly the name the preset browser lists for the same file.
pub(crate) fn preset_display_name(preset_dir: &Path, path: &Path) -> String {
    path.strip_prefix(preset_dir).unwrap_or(path).with_extension("").to_string_lossy().replace('/', " - ")
}

/// Disk-cache root for rendered preset thumbnails:
/// `$XDG_CACHE_HOME/opendrop/thumbnails`, falling back to
/// `$HOME/.cache/opendrop/thumbnails`. Deliberately not
/// `std::env::temp_dir()`: a fixed, world-predictable `/tmp` path can be
/// pre-created by another user as a symlink pointing anywhere, and it is
/// not multi-user-safe on a shared machine either. A non-absolute
/// `XDG_CACHE_HOME` is ignored, as the XDG spec requires.
pub(crate) fn thumbnail_cache_dir() -> PathBuf {
    thumbnail_cache_dir_from(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))
}

/// The env-reading half of `thumbnail_cache_dir`, split out so the fallback
/// order is testable without mutating process-global environment state.
fn thumbnail_cache_dir_from(xdg_cache_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    let xdg = xdg_cache_home.map(PathBuf::from);
    let home_cache = home.map(PathBuf::from).map(|h| h.join(".cache"));
    xdg.into_iter()
        .chain(home_cache)
        .find(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir)
        .join("opendrop")
        .join("thumbnails")
}

pub(crate) fn walk_milk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk_milk_files(&p));
            } else if p.extension().map(|e| e.eq_ignore_ascii_case("milk")).unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out
}

/// Picks up to `count` visually distinct presets, one per top-level
/// category subdirectory where possible, so the 4 decks don't all end up
/// on lookalike presets. Skips transition-only presets (near-static/black
/// by design, a bad default for eyeballing that rendering actually works),
/// same skip the earlier prototype applied when picking its one preset.
pub(crate) fn pick_distinct_presets(dir: &Path, count: usize) -> Vec<PathBuf> {
    let mut categories: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    categories.sort();

    let mut picks = Vec::with_capacity(count);
    for cat in &categories {
        if picks.len() >= count {
            break;
        }
        let mut files = walk_milk_files(cat);
        files.sort();
        if let Some(p) = files.into_iter().find(|p| !p.to_string_lossy().contains("Transition")) {
            picks.push(p);
        }
    }
    if picks.len() < count {
        let mut all = walk_milk_files(dir);
        all.sort();
        for p in all {
            if picks.len() >= count {
                break;
            }
            if !picks.contains(&p) && !p.to_string_lossy().contains("Transition") {
                picks.push(p);
            }
        }
    }
    picks
}

#[cfg(test)]
mod tests {
    use super::*;

    mod preset_display_name_tests {
        use super::*;

        #[test]
        fn strips_the_preset_root_and_the_extension() {
            let name = preset_display_name(Path::new("/presets"), Path::new("/presets/Fractal/Blobby/306 nz+.milk"));
            assert_eq!(name, "Fractal - Blobby - 306 nz+");
        }

        #[test]
        fn a_file_directly_under_the_root_keeps_just_its_stem() {
            assert_eq!(preset_display_name(Path::new("/presets"), Path::new("/presets/Solo.milk")), "Solo");
        }

        #[test]
        fn a_path_outside_the_root_falls_back_to_the_whole_path() {
            let name = preset_display_name(Path::new("/presets"), Path::new("/elsewhere/Odd.milk"));
            assert_eq!(name, " - elsewhere - Odd");
        }
    }

    mod thumbnail_cache_dir_tests {
        use super::*;

        // Only used by the POSIX-only fixtures below; gated the same way
        // so it doesn't trip an unused-function warning on Windows.
        #[cfg(not(target_os = "windows"))]
        fn os(s: &str) -> Option<OsString> {
            Some(OsString::from(s))
        }

        // These three fixtures use POSIX-absolute literals (`/xdg`,
        // `/home/u`) to exercise the `.is_absolute()` branch in
        // `thumbnail_cache_dir_from`. `Path::is_absolute()` requires a
        // drive/UNC prefix on Windows, so a bare `/xdg` is NOT absolute
        // there and the fallback-to-temp_dir branch fires instead,
        // failing these assertions, a test-fixture limitation, not a
        // bug in the function (XDG_CACHE_HOME/HOME are POSIX-only
        // conventions to begin with; see the doc comment above).
        #[test]
        #[cfg(not(target_os = "windows"))]
        fn prefers_xdg_cache_home() {
            let dir = thumbnail_cache_dir_from(os("/xdg"), os("/home/u"));
            assert_eq!(dir, PathBuf::from("/xdg/opendrop/thumbnails"));
        }

        #[test]
        #[cfg(not(target_os = "windows"))]
        fn falls_back_to_home_dot_cache() {
            let dir = thumbnail_cache_dir_from(None, os("/home/u"));
            assert_eq!(dir, PathBuf::from("/home/u/.cache/opendrop/thumbnails"));
        }

        #[test]
        #[cfg(not(target_os = "windows"))]
        fn ignores_a_relative_xdg_cache_home() {
            let dir = thumbnail_cache_dir_from(os("relative/cache"), os("/home/u"));
            assert_eq!(dir, PathBuf::from("/home/u/.cache/opendrop/thumbnails"));
        }

        #[test]
        fn never_lands_directly_in_a_shared_tmp_root() {
            // Even the last-resort branch nests under its own subdirectory
            // rather than a bare, world-predictable path.
            let dir = thumbnail_cache_dir_from(None, None);
            assert!(dir.ends_with("opendrop/thumbnails"));
        }
    }

    mod preset_dir_from_tests {
        use super::*;

        fn os(s: &str) -> Option<OsString> {
            Some(OsString::from(s))
        }

        /// A fresh temp directory under `std::env::temp_dir()`, removed
        /// when dropped. No `tempdir`-style crate is in use elsewhere in
        /// this suite, so this is the plain-`std` equivalent.
        struct ScratchDir {
            path: PathBuf,
        }

        impl ScratchDir {
            fn new(unique: &str) -> Self {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system clock before UNIX epoch")
                    .as_nanos();
                let path = std::env::temp_dir().join(format!(
                    "opendrop-preset_dir_from_tests-{unique}-{}-{nanos}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&path).expect("create scratch dir");
                ScratchDir { path }
            }
        }

        impl Drop for ScratchDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }

        const MISSING_ENV_MESSAGE: &str = "OPENDROP_PRESET_DIR is not set. Point it at a directory of .milk presets, e.g.:\n  \
             OPENDROP_PRESET_DIR=/srv/http/opendrop-presets cargo run -p opendrop-app";

        #[test]
        fn env_override_wins_even_when_appdir_and_exe_path_also_exist() {
            let appdir = ScratchDir::new("env-wins-appdir");
            std::fs::create_dir_all(appdir.path.join("usr/share/opendrop/presets")).unwrap();
            let exe_dir = ScratchDir::new("env-wins-exe");
            std::fs::create_dir_all(exe_dir.path.join("presets")).unwrap();

            let dir = preset_dir_from(
                os("/explicit/override"),
                None,
                Some(appdir.path.clone().into_os_string()),
                Some(exe_dir.path.join("opendrop-app")),
            );
            assert_eq!(dir, Ok(PathBuf::from("/explicit/override")));
        }

        #[test]
        fn appdir_set_and_the_presets_subdir_exists_is_returned() {
            let appdir = ScratchDir::new("appdir-exists");
            let presets = appdir.path.join("usr/share/opendrop/presets");
            std::fs::create_dir_all(&presets).unwrap();

            let dir = preset_dir_from(None, None, Some(appdir.path.clone().into_os_string()), None);
            assert_eq!(dir, Ok(presets));
        }

        #[test]
        fn appdir_set_but_the_presets_subdir_is_missing_is_an_error() {
            let appdir = ScratchDir::new("appdir-missing-subdir");
            // appdir.path itself exists, but usr/share/opendrop/presets under
            // it does not: must never fall back to a silent bogus path.

            let dir = preset_dir_from(None, None, Some(appdir.path.clone().into_os_string()), None);
            assert_eq!(dir, Err(MISSING_ENV_MESSAGE.to_string()));
        }

        #[test]
        #[cfg(target_os = "windows")]
        fn windows_exe_with_a_sibling_presets_dir_is_returned() {
            let exe_dir = ScratchDir::new("windows-exe-sibling");
            let presets = exe_dir.path.join("presets");
            std::fs::create_dir_all(&presets).unwrap();

            let dir = preset_dir_from(None, None, None, Some(exe_dir.path.join("opendrop-app.exe")));
            assert_eq!(dir, Ok(presets));
        }

        #[test]
        #[cfg(target_os = "windows")]
        fn windows_exe_without_a_sibling_presets_dir_is_an_error() {
            let exe_dir = ScratchDir::new("windows-exe-no-sibling");

            let dir = preset_dir_from(None, None, None, Some(exe_dir.path.join("opendrop-app.exe")));
            assert_eq!(dir, Err(MISSING_ENV_MESSAGE.to_string()));
        }

        #[test]
        fn the_texture_variable_is_set_from_the_app_only_when_free() {
            assert_eq!(texture_dir_env_value(None, Some(" /t ".to_string())), Some("/t".to_string()));
            assert_eq!(texture_dir_env_value(os("/own"), Some("/t".to_string())), None);
            assert_eq!(texture_dir_env_value(None, Some("  ".to_string())), None);
            assert_eq!(texture_dir_env_value(None, None), None);
        }

        #[test]
        fn a_chosen_folder_beats_the_packaged_presets_but_not_the_env_override() {
            let chosen = ScratchDir::new("chosen");
            let appdir = ScratchDir::new("chosen-appdir");
            std::fs::create_dir_all(appdir.path.join("usr/share/opendrop/presets")).unwrap();

            let dir = preset_dir_from(None, Some(chosen.path.clone()), Some(appdir.path.clone().into_os_string()), None);
            assert_eq!(dir, Ok(chosen.path.clone()));

            let dir = preset_dir_from(os("/explicit/override"), Some(chosen.path.clone()), None, None);
            assert_eq!(dir, Ok(PathBuf::from("/explicit/override")));
        }

        #[test]
        fn a_chosen_folder_that_no_longer_exists_falls_back_to_the_packaged_presets() {
            let appdir = ScratchDir::new("missing-chosen-appdir");
            let presets = appdir.path.join("usr/share/opendrop/presets");
            std::fs::create_dir_all(&presets).unwrap();

            let dir = preset_dir_from(None, Some(PathBuf::from("/no/such/folder")), Some(appdir.path.clone().into_os_string()), None);
            assert_eq!(dir, Ok(presets));
        }

        #[test]
        fn nothing_set_is_an_error_with_the_unchanged_dev_message() {
            // Regression coverage for the current `cargo run` dev behavior:
            // same message, now returned instead of panicking.
            let dir = preset_dir_from(None, None, None, None);
            assert_eq!(dir, Err(MISSING_ENV_MESSAGE.to_string()));
        }
    }
}
