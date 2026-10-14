use std::path::Path;
use winit::event_loop::EventLoop;

mod app;
mod bootstrap;
mod config;
mod deck_video;
mod egl_headless;
mod fatal;
mod frame;
mod keymap;
mod overlays;
mod panel;
mod paths;
mod preflight;
mod preset_load;
mod session;
mod state;
mod theme;
mod thumbnail_child;
mod thumbnails;
mod ui;
mod ui_root;
mod video_clips;

pub(crate) use panel::Panel;
use bootstrap::{bootstrap, WindowSlot};
use state::{App, AppState, InvisibleMode};

/// Known limit: paced off the control window's monitor only, read once at
/// bootstrap. A VJ setup can have control and output on different-refresh
/// monitors; revisit if that ever causes visible judder on the output side.
const FALLBACK_REFRESH_MILLIHERTZ: u32 = 60_000;

// projectM only ships `projectm_set_log_callback` from 4.2.0 onward (the
// pinned Windows/GLES overlay port); the Arch `projectm-4.pc` package this
// links against on Linux is still 4.1.6, so this diagnostic hook is
// Windows-only. Without it, every internal GLResolver/GladLoader failure
// reason (e.g. "no current GL context") is silently discarded, and
// `projectm_create()` returning NULL carries no explanation at all.
#[cfg(target_os = "windows")]
unsafe extern "C" fn log_projectm_message(
    message: *const std::os::raw::c_char,
    log_level: opendrop_engine::ffi::projectm_log_level,
    _user_data: *mut std::os::raw::c_void,
) {
    let msg = unsafe { std::ffi::CStr::from_ptr(message) }.to_string_lossy();
    eprintln!("[projectM level={log_level}] {msg}");
}

fn main() {
    #[cfg(target_os = "windows")]
    unsafe {
        opendrop_engine::ffi::projectm_set_log_callback(Some(log_projectm_message), false, std::ptr::null_mut());
    }

    // Hidden subcommands, both of them this same binary re-invoked as a
    // child process by the parent app, both of them `-> !`. Neither is a
    // user-facing CLI: the parent builds these argv lines itself, and the
    // whole result protocol is the exit code (plus, for --render-thumbnail,
    // the file it wrote).
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--preflight-check" {
        preflight::run_preflight_check(Path::new(&args[2]));
    }
    if args.len() >= 4 && args[1] == "--render-thumbnail" {
        thumbnail_child::run_render_thumbnail(Path::new(&args[2]), Path::new(&args[3]));
    }

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            fatal::report_fatal("OpenDrop could not open a window", &e.to_string());
            std::process::exit(1);
        }
    };
    let mut app = App::default();
    if let Err(e) = event_loop.run_app(&mut app) {
        fatal::report_fatal("OpenDrop stopped unexpectedly", &e.to_string());
        std::process::exit(1);
    }
}

#[cfg(test)]
mod gl_tests;
