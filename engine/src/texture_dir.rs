//! A folder of textures for presets that reference images by name.
//!
//! projectM looks next to the preset and in its own texture paths. A user
//! folder is handed over through `OPENDROP_TEXTURE_DIR`, an environment
//! variable so the helper processes that render thumbnails and scan presets
//! (which start fresh) see the same folder as the live decks.

use std::ffi::{CString, OsStr};

use crate::ffi;

pub const TEXTURE_DIR_ENV: &str = "OPENDROP_TEXTURE_DIR";

/// The value as a C string, or `None` when it is empty or holds a NUL byte.
fn texture_path(value: &OsStr) -> Option<CString> {
    if value.is_empty() {
        return None;
    }
    CString::new(value.to_string_lossy().as_bytes()).ok()
}

/// Adds the user's texture folder to `handle`'s search paths, when one is set.
///
/// # Safety
/// `handle` must be a live projectM instance whose GL context is current.
pub unsafe fn apply_texture_dir(handle: ffi::projectm_handle) {
    let Some(path) = std::env::var_os(TEXTURE_DIR_ENV).and_then(|v| texture_path(&v)) else { return };
    let paths = [path.as_ptr()];
    ffi::projectm_set_texture_search_paths(handle, paths.as_ptr().cast_mut(), 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_or_unusable_value_sets_nothing() {
        assert!(texture_path(OsStr::new("")).is_none());
        assert!(texture_path(OsStr::new("bad\0path")).is_none());
    }

    #[test]
    fn a_folder_becomes_a_c_string() {
        assert_eq!(texture_path(OsStr::new("/home/me/textures")).unwrap().to_str().unwrap(), "/home/me/textures");
    }
}
