pub mod compositor;
pub mod deck;
pub mod ffi;
pub mod gl_debug;
pub mod gl_state;
pub mod overlay_texture;
pub mod preset_patch;
pub mod qvar_patch;
pub mod readback;
pub mod thumbnail;
pub mod time_patch;
pub mod texture_dir;
pub mod timing;

/// (major, minor, patch) of the linked libprojectM: proves bindgen + linking work.
pub fn projectm_version() -> (i32, i32, i32) {
    let mut major = 0;
    let mut minor = 0;
    let mut patch = 0;
    unsafe { ffi::projectm_get_version_components(&mut major, &mut minor, &mut patch) };
    (major, minor, patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Linux links the system libprojectM (4.1.6); the Windows build uses the
    /// vcpkg overlay port, which pins 4.2.0 for its GLES 3.1 patch.
    #[test]
    fn version_is_the_pinned_one_for_the_platform() {
        let expected = if cfg!(windows) { "4.2.0" } else { "4.1.6" };
        let (major, minor, patch) = projectm_version();
        assert_eq!(format!("{major}.{minor}.{patch}"), expected);
    }
}
