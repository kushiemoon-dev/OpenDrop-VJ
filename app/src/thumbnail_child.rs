//! Out-of-process preset thumbnail rendering. `run_render_thumbnail` runs
//! inside a child process (this same binary, re-invoked with
//! `--render-thumbnail <preset-path> <out-path>`): it inits its own EGL
//! pbuffer context, loads one preset via projectM, renders the warmup
//! frames, reads the result back, and writes the RGBA8 bytes to the cache
//! file the parent already knows how to read. `thumbnails::
//! pump_thumbnail_queue` is the parent side.
//!
//! Why out of process at all: a preset is handed to projectM here for no
//! reason other than that its tile scrolled into view in the preset
//! browser. Rendering it in-process meant one crash-prone preset out of
//! ~9800 could take down the whole app, live decks included, on nothing
//! more than a scroll. Same isolation `preflight` already gives live deck
//! loads, and it takes the 31-frame render plus the blocking
//! `glReadPixels` off the event-loop thread as well.
//!
//! There is no stdout/stderr protocol: the exit code plus the presence and
//! size of `<out-path>` are the entire result. A segfault or an abort
//! inside projectM shows up to the parent as death-by-signal, which is not
//! `ExitStatus::success()`, so it lands in the same failure branch as a
//! clean non-zero exit.

use glow::HasContext;
use opendrop_engine::thumbnail::{flip_rows_vertically, synthetic_pcm, THUMB_H, THUMB_W, WARMUP_FRAMES};
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::egl_headless;

/// Exit code for "this preset produced no usable thumbnail": projectM
/// rejected it, or the result could not be written. Distinct from 1 only
/// for a human reading the code by hand; the parent treats every non-zero
/// exit identically.
const EXIT_RENDER_FAILED: i32 = 1;

static PRESET_FAILED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn on_preset_failed(_filename: *const c_char, message: *const c_char, _user_data: *mut c_void) {
    PRESET_FAILED.store(true, Ordering::SeqCst);
    let msg = unsafe { CStr::from_ptr(message) }.to_string_lossy();
    eprintln!("[render-thumbnail] preset failed: {msg}");
}

/// Renders `preset_path`'s thumbnail into `out_path` and exits: 0 once the
/// file is fully written, non-zero otherwise. Never returns.
///
/// Registers projectM's preset-switch-failed callback the same way
/// `preflight::run_preflight_check` does, so a preset projectM rejects
/// fails outright instead of quietly writing a thumbnail of whatever
/// fallback projectM substituted.
pub fn run_render_thumbnail(preset_path: &Path, out_path: &Path) -> ! {
    let (egl_inst, display, config) = egl_headless::init_egl();
    let ctx = egl_headless::create_context(&egl_inst, display, config);
    let pb = egl_headless::create_pbuffer(&egl_inst, display, config, THUMB_W as i32, THUMB_H as i32);
    egl_inst.make_current(display, Some(pb), Some(pb), Some(ctx)).expect("eglMakeCurrent failed");
    let gl = egl_headless::make_gl(&egl_inst);

    let handle = unsafe { opendrop_engine::ffi::projectm_create() };
    assert!(!handle.is_null(), "projectm_create() returned NULL");

    let pcm = synthetic_pcm();
    unsafe {
        opendrop_engine::ffi::projectm_set_window_size(handle, THUMB_W as usize, THUMB_H as usize);
        opendrop_engine::ffi::projectm_set_preset_switch_failed_event_callback(handle, Some(on_preset_failed), std::ptr::null_mut());
        let c_path = CString::new(preset_path.to_string_lossy().as_bytes()).expect("preset path is not a valid C string");
        opendrop_engine::ffi::projectm_load_preset_file(handle, c_path.as_ptr(), false);

        // Same loop the in-process renderer ran: `0..=WARMUP_FRAMES`, so 31
        // frames, each fed the same PCM chunk, each with projectM's GL
        // state churn saved and restored around it exactly as
        // `Deck::render_frame` does. Kept identical on purpose: a
        // thumbnail rendered here has to look like the one the previous
        // in-process path produced, since cache entries written by either
        // are indistinguishable on disk.
        for _ in 0..=WARMUP_FRAMES {
            opendrop_engine::ffi::projectm_pcm_add_float(
                handle,
                pcm.as_ptr(),
                (pcm.len() / 2) as u32,
                opendrop_engine::ffi::projectm_channels_PROJECTM_STEREO,
            );
            let before = opendrop_engine::gl_state::save(&gl);
            opendrop_engine::ffi::projectm_opengl_render_frame(handle);
            opendrop_engine::gl_state::restore(&gl, &before);
        }
    }

    if PRESET_FAILED.load(Ordering::SeqCst) {
        std::process::exit(EXIT_RENDER_FAILED);
    }

    let pixels = read_back(&gl);
    let flipped = flip_rows_vertically(&pixels, (THUMB_W * 4) as usize);
    match write_atomically(out_path, &flipped) {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("[render-thumbnail] {e}");
            std::process::exit(EXIT_RENDER_FAILED);
        }
    }
}

/// Reads this context's pbuffer (FBO 0) back into CPU memory, rows
/// bottom-first as `glReadPixels` hands them over.
///
/// The resets first are not optional. `projectm_opengl_render_frame` leaves
/// `READ_FRAMEBUFFER_BINDING` pointed at one of projectM's own internal FBOs
/// (the whole reason `gl_state::reset_read_framebuffer_to_fbo0` exists: see
/// its doc comment), and none of the pixel *pack* state is part of the
/// `gl_state` snapshot restored around each rendered frame, which covers
/// `UNPACK_ALIGNMENT` only. So every input `glReadPixels` reads besides the
/// framebuffer itself is set here in absolute terms rather than assumed:
/// the pack buffer binding, the alignment, and the row/skip offsets. In
/// practice libprojectM 4.1.6 appears to touch only unpack state, so this is
/// belt and braces: but "absolute reset" has to mean all of it to be worth
/// claiming.
fn read_back(gl: &glow::Context) -> Vec<u8> {
    let mut pixels = vec![0u8; (THUMB_W * THUMB_H * 4) as usize];
    opendrop_engine::gl_state::reset_read_framebuffer_to_fbo0(gl);
    unsafe {
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
        gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 0);
        gl.pixel_store_i32(glow::PACK_SKIP_ROWS, 0);
        gl.pixel_store_i32(glow::PACK_SKIP_PIXELS, 0);
        gl.read_pixels(
            0,
            0,
            THUMB_W as i32,
            THUMB_H as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
    }
    pixels
}

/// Writes `bytes` to `dest` via a sibling temp file plus a rename, so a
/// crash, a kill, or a full disk partway through can never leave a
/// half-written file at `dest`: the parent's `read_cached` would see a
/// short file and reject it, but only after having already treated the
/// child's clean exit as a success. The rename is atomic because the temp
/// file is created in `dest`'s own directory, and it carries this child's
/// pid so two children writing into the same cache directory cannot
/// collide. A leftover temp file (from a kill between write and rename) is
/// never mistaken for a cache entry: `cache_path` only ever looks for
/// `.rgba`.
fn write_atomically(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = dest.parent().ok_or_else(|| format!("{} has no parent directory", dest.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let tmp = dest.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("failed to write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, dest).map_err(|e| format!("failed to rename {} into place: {e}", tmp.display()))
}
