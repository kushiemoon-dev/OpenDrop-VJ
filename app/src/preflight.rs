//! Out-of-process preset pre-flight validation. `run_preflight_check` runs
//! inside a child process (this same binary, re-invoked with
//! `--preflight-check <path>`): it tries to init EGL, build a small pbuffer
//! context, and load one preset via projectM, isolating a crash-prone
//! preset from the decks actually running the show. `spawn_preflight` is
//! the parent side: a dedicated thread per request, never the event-loop
//! thread, with a timeout.
//!
//! Ported from an earlier prototype's `check_single_preset` (a
//! pattern already proven there), with two differences: `opendrop_engine::
//! ffi` instead of the spike's local `mod ffi`, and a smaller resolution:
//! a validation check has no visual-quality requirement. The EGL bootstrap
//! itself lives in `egl_headless`, shared with the `--render-thumbnail`
//! child.

use crate::egl_headless::{create_context, create_pbuffer, init_egl};
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PREFLIGHT_W: i32 = 320;
const PREFLIGHT_H: i32 = 180;

// ---------------------------------------------------------- child process

static PRESET_FAILED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn on_preset_failed(_filename: *const c_char, message: *const c_char, _user_data: *mut c_void) {
    PRESET_FAILED.store(true, Ordering::SeqCst);
    let msg = unsafe { CStr::from_ptr(message) }.to_string_lossy();
    eprintln!("[preflight] preset failed: {msg}");
}

/// Runs entirely inside a `--preflight-check` child process: load one
/// preset, render a few frames, exit(1) if projectM's failure callback
/// fired, exit(0) otherwise. A crash (segfault, abort) shows up to the
/// parent as death-by-signal, not exit(1): `spawn_preflight` treats
/// anything other than a clean `exit(0)` as a failed validation.
pub fn run_preflight_check(path: &Path) -> ! {
    let (egl_inst, display, config) = init_egl();
    let ctx = create_context(&egl_inst, display, config);
    let pb = create_pbuffer(&egl_inst, display, config, PREFLIGHT_W, PREFLIGHT_H);
    egl_inst.make_current(display, Some(pb), Some(pb), Some(ctx)).expect("eglMakeCurrent failed");

    let handle = unsafe { opendrop_engine::ffi::projectm_create() };
    assert!(!handle.is_null(), "projectm_create() returned NULL");
    unsafe {
        opendrop_engine::ffi::projectm_set_window_size(handle, PREFLIGHT_W as usize, PREFLIGHT_H as usize);
        opendrop_engine::ffi::projectm_set_preset_switch_failed_event_callback(handle, Some(on_preset_failed), std::ptr::null_mut());
        let c = CString::new(path.to_string_lossy().as_bytes()).unwrap();
        opendrop_engine::ffi::projectm_load_preset_file(handle, c.as_ptr(), false);
        for _ in 0..5 {
            opendrop_engine::ffi::projectm_opengl_render_frame(handle);
        }
    }
    std::process::exit(if PRESET_FAILED.load(Ordering::SeqCst) { 1 } else { 0 });
}

// --------------------------------------------------------------- parent side

pub enum PreflightVerdict {
    Ok,
    Failed(String),
}

/// Spawns a dedicated thread for this one validation request: UI click
/// volume triggering these is low, so a thread per request is plenty; no
/// pool. Never call this from the event-loop thread. The child is this
/// same binary re-invoked with `--preflight-check`, killed if it hasn't
/// exited within `TIMEOUT`. Result goes back over `result_tx`, read by
/// `about_to_wait`'s non-blocking drain.
pub fn spawn_preflight(path: PathBuf, slot: usize, name: String, result_tx: mpsc::Sender<(usize, String, PreflightVerdict)>) {
    std::thread::spawn(move || {
        // Minor #11: fallible, not `.expect()`. A panic here would drop
        // `result_tx` without ever sending on it, leaving this slot stuck
        // in `pending_validations` forever (the UI shows "Validating…"
        // indefinitely, since nothing else ever clears it).
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                let _ = result_tx.send((slot, name, PreflightVerdict::Failed(format!("current_exe() failed: {e}"))));
                return;
            }
        };
        let mut child = match Command::new(&exe).arg("--preflight-check").arg(&path).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = result_tx.send((slot, name, PreflightVerdict::Failed(format!("spawn failed: {e}"))));
                return;
            }
        };
        const TIMEOUT: Duration = Duration::from_secs(5);
        let start = Instant::now();
        let verdict = loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break PreflightVerdict::Ok,
                Ok(Some(_)) => break PreflightVerdict::Failed("preset rejected by projectM".to_string()),
                Ok(None) if start.elapsed() > TIMEOUT => {
                    let _ = child.kill();
                    let _ = child.wait(); // reap: kill() alone leaves a zombie until this process exits
                    break PreflightVerdict::Failed("preflight check timed out".to_string());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => {
                    // Minor #10: still attempt to reap the child on this
                    // error path too, same as the timeout branch above,
                    // instead of leaking it.
                    let _ = child.kill();
                    let _ = child.wait();
                    break PreflightVerdict::Failed(format!("wait failed: {e}"));
                }
            }
        };
        let _ = result_tx.send((slot, name, verdict));
    });
}
