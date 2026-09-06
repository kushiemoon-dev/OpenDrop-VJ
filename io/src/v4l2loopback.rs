//! v4l2loopback output: pipes the compositor's RGBA readback bytes into an
//! existing v4l2loopback device via an `ffmpeg` subprocess, fed by its own
//! `mpsc::Receiver<Vec<u8>>`: a second channel pair alongside `io::ndi`'s,
//! since the original `compositor_frame_rx` was already moved into
//! `opendrop_io::ndi::spawn` and each consumer needs its own `Sender`.
//!
//! Mirrors `findV4l2Device`, OpenDrop-VJ's `electron/main.cjs:82-99`,
//! exactly: [`find_device`] is a pure filesystem scan, no subprocess, no
//! side effects.
//!
//! **RGBA, not BGRA**: same departure from the OpenDrop-VJ reference as
//! `io::ndi::out` (see that module's doc comment): the readback is
//! native GL RGBA8, not `Electron.capturePage()`'s BGRA, so there is no
//! reason to swizzle just to match the old pix_fmt.
//!
//! **No "capture first frame to lock size" trick**: the reference captures
//! one frame before spawning ffmpeg because `capturePage()`'s resolution
//! isn't known until it runs. Here the compositor's resolution is fixed at
//! compile time ([`COMP_W`]/[`COMP_H`]), so ffmpeg is spawned immediately
//! with `-s` already set.
//!
//! Mirrors `osc::spawn`/`OscHandle`'s shape: a dedicated `std::thread`
//! publishes continuous state via `ArcSwap` and never panics on an ffmpeg
//! spawn/write failure: it logs once and leaves [`V4l2Snapshot::running`]
//! `false`. `stdin.write_all` (in [`FfmpegPipe::write_frame`]) may block if
//! ffmpeg is slow to consume: acceptable here since this thread never
//! affects rendering, which lives on a different thread and only does a
//! non-blocking `send()` into this thread's channel.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;

/// Mirrors `engine::compositor::COMP_W`/`COMP_H`: hardcoded here rather
/// than depending on `opendrop-engine` (a GL/projectM crate) from this
/// I/O-only crate just for 2 integers: same precedent as `io::ndi::out`'s
/// local `COMP_W`/`COMP_H` (see that module's doc comment for the full
/// rationale, including the "a mismatch fails loudly, not silently" note).
const COMP_W: u32 = 1920;
const COMP_H: u32 = 1080;

/// Expected length of one compositor readback buffer: `COMP_W * COMP_H`
/// RGBA8 pixels, 4 bytes each. A frame of any other length is dropped (see
/// [`FfmpegPipe::write_frame`]) rather than fed to ffmpeg misaligned.
const EXPECTED_FRAME_LEN: usize = (COMP_W * COMP_H * 4) as usize;

/// `-r` passed to ffmpeg, matching the OpenDrop-VJ reference
/// (`electron/main.cjs:818+`).
const FRAME_RATE: u32 = 30;

/// How often the run loop wakes up to drain the frame receiver even when no
/// control message has arrived. Same role as `io::ndi::out::POLL_TICK`.
const POLL_TICK: Duration = Duration::from_millis(5);

/// Scans `/sys/class/video4linux/*/name` for the first device whose label
/// contains "OpenDrop", returning its `/dev/videoN` path. Pure filesystem
/// scan: no subprocess, no side effects. `None` covers every "no such
/// device" case alike (missing `/sys/class/video4linux`, an empty
/// directory, or no matching label): mirrors `findV4l2Device`'s `try {
/// ... } catch { return null }` fallback, `electron/main.cjs:82-99`.
pub fn find_device() -> Option<PathBuf> {
    find_device_in(Path::new("/sys/class/video4linux"))
}

/// The testable core of [`find_device`], split out so a test can point it
/// at a synthetic directory tree instead of the real sysfs path. Entries
/// are sorted by filename before scanning so "the first match" is
/// deterministic across runs/platforms, unlike raw `read_dir` order (which
/// the JS reference doesn't guarantee either, since `fs.readdirSync` order
/// is filesystem-dependent).
fn find_device_in(base: &Path) -> Option<PathBuf> {
    let mut entries: Vec<_> = std::fs::read_dir(base).ok()?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Ok(label) = std::fs::read_to_string(entry.path().join("name")) else { continue };
        if label_matches(&label) {
            return Some(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    None
}

/// Pure predicate factored out of [`find_device_in`]: does this device
/// label (the raw contents of a `.../videoN/name` file) identify an
/// OpenDrop v4l2loopback device? Trimmed before matching since sysfs
/// `name` files carry a trailing newline.
fn label_matches(label: &str) -> bool {
    label.trim().contains("OpenDrop")
}

/// Continuous state published via `V4l2Handle::latest()`: never blocks,
/// always the latest known value (mirrors `OscSnapshot`).
#[derive(Debug, Clone, Default)]
pub struct V4l2Snapshot {
    pub running: bool,
}

/// Outward control messages sent to the v4l2loopback thread.
pub enum V4l2Control {
    /// Spawns ffmpeg piping into `device`. Replaces any pipe already
    /// running, same unconditional-restart convention as `OscControl::
    /// Start`.
    Start(PathBuf),
    /// Kills the ffmpeg process, if any. A no-op while already idle.
    Stop,
}

/// Handle to the running v4l2loopback thread. Mirrors `OscHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block.
pub struct V4l2Handle {
    state: Arc<ArcSwap<V4l2Snapshot>>,
    pub control_tx: Sender<V4l2Control>,
}

impl V4l2Handle {
    /// Never blocks: an atomic load of the current Arc (mirrors
    /// `OscHandle::latest`).
    pub fn latest(&self) -> Arc<V4l2Snapshot> {
        self.state.load_full()
    }
}

/// Spawns the dedicated v4l2loopback thread and returns immediately. The
/// thread starts idle (no ffmpeg process) until it receives `V4l2Control::
/// Start`: mirrors `OscHandle::spawn`'s "starts idle until an explicit
/// connect" pattern.
///
/// `compositor_rx` is this module's own second channel pair (see the module
/// doc comment for why it isn't shared with `io::ndi`'s), passed in by
/// value.
pub fn spawn(compositor_rx: Receiver<Vec<u8>>) -> V4l2Handle {
    let state = Arc::new(ArcSwap::from_pointee(V4l2Snapshot::default()));
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, compositor_rx, control_rx)
    });
    V4l2Handle { state, control_tx }
}

/// The running ffmpeg subprocess plus its stdin handle. `stdout`/`stderr`
/// are both discarded (`Stdio::null()`): capturing `stderr` for
/// diagnostics would need a second thread draining it, or a full pipe
/// buffer would eventually stall ffmpeg: not attempted here, and
/// `Drop` still guarantees the process never outlives the pipe either way.
struct FfmpegPipe {
    child: Child,
    stdin: ChildStdin,
}

impl FfmpegPipe {
    /// Spawns `ffmpeg` reading raw RGBA frames from stdin and writing to
    /// `device` as a v4l2 sink. Same argument shape as the OpenDrop-VJ
    /// reference (`electron/main.cjs:818+`), except `-pix_fmt rgba`
    /// (native GL readback, no BGRA swizzle) and `-s` fixed to `COMP_W`x
    /// `COMP_H`: see the module doc comment.
    fn spawn(device: &Path) -> std::io::Result<Self> {
        let mut child = Command::new("ffmpeg")
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
            .arg(format!("{COMP_W}x{COMP_H}"))
            .args(["-r", &FRAME_RATE.to_string(), "-i", "pipe:0", "-f", "v4l2", "-pix_fmt", "yuv420p"])
            .arg(device)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("Stdio::piped() guarantees Some");
        Ok(Self { child, stdin })
    }

    /// Writes one RGBA frame to ffmpeg's stdin. `bytes` must be exactly
    /// `COMP_W*COMP_H*4`: a mismatch is logged and the frame dropped
    /// (mirrors `io::ndi::out::SlotSender::send`'s same-shaped guard)
    /// rather than feeding ffmpeg a misaligned rawvideo stream. May block
    /// if ffmpeg is slow to consume: see the module doc comment.
    fn write_frame(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if bytes.len() != EXPECTED_FRAME_LEN {
            eprintln!("[v4l2] dropping frame with unexpected size: {} bytes, expected {EXPECTED_FRAME_LEN}", bytes.len());
            return Ok(());
        }
        self.stdin.write_all(bytes)
    }

    /// Non-blocking liveness check: `try_wait` never blocks. `Err` (can't
    /// tell) is treated as "not alive": safer than assuming a broken pipe
    /// is still usable.
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

/// Kills and reaps the ffmpeg process on `Stop`, on a fresh `Start`
/// replacing it, and when the thread itself shuts down: never leaves a
/// zombie or orphaned ffmpeg process behind.
impl Drop for FfmpegPipe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn publish(state: &Arc<ArcSwap<V4l2Snapshot>>, running: bool) {
    state.store(Arc::new(V4l2Snapshot { running }));
}

fn handle_control(pipe: &mut Option<FfmpegPipe>, ctrl: V4l2Control, state: &Arc<ArcSwap<V4l2Snapshot>>) {
    match ctrl {
        V4l2Control::Start(device) => {
            match FfmpegPipe::spawn(&device) {
                Ok(p) => *pipe = Some(p),
                Err(e) => {
                    eprintln!("[v4l2] failed to spawn ffmpeg for {}: {e}", device.display());
                    *pipe = None;
                }
            }
            publish(state, pipe.is_some());
        }
        V4l2Control::Stop => {
            *pipe = None; // Drop kills+reaps the child, if any.
            publish(state, false);
        }
    }
}

/// Fully drains `rx` this tick. When `pipe` is `Some` (a stream was
/// started), every received frame is forwarded to ffmpeg's stdin; when
/// `None`, the bytes are simply discarded: draining still happens either
/// way so the upstream (unbounded) `mpsc::Sender` in `app` never backs up.
/// Same correctness requirement as `io::ndi::out::drain_slot`.
fn drain(rx: &Receiver<Vec<u8>>, pipe: &mut Option<FfmpegPipe>, state: &Arc<ArcSwap<V4l2Snapshot>>) {
    let Some(p) = pipe else {
        while rx.try_recv().is_ok() {}
        return;
    };
    while let Ok(bytes) = rx.try_recv() {
        if let Err(e) = p.write_frame(&bytes) {
            eprintln!("[v4l2] ffmpeg stdin write failed, stopping: {e}");
            *pipe = None;
            publish(state, false);
            while rx.try_recv().is_ok() {} // keep draining with no consumer, see doc comment
            return;
        }
    }
}

fn run(state: Arc<ArcSwap<V4l2Snapshot>>, compositor_rx: Receiver<Vec<u8>>, control_rx: Receiver<V4l2Control>) {
    let mut pipe: Option<FfmpegPipe> = None;
    publish(&state, false);

    loop {
        let mut owner_gone = false;
        if pipe.is_none() {
            // Idle: no ffmpeg process to feed or watch, so block until a
            // control message arrives instead of spinning the 5ms poll for
            // nothing: same idle/active
            // split as `io::ndi::out::run`.
            match control_rx.recv() {
                Ok(ctrl) => handle_control(&mut pipe, ctrl, &state),
                Err(_) => owner_gone = true,
            }
        } else {
            match control_rx.recv_timeout(POLL_TICK) {
                Ok(ctrl) => handle_control(&mut pipe, ctrl, &state),
                Err(RecvTimeoutError::Timeout) => {}
                // Never actually happens in practice: control_tx is held alive
                // by `app`'s AppState/V4l2Handle for the whole process
                // lifetime, same as `io::ndi::out::run`'s equivalent comment.
                Err(RecvTimeoutError::Disconnected) => owner_gone = true,
            }
        }
        while !owner_gone {
            match control_rx.try_recv() {
                Ok(ctrl) => handle_control(&mut pipe, ctrl, &state),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => owner_gone = true,
            }
        }

        // Detects ffmpeg exiting on its own (bad/removed device, killed
        // externally, etc.): try_wait never blocks.
        if let Some(p) = pipe.as_mut() {
            if !p.is_alive() {
                eprintln!("[v4l2] ffmpeg exited unexpectedly");
                pipe = None;
                publish(&state, false);
            }
        }

        // Drained every tick regardless of active state: see `drain`'s
        // doc comment.
        drain(&compositor_rx, &mut pipe, &state);

        if owner_gone {
            break; // V4l2Handle (and its control_tx) dropped: shut down.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_matching_is_substring_and_trims_whitespace() {
        assert!(label_matches("OpenDrop"));
        assert!(label_matches("OpenDrop Video\n"));
        assert!(label_matches("  OpenDrop  \n"));
        assert!(!label_matches("USB Camera"));
        assert!(!label_matches(""));
    }

    /// Real-environment test: this machine has no v4l2loopback device
    /// loaded (only a real USB webcam under `/sys/class/video4linux`,
    /// verified by hand), so `find_device()` must gracefully
    /// return `None` rather than panicking or erroring.
    #[test]
    fn find_device_returns_none_when_no_opendrop_device_exists_on_this_machine() {
        assert_eq!(find_device(), None);
    }

    #[test]
    fn find_device_in_returns_none_for_a_nonexistent_base_dir() {
        assert_eq!(find_device_in(Path::new("/nonexistent/opendrop-test-path")), None);
    }

    /// Synthetic sysfs tree proving `find_device_in` *would* find a match
    /// if a real v4l2loopback device were present: `video0`'s label is an
    /// unrelated webcam, `video1`'s label contains "OpenDrop".
    #[test]
    fn find_device_in_finds_the_first_matching_label() {
        let dir = std::env::temp_dir().join(format!("opendrop-v4l2-test-{}", std::process::id()));
        let video0 = dir.join("video0");
        let video1 = dir.join("video1");
        std::fs::create_dir_all(&video0).unwrap();
        std::fs::create_dir_all(&video1).unwrap();
        std::fs::write(video0.join("name"), "USB2.0 Camera\n").unwrap();
        std::fs::write(video1.join("name"), "OpenDrop Video\n").unwrap();

        let found = find_device_in(&dir);

        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(found, Some(PathBuf::from("/dev/video1")));
    }

    #[test]
    fn find_device_in_returns_none_when_no_label_matches() {
        let dir = std::env::temp_dir().join(format!("opendrop-v4l2-test-nomatch-{}", std::process::id()));
        let video0 = dir.join("video0");
        std::fs::create_dir_all(&video0).unwrap();
        std::fs::write(video0.join("name"), "USB2.0 Camera\n").unwrap();

        let found = find_device_in(&dir);

        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(found, None);
    }

    #[test]
    fn fresh_snapshot_is_idle() {
        let s = V4l2Snapshot::default();
        assert!(!s.running);
    }

    /// The dimension/frame-length constants mirrored from `engine::
    /// compositor` (see their doc comments) must agree with each other.
    #[test]
    fn expected_frame_len_matches_comp_dimensions() {
        assert_eq!(EXPECTED_FRAME_LEN, (COMP_W as usize) * (COMP_H as usize) * 4);
    }

    /// Structural check only: confirms `Command::new(
    /// "ffmpeg")...spawn()` itself never panics, whether or not the target
    /// device path is real. No v4l2loopback device exists on this machine
    /// (see `find_device_returns_none...` above), so ffmpeg will fail once
    /// it reaches the v4l2 muxer: that's expected and not asserted on
    /// here, only that spawning + a liveness check don't panic.
    #[test]
    fn ffmpeg_pipe_spawn_does_not_panic_regardless_of_device_validity() {
        let result = FfmpegPipe::spawn(Path::new("/dev/opendrop-test-nonexistent"));
        if let Ok(mut pipe) = result {
            let _ = pipe.is_alive();
        }
        // else: ffmpeg not found on PATH in this environment, also not a panic.
    }
}
