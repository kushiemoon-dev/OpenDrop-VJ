//! The Video panel's clip library: what replaces the web app's IndexedDB
//! blob store (`video-store.ts`) with plain files on disk (Step 14 of the
//! Phase 8 VJ-panels plan).
//!
//! Two sources, in the same order the web listed `[...builtinClips,
//! ...userClips]`:
//!  - **bundled loops**, `<exe dir>/assets/video-loops/`: the native
//!    equivalent of the web's `cdn-video-loops` pack. Per the plan's
//!    Override 5 this step ships the *folder and its README*, not the ~46
//!    `.webm` files themselves (an asset-relocation task, deliberately left
//!    to a manual step); the directory being absent or empty is the normal
//!    case and is not an error.
//!  - **user clips**, `ProjectDirs::data_dir()/video-clips/`: where the
//!    panel's "+ Video" button copies whatever the file dialog returned.
//!    The *data* dir, not the config dir `ui.json` uses (`app::config`):
//!    these are user content, not settings.
//!
//! A clip's identity ([`VideoClip::key`]) is its absolute path. The web
//! keyed builtins by URL and user clips by a generated UUID; a path is the
//! same thing for both here: stable across restarts, unique, and already
//! what the decoder needs. It is what `core::video`'s
//! `selected_clip_keys`/`current_clip_index` are expressed against.

use std::path::{Path, PathBuf};

/// Extensions offered in the import dialog and recognised by the scan.
/// Nothing is decoded here: ffmpeg is what will actually open the file,
/// so this list only needs to keep obviously-wrong files out of the
/// library, not to be exhaustive.
pub(crate) const VIDEO_EXTENSIONS: [&str; 8] = ["webm", "mp4", "mov", "mkv", "avi", "m4v", "ogv", "mpg"];

/// One entry in the clip library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VideoClip {
    /// Stable identity: the absolute path, as a string. See the module
    /// doc comment.
    pub(crate) key: String,
    /// File stem, for display (the web showed `file.name` minus its
    /// extension).
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    /// From the bundled pack rather than the user's own folder: shown
    /// with a 📦 marker and not deletable from the panel, same split the
    /// web made between builtin and user clips.
    pub(crate) builtin: bool,
}

/// `ProjectDirs::data_dir()/video-clips`, or `None` in an environment with
/// no home directory at all: same contract (and same "treat it as nothing
/// to load, nothing to save") as `config::config_file_path`.
pub(crate) fn user_clips_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "opendrop-native").map(|dirs| dirs.data_dir().join("video-clips"))
}

/// `<directory of the running binary>/assets/video-loops`, or `None` if
/// the executable's own path can't be resolved. Same `current_exe()`-
/// relative convention `thumbnails::spawn_render` already relies on.
pub(crate) fn bundled_clips_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|dir| dir.join("assets").join("video-loops"))
}

/// The whole library: bundled loops first, then the user's own, each
/// sorted by filename so the list (and therefore every saved
/// `current_clip_index`) is stable across runs.
pub(crate) fn scan_clips() -> Vec<VideoClip> {
    let mut clips = scan_dir(bundled_clips_dir().as_deref(), true);
    clips.extend(scan_dir(user_clips_dir().as_deref(), false));
    clips
}

/// Every recognised video file directly inside `dir`, sorted by filename.
/// A missing/unreadable directory yields nothing: the normal case for the
/// bundled folder (see the module doc comment), and for a user who has
/// never imported a clip.
fn scan_dir(dir: Option<&Path>, builtin: bool) -> Vec<VideoClip> {
    let Some(dir) = dir else { return Vec::new() };
    let Ok(read_dir) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut entries: Vec<PathBuf> = read_dir.flatten().map(|e| e.path()).filter(|p| has_video_extension(p)).collect();
    entries.sort();
    entries.into_iter().map(|path| clip_from_path(path, builtin)).collect()
}

/// Case-insensitive extension check against [`VIDEO_EXTENSIONS`].
fn has_video_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.as_str()))
}

fn clip_from_path(path: PathBuf, builtin: bool) -> VideoClip {
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "clip".to_string());
    VideoClip { key: path.to_string_lossy().into_owned(), name, path, builtin }
}

/// Copies `source` into the user clip folder and returns the resulting
/// entry. Port of `addVideoFromFile`, including its size cap
/// (`core::video::MAX_CLIP_BYTES`); the web wrote the blob into IndexedDB,
/// this writes a file.
///
/// A name collision gets a ` (2)`, ` (3)`, … suffix rather than
/// overwriting: two different files with the same basename are a normal
/// thing to import, and silently replacing one would lose it.
pub(crate) fn import_clip(source: &Path) -> Result<VideoClip, String> {
    let dir = user_clips_dir().ok_or("no data directory available on this system")?;
    let size = std::fs::metadata(source).map_err(|e| format!("reading {}: {e}", source.display()))?.len();
    if size > opendrop_core::video::MAX_CLIP_BYTES {
        return Err(format!(
            "{} is {} MB, exceeding the {} MB limit",
            source.display(),
            size / (1024 * 1024),
            opendrop_core::video::MAX_CLIP_BYTES / (1024 * 1024)
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let target = free_target_path(&dir, source);
    std::fs::copy(source, &target).map_err(|e| format!("copying to {}: {e}", target.display()))?;
    Ok(clip_from_path(target, false))
}

/// First unused `<dir>/<stem>[ (n)].<ext>` for `source`'s filename.
fn free_target_path(dir: &Path, source: &Path) -> PathBuf {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "clip".to_string());
    let ext = source.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "webm".to_string());
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    // Bounded rather than `loop`: 999 same-named imports is well past any
    // real use, and an unbounded loop here could spin forever against a
    // filesystem that keeps reporting every candidate as existing.
    for n in 2..1000 {
        let candidate = dir.join(format!("{stem} ({n}).{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    first
}

/// Deletes a user clip's file. Bundled clips are never deletable (the
/// panel doesn't offer the button, and this refuses anyway); they aren't
/// the user's to remove from inside the app.
pub(crate) fn delete_clip(clip: &VideoClip) -> Result<(), String> {
    if clip.builtin {
        return Err(format!("{} is a bundled loop and can't be deleted here", clip.name));
    }
    std::fs::remove_file(&clip.path).map_err(|e| format!("deleting {}: {e}", clip.path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opendrop-clips-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn video_extensions_are_recognised_case_insensitively() {
        assert!(has_video_extension(Path::new("/a/b.webm")));
        assert!(has_video_extension(Path::new("/a/b.MP4")));
        assert!(has_video_extension(Path::new("/a/b.MoV")));
        assert!(!has_video_extension(Path::new("/a/b.milk")));
        assert!(!has_video_extension(Path::new("/a/b.png")));
        assert!(!has_video_extension(Path::new("/a/b")));
    }

    #[test]
    fn a_clips_key_is_its_absolute_path_and_its_name_is_the_file_stem() {
        let clip = clip_from_path(PathBuf::from("/loops/Neon Grid.webm"), true);
        assert_eq!(clip.key, "/loops/Neon Grid.webm");
        assert_eq!(clip.name, "Neon Grid");
        assert!(clip.builtin);
    }

    #[test]
    fn scanning_a_missing_directory_yields_nothing_rather_than_an_error() {
        assert_eq!(scan_dir(Some(Path::new("/nonexistent/opendrop-clips")), false), Vec::new());
        assert_eq!(scan_dir(None, false), Vec::new());
    }

    #[test]
    fn a_scan_returns_only_video_files_sorted_by_filename() {
        let dir = temp_dir("scan");
        for name in ["zeta.webm", "alpha.mp4", "notes.txt", "cover.png"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }

        let clips = scan_dir(Some(&dir), false);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(clips.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["alpha", "zeta"]);
        assert!(clips.iter().all(|c| !c.builtin));
    }

    #[test]
    fn importing_copies_the_file_and_returns_a_user_clip() {
        let src_dir = temp_dir("import-src");
        let dst_dir = temp_dir("import-dst");
        let source = src_dir.join("Loop One.webm");
        std::fs::write(&source, b"not really a video").unwrap();

        let target = free_target_path(&dst_dir, &source);
        std::fs::copy(&source, &target).unwrap();
        let clip = clip_from_path(target, false);

        assert_eq!(clip.name, "Loop One");
        assert!(clip.path.starts_with(&dst_dir));
        assert!(clip.path.exists());
        assert!(!clip.builtin);

        std::fs::remove_dir_all(&src_dir).unwrap();
        std::fs::remove_dir_all(&dst_dir).unwrap();
    }

    #[test]
    fn a_name_collision_gets_a_numbered_suffix_instead_of_overwriting() {
        let dir = temp_dir("collision");
        let source = dir.join("src").join("clip.webm");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"x").unwrap();

        let first = free_target_path(&dir, &source);
        assert_eq!(first, dir.join("clip.webm"));
        std::fs::write(&first, b"x").unwrap();

        let second = free_target_path(&dir, &source);
        assert_eq!(second, dir.join("clip (2).webm"));
        std::fs::write(&second, b"x").unwrap();

        assert_eq!(free_target_path(&dir, &source), dir.join("clip (3).webm"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn importing_a_file_over_the_size_cap_is_refused_with_a_readable_message() {
        let dir = temp_dir("oversize");
        let source = dir.join("huge.webm");
        // Sparse-ish: just past the cap, written once.
        let file = std::fs::File::create(&source).unwrap();
        file.set_len(opendrop_core::video::MAX_CLIP_BYTES + 1).unwrap();
        drop(file);

        let err = import_clip(&source).unwrap_err();
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(err.contains("exceeding the"), "unexpected message: {err}");
    }

    #[test]
    fn importing_a_missing_file_reports_which_file_failed() {
        let err = import_clip(Path::new("/nonexistent/opendrop-no-clip.webm")).unwrap_err();
        assert!(err.contains("opendrop-no-clip.webm"), "unexpected message: {err}");
    }

    #[test]
    fn deleting_a_user_clip_removes_its_file() {
        let dir = temp_dir("delete");
        let path = dir.join("gone.webm");
        std::fs::write(&path, b"x").unwrap();
        let clip = clip_from_path(path.clone(), false);

        assert!(delete_clip(&clip).is_ok());
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bundled_clip_is_never_deleted_from_inside_the_app() {
        let dir = temp_dir("delete-builtin");
        let path = dir.join("bundled.webm");
        std::fs::write(&path, b"x").unwrap();
        let clip = clip_from_path(path.clone(), true);

        assert!(delete_clip(&clip).is_err());
        assert!(path.exists(), "the file must still be there");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_two_library_directories_are_distinct_and_never_the_config_dir() {
        // Both are `Option` in a home-less environment; when they exist,
        // user clips live under the DATA dir (not the config dir `ui.json`
        // uses) and the bundled pack sits next to the binary.
        if let (Some(user), Some(bundled)) = (user_clips_dir(), bundled_clips_dir()) {
            assert_ne!(user, bundled);
            assert!(user.ends_with("video-clips"));
            assert!(bundled.ends_with("video-loops"));
            if let Some(config) = crate::config::config_file_path() {
                assert_ne!(Some(user.as_path()), config.parent());
            }
        }
    }

    #[test]
    fn scan_clips_on_this_machine_does_not_panic() {
        let _ = scan_clips();
    }
}
