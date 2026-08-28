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
//! a validation check has no visual-quality requirement.

use khronos_egl as egl;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

type Egl = egl::DynamicInstance<egl::Latest>;

const PREFLIGHT_W: i32 = 320;
const PREFLIGHT_H: i32 = 180;

// ---------------------------------------------------------- child process

fn init_egl() -> (Egl, egl::Display, egl::Config) {
    let inst = unsafe { egl::DynamicInstance::<egl::Latest>::load_required() }.expect("failed to load libEGL.so.1");
    let display = unsafe { inst.get_display(egl::DEFAULT_DISPLAY) }.expect("eglGetDisplay failed");
    inst.initialize(display).expect("eglInitialize failed");
    inst.bind_api(egl::OPENGL_API).expect("eglBindAPI(OPENGL_API) failed");

    let config_attribs = [
        egl::SURFACE_TYPE,
        egl::PBUFFER_BIT,
        egl::RENDERABLE_TYPE,
        egl::OPENGL_BIT,
        egl::RED_SIZE,
        8,
        egl::GREEN_SIZE,
        8,
        egl::BLUE_SIZE,
        8,
        egl::ALPHA_SIZE,
        8,
        egl::NONE,
    ];
    let config = inst
        .choose_first_config(display, &config_attribs)
        .expect("eglChooseConfig failed")
        .expect("no matching EGL config for pbuffer+OpenGL");
    (inst, display, config)
}

// No `share` context param here (unlike the spike's general-purpose
// version): a preflight child never has more than this one context.
fn create_context(inst: &Egl, display: egl::Display, config: egl::Config) -> egl::Context {
    let attribs = [
        egl::CONTEXT_MAJOR_VERSION,
        3,
        egl::CONTEXT_MINOR_VERSION,
        3,
        egl::CONTEXT_OPENGL_PROFILE_MASK,
        egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
        egl::NONE,
    ];
    inst.create_context(display, config, None, &attribs).expect("eglCreateContext failed")
}

fn create_pbuffer(inst: &Egl, display: egl::Display, config: egl::Config, w: i32, h: i32) -> egl::Surface {
    let attribs = [egl::WIDTH, w, egl::HEIGHT, h, egl::NONE];
    inst.create_pbuffer_surface(display, config, &attribs).expect("eglCreatePbufferSurface failed")
}

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
#[allow(dead_code)] // wired up to the UI by Task 14
pub fn spawn_preflight(path: PathBuf, slot: usize, name: String, result_tx: mpsc::Sender<(usize, String, PreflightVerdict)>) {
    std::thread::spawn(move || {
        let exe = std::env::current_exe().expect("current_exe() failed");
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
                Err(e) => break PreflightVerdict::Failed(format!("wait failed: {e}")),
            }
        };
        let _ = result_tx.send((slot, name, verdict));
    });
}
