pub mod compositor;
pub mod deck;
pub mod ffi;
pub mod gl_debug;
pub mod gl_state;
pub mod preset_patch;
pub mod readback;
pub mod thumbnail;
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

    #[test]
    fn version_is_4_1_6() {
        let (major, minor, patch) = projectm_version();
        assert_eq!(format!("{major}.{minor}.{patch}"), "4.1.6");
    }
}
