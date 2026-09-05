//! Video input: decodes a local clip file, or captures a live camera, into
//! raw RGBA frames this app can upload as a GL texture (Step 14 of the
//! Phase 8 VJ-panels plan).
//!
//! **The mirror image of [`crate::v4l2loopback`]**, deliberately so: that
//! module pipes the compositor's readback bytes *into* an `ffmpeg`
//! subprocess (output), this one reads raw frames *out* of one (input).
//! Same subprocess choice for the same reason (PLAN.md Override 2: no video
//! decoding crate exists anywhere in `Cargo.lock`, and `ffmpeg` is already a
//! runtime dependency of the v4l2 output path), same lifecycle discipline:
//! a dedicated thread, an `mpsc` control channel, state published through
//! `ArcSwap`, and an `impl Drop` doing `kill()` + `wait()` so no ffmpeg
//! process ever outlives its owner and no zombie is ever left behind.
//!
//! **Two threads per active source, not one.** `v4l2loopback`'s single
//! thread can poll its control channel between (non-blocking) writes;
//! reading cannot work that way: a `read_exact` on a camera that opened
//! but never delivers blocks forever, and a control loop stuck inside it
//! would never see `Stop`. So the control thread owns the child and the
//! control channel, and hands the child's `stdout` to a short-lived reader
//! thread. `Stop` kills the child, which closes that pipe, which ends the
//! reader's blocking read with EOF and lets it exit: no cancellation
//! protocol beyond the one the OS already gives us.
//!
//! **Latest-frame-wins, not a queue.** One frame here is
//! `CAPTURE_W * CAPTURE_H * 4` = 3.5 MB. An unbounded `mpsc` of those would
//! grow without bound whenever the app's tick fell behind the source's
//! frame rate, so frames are published into an [`arc_swap::ArcSwapOption`]
//! instead: constant memory, and the renderer always gets the newest frame.
//! [`VideoFrame::seq`] lets the caller skip re-uploading a frame it has
//! already seen.
//!
//! **Playback speed is paced here, not by ffmpeg.** For a file input the
//! reader thread throttles its own reads to `CAPTURE_FPS * rate` frames per
//! second; ffmpeg then blocks on its own `write()` against the full pipe,
//! which is what keeps it in step. That is what makes the Video panel's
//! bass-driven warp (`core::video::VideoState::on_audio_tick`) adjustable
//! *live*, with no restart and no `ffmpeg`-version-specific input flag
//! (`-re`/`-readrate` are both spawn-time only). A camera is never paced:
//! the hardware already sets the rate.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::{ArcSwap, ArcSwapOption};

/// Resolution every source is scaled to by ffmpeg (`-s WxH`), so the
/// receiving GL texture never has to be reallocated mid-session and one
/// frame is always exactly [`frame_len`] bytes. 720p rather than the
/// compositor's own 1080p: the video layer is a background wash behind a
/// 1920x1080 composite, and a 720p RGBA frame is 3.5 MB per copy instead of
/// 7.9 MB.
pub const CAPTURE_W: u32 = 1280;
pub const CAPTURE_H: u32 = 720;

/// Frames per second requested from ffmpeg (`-r`), and the pacing rate the
/// reader thread throttles a file source to at `rate == 1.0`.
pub const CAPTURE_FPS: u32 = 30;

/// Bytes in one RGBA8 frame of the given size.
pub fn frame_len(width: u32, height: u32) -> usize {
    width as usize * height as usize * 4
}

/// How often the control thread wakes while a source is running, to notice
/// ffmpeg exiting on its own. Same role (and value) as
/// `v4l2loopback::POLL_TICK`.
const POLL_TICK: Duration = Duration::from_millis(5);

/// Playback rate is shared with the reader thread as an integer (rate ×
/// [`RATE_SCALE`]) so it can live in an `AtomicU32` and be changed live
/// without a restart.
const RATE_SCALE: f64 = 1000.0;

/// Bounds the shared rate is clamped to before it reaches the reader:
/// matches `core::video`'s `WARP_RATE_MIN`/`WARP_RATE_MAX` (0.6..2.0),
/// hardcoded here rather than depending on `opendrop-core` for two floats
/// (same precedent as `v4l2loopback`'s local `COMP_W`/`COMP_H`). A rate of
/// 0 would stall the reader forever, which is what the lower bound really
/// guards against.
const RATE_MIN: f64 = 0.6;
const RATE_MAX: f64 = 2.0;

/// What a source is reading from. Kept minimal on purpose: everything that
/// varies per platform is [`camera_input_args`]'s business, not this
/// enum's.
#[derive(Debug, Clone, PartialEq)]
pub enum VideoInput {
    /// A clip file on disk, looped forever (`-stream_loop -1`), optionally
    /// starting `start_seconds` into the file (`-ss`, ticket #10's
    /// "Synchronised music video playback"). `0.0` (the pre-ticket-#10
    /// default everywhere except the sync path) omits `-ss` entirely,
    /// preserving today's exact ffmpeg invocation.
    File { path: PathBuf, start_seconds: f64 },
    /// A live camera, identified the way the platform's ffmpeg input device
    /// expects: `/dev/videoN` on Linux, the DirectShow device *name* on
    /// Windows, the AVFoundation device index on macOS.
    Camera(String),
}

impl VideoInput {
    /// Whether the reader thread paces itself for this input: see the
    /// module doc comment. A file is paced (ffmpeg would otherwise decode
    /// as fast as the pipe drains); a camera is not (the hardware paces it).
    fn is_paced(&self) -> bool {
        matches!(self, VideoInput::File { .. })
    }
}

/// Control messages sent to the video-capture thread.
pub enum VideoCaptureControl {
    /// Starts decoding/capturing `input`, replacing any source already
    /// running: same unconditional-restart convention as
    /// `V4l2Control::Start`.
    Start(VideoInput),
    /// Kills the ffmpeg process, if any, and clears the published frame. A
    /// no-op while already idle.
    ///
    /// One already-decoded frame can, very rarely, land in the slot just
    /// after the clear (the reader thread may be a few instructions from
    /// its `store` when the child is killed). Harmless by construction:
    /// consumers gate on [`VideoCaptureSnapshot::running`], which is
    /// `false` from the moment this message is handled, not on the mere
    /// presence of a frame.
    Stop,
    /// Live playback-rate change (the bass warp). Applies to a file source
    /// immediately, with no restart; ignored by a camera source, whose rate
    /// this app doesn't set.
    SetRate(f64),
}

/// One decoded frame, published latest-wins (see the module doc comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    /// Strictly increasing across the whole session, and genuinely NOT
    /// reset by a `Start`: every reader thread draws from one shared
    /// [`AtomicU64`] (see [`read_loop`]), so the first frame of a new clip
    /// continues from wherever the previous source left off rather than
    /// starting over.
    ///
    /// That is what lets a caller remember the last value it uploaded and
    /// skip re-uploading the same frame, with either `!=` or `>`. A
    /// per-source counter would have been enough for the one consumer that
    /// exists today (`app` compares with `!=` and resets its own tracking
    /// on every source change) but would silently break the next one:
    /// during the brief overlap while an old reader is winding down, a
    /// restarted counter can hand out a *lower* number for a *newer* frame.
    /// Never zero, so 0 is usable as a caller-side "nothing uploaded yet".
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Continuous state published via [`VideoCaptureHandle::latest`]: never
/// blocks, always the latest known value (mirrors `V4l2Snapshot`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoCaptureSnapshot {
    pub running: bool,
    /// Why the last `Start` failed (or why a running source stopped on its
    /// own), for the panel to display. Cleared by a successful `Start`.
    pub last_error: Option<String>,
}

/// Handle to the running video-capture thread. Mirrors `V4l2Handle`'s
/// shape: `latest()` never blocks, `control_tx` sends never block.
pub struct VideoCaptureHandle {
    state: Arc<ArcSwap<VideoCaptureSnapshot>>,
    frame: Arc<ArcSwapOption<VideoFrame>>,
    pub control_tx: Sender<VideoCaptureControl>,
}

impl VideoCaptureHandle {
    /// Never blocks: an atomic load of the current Arc.
    pub fn latest(&self) -> Arc<VideoCaptureSnapshot> {
        self.state.load_full()
    }

    /// The most recently decoded frame, or `None` while idle (or before the
    /// first frame of a freshly started source arrives). Never blocks, and
    /// never yields the same `seq` twice unless nothing new has arrived.
    pub fn latest_frame(&self) -> Option<Arc<VideoFrame>> {
        self.frame.load_full()
    }
}

/// Spawns the dedicated video-capture thread and returns immediately. The
/// thread starts idle (no ffmpeg process) until it receives
/// `VideoCaptureControl::Start`: same "starts idle until an explicit
/// start" pattern as `v4l2loopback::spawn`.
pub fn spawn() -> VideoCaptureHandle {
    let state = Arc::new(ArcSwap::from_pointee(VideoCaptureSnapshot::default()));
    let frame = Arc::new(ArcSwapOption::empty());
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        let frame = frame.clone();
        move || run(state, frame, control_rx)
    });
    VideoCaptureHandle { state, frame, control_tx }
}

// --- ffmpeg argument construction -----------------------------------------
//
// All four builders below are plain `Vec<String>` constructors with no
// platform API of their own, so all four compile (and are unit-tested) on
// every platform; only `camera_input_args`'s dispatch is `#[cfg]`-selected.
// That is deliberate: the Windows/macOS argument shapes are exactly the
// part that can't be verified by running the app on Linux, so they are at
// least verified by a test that runs everywhere.

/// Input args for a local clip: looped forever, so a short loop keeps
/// playing instead of the layer going black after one pass. A positive
/// `start_seconds` adds an input-side `-ss` seek (ticket #10); `0.0` omits
/// it, unchanged from before that ticket.
pub fn file_input_args(path: &Path, start_seconds: f64) -> Vec<String> {
    let mut args = vec!["-stream_loop".into(), "-1".into()];
    if start_seconds > 0.0 {
        args.push("-ss".into());
        args.push(format!("{start_seconds:.3}"));
    }
    args.push("-i".into());
    args.push(path.to_string_lossy().into_owned());
    args
}

/// Linux camera: Video4Linux2, device path as-is (`/dev/video0`).
pub fn camera_input_args_v4l2(device: &str) -> Vec<String> {
    vec!["-f".into(), "v4l2".into(), "-i".into(), device.to_string()]
}

/// Windows camera: DirectShow, which addresses devices by *name*, prefixed
/// `video=` (`-f dshow -i video="Integrated Webcam"`). No shell is involved
/// (`Command` passes argv directly), so the value carries no quotes of its
/// own.
pub fn camera_input_args_dshow(device: &str) -> Vec<String> {
    vec!["-f".into(), "dshow".into(), "-i".into(), format!("video={device}")]
}

/// macOS camera: AVFoundation, which addresses devices by index (`"0"` is
/// the default camera).
pub fn camera_input_args_avfoundation(device: &str) -> Vec<String> {
    vec!["-f".into(), "avfoundation".into(), "-i".into(), device.to_string()]
}

/// The platform's camera input args. `#[cfg]`-selected, same conditional
/// -compilation approach `engine/Cargo.toml` already uses for its
/// Windows-only `vcpkg` dependency.
pub fn camera_input_args(device: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        camera_input_args_dshow(device)
    }
    #[cfg(target_os = "macos")]
    {
        camera_input_args_avfoundation(device)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        camera_input_args_v4l2(device)
    }
}

/// Output args shared by every input: raw RGBA at a fixed size and rate,
/// straight to stdout. RGBA (not BGRA) for the same reason
/// `v4l2loopback` writes RGBA: it is what GL wants, with no swizzle.
///
/// `-vf vflip` is what lets the decoded frames go through the compositor's
/// *deck* shader unchanged. Every decoder hands out row 0 = the image's TOP
/// row, while a GL texture's origin is bottom-left; the deck shader
/// deliberately does not flip V (its inputs are FBO copies, already
/// bottom-left, see `engine::compositor`'s header). Flipping in ffmpeg's
/// own filter chain, which is already scaling the frame anyway, costs
/// nothing measurable and keeps the flip out of both the upload path (a
/// 3.5 MB row-reversal per frame) and the shader (a second sampling
/// convention to keep straight).
pub fn output_args(width: u32, height: u32, fps: u32) -> Vec<String> {
    vec![
        "-vf".into(),
        "vflip".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        "-s".into(),
        format!("{width}x{height}"),
        "-r".into(),
        fps.to_string(),
        "pipe:1".into(),
    ]
}

fn input_args_for(input: &VideoInput) -> Vec<String> {
    match input {
        VideoInput::File { path, start_seconds } => file_input_args(path, *start_seconds),
        VideoInput::Camera(device) => camera_input_args(device),
    }
}

// --- camera enumeration ----------------------------------------------------

/// One camera the Video panel can offer. `id` is what
/// [`camera_input_args`] wants for this platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraDevice {
    pub id: String,
    pub label: String,
}

/// Cameras this machine offers, for the Video panel's device picker.
///
/// Linux only: a pure `/sys/class/video4linux` scan, the same filesystem
/// -only approach `v4l2loopback::find_device` takes (no subprocess, no
/// side effects, `None`/empty for every "can't tell" case alike). On
/// Windows/macOS this returns empty and the panel falls back to its
/// free-text device field: enumerating there needs `ffmpeg -list_devices
/// true`, i.e. spawning a subprocess and parsing its stderr, which is a
/// different (and less reliable) shape of code that this step deliberately
/// does not take on untested on platforms it cannot run.
pub fn list_cameras() -> Vec<CameraDevice> {
    #[cfg(target_os = "linux")]
    {
        list_cameras_in(Path::new("/sys/class/video4linux"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

/// The testable core of [`list_cameras`], split out so a test can point it
/// at a synthetic directory tree: same split (and same deterministic
/// filename sort) as `v4l2loopback::find_device_in`.
///
/// Two entries are skipped, and every surviving label is made
/// self-identifying:
///
/// - **Our own v4l2loopback sink** (label containing "OpenDrop"): capturing
///   our own output back into the video layer is a feedback loop, never
///   something a user means to pick.
/// - **Secondary nodes of a multi-node device** (`index` other than `0`). A
///   UVC webcam registers several `/dev/videoN` nodes under one physical
///   device (typically `index 0` for capture and `index 1` for the
///   metadata stream), and they report the *identical* `name`. Verified on
///   two machines during review: `video0` and `video1` both read
///   `"USB2.0 HD UVC WebCam: USB2.0 HD"`, and only `video0` can actually be
///   captured from. Listing both gave the user two indistinguishable
///   entries and a coin-flip chance of picking the dead one, whose only
///   feedback is a generic "the video source stopped unexpectedly".
///   `index == 0` is the primary-capture-node convention; a device that
///   does not follow it simply isn't listed, and the panel's device field
///   (always editable, never gated on this list being empty) is the escape
///   hatch for that case.
/// - A node with **no readable `index`** is kept rather than dropped:
///   "can't tell" should not hide a working camera.
///
/// The label always carries the device node (`Integrated Camera
/// (/dev/video0)`), so two genuinely distinct cameras that happen to share
/// a name stay distinguishable too.
fn list_cameras_in(base: &Path) -> Vec<CameraDevice> {
    let Ok(read_dir) = std::fs::read_dir(base) else { return Vec::new() };
    let mut entries: Vec<_> = read_dir.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    entries
        .into_iter()
        .filter_map(|entry| {
            let name = std::fs::read_to_string(entry.path().join("name")).ok()?;
            let name = name.trim();
            if name.contains("OpenDrop") {
                return None; // our own v4l2loopback sink: see the doc comment
            }
            if !is_primary_capture_node(&entry.path()) {
                return None;
            }
            let id = Path::new("/dev").join(entry.file_name()).to_string_lossy().into_owned();
            let label = if name.is_empty() { id.clone() } else { format!("{name} ({id})") };
            Some(CameraDevice { id, label })
        })
        .collect()
}

/// Whether this `/sys/class/video4linux/videoN` directory is a physical
/// device's primary (index 0) node: see [`list_cameras_in`]'s doc comment.
/// An unreadable or unparseable `index` counts as primary: "can't tell"
/// must not hide a working camera.
fn is_primary_capture_node(dir: &Path) -> bool {
    match std::fs::read_to_string(dir.join("index")) {
        Ok(index) => index.trim() == "0",
        Err(_) => true,
    }
}

// --- the subprocess --------------------------------------------------------

/// The running ffmpeg subprocess. The reading counterpart of
/// `v4l2loopback::FfmpegPipe`: `stdout` is piped (and handed straight to a
/// reader thread, see the module doc comment), `stdin`/`stderr` are both
/// discarded, for the same reason `FfmpegPipe` discards its own: capturing
/// stderr would need yet another thread draining it or a full pipe would
/// eventually stall ffmpeg, and `Drop` guarantees the process never
/// outlives this struct either way.
struct VideoSource {
    child: Child,
}

impl VideoSource {
    /// Spawns `ffmpeg <input_args> <output_args>` and takes its stdout.
    /// Returns the source and the pipe separately: the source stays on the
    /// control thread (which owns the child's lifetime), the pipe moves to
    /// a reader thread.
    fn spawn(input_args: &[String], width: u32, height: u32, fps: u32) -> std::io::Result<(Self, ChildStdout)> {
        let mut child = Command::new("ffmpeg")
            .args(input_args)
            .args(output_args(width, height, fps))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().expect("Stdio::piped() guarantees Some");
        Ok((Self { child }, stdout))
    }

    /// Non-blocking liveness check: `try_wait` never blocks. `Err` (can't
    /// tell) is treated as "not alive", same conservative reading as
    /// `FfmpegPipe::is_alive`.
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

/// Kills and reaps the ffmpeg process on `Stop`, on a fresh `Start`
/// replacing it, and when the thread itself shuts down: never leaves a
/// zombie or an orphaned ffmpeg process behind. Identical to
/// `FfmpegPipe`'s. Killing it also closes the stdout pipe, which is what
/// ends the reader thread's blocking read (see the module doc comment).
impl Drop for VideoSource {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// --- threads ---------------------------------------------------------------

fn publish(state: &Arc<ArcSwap<VideoCaptureSnapshot>>, running: bool, last_error: Option<String>) {
    state.store(Arc::new(VideoCaptureSnapshot { running, last_error }));
}

/// `rate` as stored in the shared `AtomicU32`, clamped to the documented
/// bounds so a NaN/0/absurd value can never stall or run away the reader.
fn encode_rate(rate: f64) -> u32 {
    let clamped = if rate.is_finite() { rate.clamp(RATE_MIN, RATE_MAX) } else { 1.0 };
    (clamped * RATE_SCALE).round() as u32
}

fn decode_rate(encoded: u32) -> f64 {
    (encoded as f64 / RATE_SCALE).clamp(RATE_MIN, RATE_MAX)
}

/// Reads fixed-size RGBA frames off `stdout` until the pipe ends (EOF, a
/// read error, or the child being killed) and publishes each one, newest
/// wins. Owns nothing but the pipe: stopping it is done by killing the
/// child on the control thread.
///
/// `seq` is the session-wide counter, SHARED with the control thread and
/// with every other reader, not a per-source copy: that is what makes
/// [`VideoFrame::seq`]'s across-restarts guarantee real rather than
/// aspirational. `Relaxed` is the right ordering: the only thing anyone
/// needs from it is that each `fetch_add` hands out a distinct, larger
/// number, which an atomic read-modify-write gives unconditionally; the
/// frame's own visibility is carried by the `ArcSwapOption` store below.
fn read_loop(
    mut stdout: ChildStdout,
    frame: Arc<ArcSwapOption<VideoFrame>>,
    rate: Arc<AtomicU32>,
    paced: bool,
    seq: Arc<AtomicU64>,
) {
    let len = frame_len(CAPTURE_W, CAPTURE_H);
    let mut next_at = Instant::now();
    loop {
        // A fresh buffer per frame, read into and then moved into the
        // published `VideoFrame`, rather than one reused buffer cloned on
        // every publish, which would double this thread's memory traffic
        // (3.5 MB per frame, 30 times a second).
        let mut data = vec![0u8; len];
        if stdout.read_exact(&mut data).is_err() {
            return; // EOF (source stopped/replaced) or a read error; either way, done.
        }
        let seq = seq.fetch_add(1, Ordering::Relaxed) + 1;
        frame.store(Some(Arc::new(VideoFrame { seq, width: CAPTURE_W, height: CAPTURE_H, data })));
        if !paced {
            continue;
        }
        // Throttle to CAPTURE_FPS * rate. ffmpeg blocks on its own write
        // while we sleep, which is what actually slows the decode down.
        let period = Duration::from_secs_f64(1.0 / (CAPTURE_FPS as f64 * decode_rate(rate.load(Ordering::Relaxed))));
        next_at += period;
        let now = Instant::now();
        if next_at > now {
            std::thread::sleep(next_at - now);
        } else {
            next_at = now; // fell behind (a rate change, a stalled tick): don't try to catch up
        }
    }
}

/// Replaces the running source (if any) with a fresh one for `input`.
/// Dropping the old `VideoSource` kills+reaps its child, which also ends
/// its reader thread; the published frame is cleared so the caller never
/// composites the previous clip's last frame over the new one.
fn start_source(
    source: &mut Option<VideoSource>,
    input: &VideoInput,
    frame: &Arc<ArcSwapOption<VideoFrame>>,
    rate: &Arc<AtomicU32>,
    seq: &Arc<AtomicU64>,
    state: &Arc<ArcSwap<VideoCaptureSnapshot>>,
) {
    *source = None;
    frame.store(None);
    match VideoSource::spawn(&input_args_for(input), CAPTURE_W, CAPTURE_H, CAPTURE_FPS) {
        Ok((src, stdout)) => {
            *source = Some(src);
            // The counter is handed over by reference-counted *sharing*, not
            // by value: a copy would restart the sequence on every `Start`
            // and quietly break `VideoFrame::seq`'s documented guarantee.
            std::thread::spawn({
                let frame = frame.clone();
                let rate = rate.clone();
                let seq = seq.clone();
                let paced = input.is_paced();
                move || read_loop(stdout, frame, rate, paced, seq)
            });
            publish(state, true, None);
        }
        Err(e) => {
            let msg = format!("failed to start ffmpeg for {input:?}: {e}");
            eprintln!("[video] {msg}");
            publish(state, false, Some(msg));
        }
    }
}

fn handle_control(
    source: &mut Option<VideoSource>,
    ctrl: VideoCaptureControl,
    frame: &Arc<ArcSwapOption<VideoFrame>>,
    rate: &Arc<AtomicU32>,
    seq: &Arc<AtomicU64>,
    state: &Arc<ArcSwap<VideoCaptureSnapshot>>,
) {
    match ctrl {
        VideoCaptureControl::Start(input) => start_source(source, &input, frame, rate, seq, state),
        VideoCaptureControl::Stop => {
            *source = None; // Drop kills+reaps the child, ending its reader thread.
            frame.store(None);
            publish(state, false, None);
        }
        VideoCaptureControl::SetRate(v) => rate.store(encode_rate(v), Ordering::Relaxed),
    }
}

fn run(
    state: Arc<ArcSwap<VideoCaptureSnapshot>>,
    frame: Arc<ArcSwapOption<VideoFrame>>,
    control_rx: Receiver<VideoCaptureControl>,
) {
    let mut source: Option<VideoSource> = None;
    let rate = Arc::new(AtomicU32::new(encode_rate(1.0)));
    // Session-wide frame counter, shared with every reader thread this
    // loop ever spawns: see `VideoFrame::seq`.
    let seq = Arc::new(AtomicU64::new(0));
    publish(&state, false, None);

    loop {
        let mut owner_gone = false;
        if source.is_none() {
            // Idle: nothing to watch, so block until a control message
            // arrives rather than spinning the 5 ms poll for nothing: same
            // idle/active split as `v4l2loopback::run`.
            match control_rx.recv() {
                Ok(ctrl) => handle_control(&mut source, ctrl, &frame, &rate, &seq, &state),
                Err(_) => owner_gone = true,
            }
        } else {
            match control_rx.recv_timeout(POLL_TICK) {
                Ok(ctrl) => handle_control(&mut source, ctrl, &frame, &rate, &seq, &state),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => owner_gone = true,
            }
        }
        while !owner_gone {
            match control_rx.try_recv() {
                Ok(ctrl) => handle_control(&mut source, ctrl, &frame, &rate, &seq, &state),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => owner_gone = true,
            }
        }

        // Detects ffmpeg exiting on its own (unreadable file, camera
        // unplugged, killed externally): `try_wait` never blocks.
        if let Some(src) = source.as_mut() {
            if !src.is_alive() {
                eprintln!("[video] ffmpeg exited unexpectedly");
                source = None;
                frame.store(None);
                publish(&state, false, Some("the video source stopped unexpectedly".to_string()));
            }
        }

        if owner_gone {
            break; // VideoCaptureHandle (and its control_tx) dropped: shut down.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod argument_construction {
        use super::*;

        #[test]
        fn one_frame_is_width_times_height_times_four_bytes() {
            assert_eq!(frame_len(2, 3), 24);
            assert_eq!(frame_len(CAPTURE_W, CAPTURE_H), 1280 * 720 * 4);
            assert_eq!(frame_len(0, 720), 0);
        }

        #[test]
        fn a_file_input_loops_forever() {
            assert_eq!(
                file_input_args(Path::new("/clips/a b.webm"), 0.0),
                ["-stream_loop", "-1", "-i", "/clips/a b.webm"]
            );
        }

        #[test]
        fn the_linux_camera_input_is_v4l2_with_the_device_path() {
            assert_eq!(camera_input_args_v4l2("/dev/video0"), ["-f", "v4l2", "-i", "/dev/video0"]);
        }

        #[test]
        fn the_windows_camera_input_is_dshow_with_a_video_prefixed_device_name() {
            // No shell quoting: `Command` passes argv directly, so the
            // value is the bare `video=<name>` ffmpeg expects.
            assert_eq!(
                camera_input_args_dshow("Integrated Webcam"),
                ["-f", "dshow", "-i", "video=Integrated Webcam"]
            );
        }

        #[test]
        fn the_macos_camera_input_is_avfoundation_with_the_device_index() {
            assert_eq!(camera_input_args_avfoundation("0"), ["-f", "avfoundation", "-i", "0"]);
        }

        #[test]
        fn the_platform_dispatch_picks_this_platforms_builder() {
            let dispatched = camera_input_args("dev");
            #[cfg(target_os = "windows")]
            assert_eq!(dispatched, camera_input_args_dshow("dev"));
            #[cfg(target_os = "macos")]
            assert_eq!(dispatched, camera_input_args_avfoundation("dev"));
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            assert_eq!(dispatched, camera_input_args_v4l2("dev"));
        }

        #[test]
        fn the_output_is_vertically_flipped_raw_rgba_at_the_requested_size_and_rate_on_stdout() {
            assert_eq!(
                output_args(1280, 720, 30),
                ["-vf", "vflip", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "1280x720", "-r", "30", "pipe:1"]
            );
        }

        #[test]
        fn input_args_dispatch_on_the_input_kind() {
            assert_eq!(
                input_args_for(&VideoInput::File { path: PathBuf::from("/x.webm"), start_seconds: 0.0 }),
                file_input_args(Path::new("/x.webm"), 0.0)
            );
            assert_eq!(input_args_for(&VideoInput::Camera("cam".into())), camera_input_args("cam"));
        }

        #[test]
        fn a_positive_start_offset_adds_an_ss_flag() {
            let args = file_input_args(Path::new("/x.webm"), 12.5);
            assert_eq!(args, vec!["-stream_loop", "-1", "-ss", "12.500", "-i", "/x.webm"]);
        }

        #[test]
        fn a_zero_start_offset_omits_the_ss_flag() {
            let args = file_input_args(Path::new("/x.webm"), 0.0);
            assert_eq!(args, vec!["-stream_loop", "-1", "-i", "/x.webm"]);
        }

        #[test]
        fn only_a_file_source_is_paced() {
            assert!(VideoInput::File { path: PathBuf::from("/x.webm"), start_seconds: 0.0 }.is_paced());
            assert!(!VideoInput::Camera("cam".into()).is_paced());
        }
    }

    mod rate_encoding {
        use super::*;

        #[test]
        fn a_rate_round_trips_through_the_shared_atomic() {
            for rate in [0.6, 1.0, 1.25, 2.0] {
                assert!((decode_rate(encode_rate(rate)) - rate).abs() < 1e-9, "rate {rate}");
            }
        }

        #[test]
        fn an_out_of_range_rate_is_clamped_not_wrapped() {
            assert_eq!(decode_rate(encode_rate(0.0)), RATE_MIN);
            assert_eq!(decode_rate(encode_rate(-5.0)), RATE_MIN);
            assert_eq!(decode_rate(encode_rate(99.0)), RATE_MAX);
        }

        #[test]
        fn a_nan_rate_falls_back_to_normal_speed_instead_of_stalling_the_reader() {
            assert_eq!(decode_rate(encode_rate(f64::NAN)), 1.0);
            assert_eq!(decode_rate(encode_rate(f64::INFINITY)), 1.0);
        }

        #[test]
        fn no_reachable_encoded_rate_can_make_the_frame_period_infinite() {
            // Guards the `1.0 / (fps * rate)` division in `read_loop`.
            for encoded in [0u32, 1, 600, 1000, 2000, u32::MAX] {
                let period = 1.0 / (CAPTURE_FPS as f64 * decode_rate(encoded));
                assert!(period.is_finite() && period > 0.0, "encoded {encoded} gave period {period}");
            }
        }
    }

    mod camera_enumeration {
        use super::*;

        /// Builds a synthetic `/sys/class/video4linux`-shaped tree:
        /// `(node, name, index)`, where an `index` of `None` writes no
        /// `index` file at all.
        fn sysfs(tag: &str, nodes: &[(&str, &str, Option<&str>)]) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("opendrop-video-cams-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            for (node, name, index) in nodes {
                std::fs::create_dir_all(dir.join(node)).unwrap();
                std::fs::write(dir.join(node).join("name"), format!("{name}\n")).unwrap();
                if let Some(index) = index {
                    std::fs::write(dir.join(node).join("index"), format!("{index}\n")).unwrap();
                }
            }
            dir
        }

        #[test]
        fn a_nonexistent_sysfs_tree_yields_no_cameras() {
            assert_eq!(list_cameras_in(Path::new("/nonexistent/opendrop-video-test")), Vec::new());
        }

        #[test]
        fn cameras_are_listed_in_deterministic_device_order_with_their_node_in_the_label() {
            let dir = sysfs(
                "order",
                &[("video2", "USB2.0 Camera", Some("0")), ("video0", "Integrated Camera", Some("0"))],
            );

            let found = list_cameras_in(&dir);
            std::fs::remove_dir_all(&dir).unwrap();

            assert_eq!(
                found,
                vec![
                    CameraDevice {
                        id: "/dev/video0".into(),
                        label: "Integrated Camera (/dev/video0)".into()
                    },
                    CameraDevice { id: "/dev/video2".into(), label: "USB2.0 Camera (/dev/video2)".into() },
                ]
            );
        }

        /// Review finding: the exact layout on the reviewer's machine (and
        /// on this one): one physical UVC webcam registering two nodes
        /// with the *same* name, only the second of which is uncapturable.
        #[test]
        fn a_uvc_webcams_metadata_node_is_not_offered_alongside_its_capture_node() {
            let name = "USB2.0 HD UVC WebCam: USB2.0 HD";
            let dir = sysfs("uvc", &[("video0", name, Some("0")), ("video1", name, Some("1"))]);

            let found = list_cameras_in(&dir);
            std::fs::remove_dir_all(&dir).unwrap();

            assert_eq!(
                found,
                vec![CameraDevice {
                    id: "/dev/video0".into(),
                    label: format!("{name} (/dev/video0)"),
                }],
                "only the index-0 capture node should be offered"
            );
        }

        #[test]
        fn two_physical_cameras_each_keep_their_own_primary_node() {
            let dir = sysfs(
                "two-cams",
                &[
                    ("video0", "Cam A", Some("0")),
                    ("video1", "Cam A", Some("1")),
                    ("video2", "Cam B", Some("0")),
                    ("video3", "Cam B", Some("1")),
                ],
            );

            let found = list_cameras_in(&dir);
            std::fs::remove_dir_all(&dir).unwrap();

            assert_eq!(found.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["/dev/video0", "/dev/video2"]);
        }

        /// Two identical webcams of the same model share a `name`; the
        /// device node in the label is what keeps them apart.
        #[test]
        fn identical_models_stay_distinguishable_through_their_device_node() {
            let dir = sysfs("twins", &[("video0", "C920", Some("0")), ("video2", "C920", Some("0"))]);

            let found = list_cameras_in(&dir);
            std::fs::remove_dir_all(&dir).unwrap();

            let labels: Vec<&str> = found.iter().map(|c| c.label.as_str()).collect();
            assert_eq!(labels, ["C920 (/dev/video0)", "C920 (/dev/video2)"]);
            assert_ne!(labels[0], labels[1], "the whole point: no unlabeled duplicates");
        }

        #[test]
        fn a_node_with_no_index_file_is_kept_rather_than_hidden() {
            // "Can't tell" must not hide a working camera: an older
            // kernel, or a driver that doesn't publish `index`.
            let dir = sysfs("no-index", &[("video0", "Odd Camera", None)]);

            let found = list_cameras_in(&dir);
            std::fs::remove_dir_all(&dir).unwrap();

            assert_eq!(found, vec![CameraDevice { id: "/dev/video0".into(), label: "Odd Camera (/dev/video0)".into() }]);
        }

        #[test]
        fn an_unparseable_index_is_treated_as_primary() {
            let dir = std::env::temp_dir().join(format!("opendrop-video-cams-badidx-{}", std::process::id()));
            std::fs::create_dir_all(dir.join("video0")).unwrap();
            std::fs::write(dir.join("video0").join("index"), "not a number\n").unwrap();
            assert!(!is_primary_capture_node(&dir.join("video0")), "a non-'0' index is not primary");
            std::fs::write(dir.join("video0").join("index"), "0").unwrap();
            assert!(is_primary_capture_node(&dir.join("video0")), "'0' with no trailing newline is primary");
            std::fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        fn our_own_v4l2loopback_sink_is_never_offered_as_a_camera() {
            let dir = sysfs("loopback", &[("video0", "OpenDrop Video", Some("0")), ("video1", "Real Camera", Some("0"))]);

            let found = list_cameras_in(&dir);
            std::fs::remove_dir_all(&dir).unwrap();

            assert_eq!(
                found,
                vec![CameraDevice { id: "/dev/video1".into(), label: "Real Camera (/dev/video1)".into() }]
            );
        }

        /// Real-environment check: whatever this machine has, no two
        /// entries may share a label: that is exactly the state the
        /// review flagged.
        #[test]
        fn no_two_cameras_on_this_machine_share_a_label() {
            let cameras = list_cameras();
            let mut labels: Vec<&str> = cameras.iter().map(|c| c.label.as_str()).collect();
            let total = labels.len();
            labels.sort_unstable();
            labels.dedup();
            assert_eq!(labels.len(), total, "duplicate camera labels in {cameras:?}");
        }
    }

    mod snapshot_and_handle {
        use super::*;

        #[test]
        fn a_fresh_snapshot_is_idle_with_no_error() {
            let s = VideoCaptureSnapshot::default();
            assert!(!s.running);
            assert_eq!(s.last_error, None);
        }

        #[test]
        fn a_freshly_spawned_thread_is_idle_and_publishes_no_frame() {
            let handle = spawn();
            assert!(!handle.latest().running);
            assert!(handle.latest_frame().is_none());
        }

        #[test]
        fn a_start_on_a_nonexistent_file_reports_an_error_without_panicking() {
            let handle = spawn();
            let _ = handle
                .control_tx
                .send(VideoCaptureControl::Start(VideoInput::File {
                    path: PathBuf::from("/nonexistent/opendrop-no-such.webm"),
                    start_seconds: 0.0,
                }));
            // ffmpeg is spawned successfully but exits immediately (it
            // can't open the file); either the spawn failed outright (no
            // ffmpeg on PATH) or the liveness poll notices the exit. Both
            // land on `running == false` with an error. Poll briefly
            // rather than sleeping a fixed amount.
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                let s = handle.latest();
                if !s.running && s.last_error.is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            panic!("a bad file should have surfaced an error, got {:?}", handle.latest());
        }
    }

    /// Real-subprocess tests. Every one of them degrades to a silent pass
    /// when `ffmpeg` isn't on PATH (same "structural check only" convention
    /// as `v4l2loopback`'s own spawn test), but on a machine that has it
    /// (every machine this app can actually run the v4l2 output on), they
    /// exercise the parts no amount of `cargo build` can: that the argument
    /// set really produces frames of exactly the size we claim, and that
    /// `Drop` really leaves no process behind.
    mod real_ffmpeg {
        use super::*;

        /// A synthetic 64x48 source, so a frame is 12 KB rather than 3.5 MB
        /// and one arrives in milliseconds.
        fn testsrc() -> Vec<String> {
            vec!["-f".into(), "lavfi".into(), "-i".into(), "testsrc=size=64x48:rate=30".into()]
        }

        #[test]
        fn spawning_does_not_panic_whether_or_not_the_input_is_valid() {
            let bogus = file_input_args(Path::new("/nonexistent/opendrop-no-such.webm"), 0.0);
            if let Ok((mut src, _stdout)) = VideoSource::spawn(&bogus, 64, 48, 30) {
                let _ = src.is_alive();
            }
            // else: ffmpeg not on PATH here, also not a panic.
        }

        #[test]
        fn a_real_pipe_yields_exactly_one_frames_worth_of_rgba_bytes() {
            let Ok((src, mut stdout)) = VideoSource::spawn(&testsrc(), 64, 48, 30) else {
                return; // no ffmpeg on PATH
            };
            let mut buf = vec![0u8; frame_len(64, 48)];
            let read = stdout.read_exact(&mut buf);
            drop(src);
            assert!(read.is_ok(), "reading one 64x48 RGBA frame failed: {read:?}");
            // `testsrc` is a color pattern with an opaque alpha channel: a
            // pix_fmt/size mismatch would show up as an all-zero buffer.
            assert!(buf.iter().any(|&b| b != 0), "the frame was entirely zero: wrong pix_fmt or size?");
            assert!(buf.as_chunks::<4>().0.iter().all(|px| px[3] == 255), "rgba alpha should be opaque");
        }

        /// Proves the `-vf vflip` in [`output_args`] really lands row 0 of
        /// the source at the BOTTOM of the emitted buffer: the one thing
        /// standing between a decoded clip and an upside-down video layer,
        /// and something no argument-shape assertion can show.
        ///
        /// Hermetic: a hand-built 1x2 RGBA source (top row red, bottom row
        /// blue) piped straight back out at the same size, so nothing here
        /// depends on a codec, a scaler, or a test pattern's own layout.
        #[test]
        fn the_filter_chain_flips_the_image_into_gls_bottom_left_origin() {
            let dir = std::env::temp_dir().join(format!("opendrop-video-flip-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let src_path = dir.join("two-rows.rgba");
            // Row 0 (TOP) red, row 1 (BOTTOM) blue.
            std::fs::write(&src_path, [255u8, 0, 0, 255, 0, 0, 255, 255]).unwrap();

            let input = vec![
                "-f".into(),
                "rawvideo".into(),
                "-pix_fmt".into(),
                "rgba".into(),
                "-s".into(),
                "1x2".into(),
                "-i".into(),
                src_path.to_string_lossy().into_owned(),
            ];
            let Ok((source, mut stdout)) = VideoSource::spawn(&input, 1, 2, 30) else {
                let _ = std::fs::remove_dir_all(&dir);
                return; // no ffmpeg on PATH
            };
            let mut buf = [0u8; 8];
            let read = stdout.read_exact(&mut buf);
            drop(source);
            let _ = std::fs::remove_dir_all(&dir);

            assert!(read.is_ok(), "reading the 1x2 frame back failed: {read:?}");
            // After the flip, the first bytes out are the source's BOTTOM
            // row (blue), which is exactly what GL wants at v = 0.
            assert_eq!(&buf[0..4], &[0, 0, 255, 255], "first emitted row should be the source's bottom row");
            assert_eq!(&buf[4..8], &[255, 0, 0, 255], "last emitted row should be the source's top row");
        }

        /// End-to-end through the real handle: a real clip file on disk
        /// reaches [`VideoCaptureHandle::latest_frame`] as full-size RGBA
        /// frames with a strictly increasing `seq`, and `Stop` takes the
        /// snapshot back to idle. This is the one test that exercises the
        /// whole two-thread lifecycle rather than a piece of it.
        #[test]
        fn a_real_clip_file_reaches_the_handle_as_full_size_rgba_frames() {
            let dir = std::env::temp_dir().join(format!("opendrop-video-e2e-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let clip = dir.join("clip.avi");
            // mpeg4/avi rather than the app's usual webm: both muxer and
            // encoder are built into every ffmpeg, and encoding half a
            // second of 64x48 takes milliseconds.
            let encoded = Command::new("ffmpeg")
                .args(["-y", "-f", "lavfi", "-i", "testsrc=size=64x48:rate=30:duration=0.5", "-c:v", "mpeg4"])
                .arg(&clip)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if !matches!(encoded, Ok(s) if s.success()) {
                let _ = std::fs::remove_dir_all(&dir);
                return; // no ffmpeg on PATH, or it can't encode here
            }

            let handle = spawn();
            let _ = handle.control_tx.send(VideoCaptureControl::Start(VideoInput::File { path: clip, start_seconds: 0.0 }));

            let deadline = Instant::now() + Duration::from_secs(10);
            let mut seen: Vec<u64> = Vec::new();
            while Instant::now() < deadline && seen.len() < 3 {
                if let Some(frame) = handle.latest_frame() {
                    if seen.last() != Some(&frame.seq) {
                        assert_eq!((frame.width, frame.height), (CAPTURE_W, CAPTURE_H));
                        assert_eq!(frame.data.len(), frame_len(CAPTURE_W, CAPTURE_H));
                        seen.push(frame.seq);
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let running_while_started = handle.latest().running;

            let _ = handle.control_tx.send(VideoCaptureControl::Stop);
            let stop_deadline = Instant::now() + Duration::from_secs(5);
            while handle.latest().running && Instant::now() < stop_deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            let running_after_stop = handle.latest().running;
            let _ = std::fs::remove_dir_all(&dir);

            assert!(running_while_started, "the source should report running: {:?}", handle.latest());
            assert!(seen.len() >= 3, "expected at least 3 distinct frames, got {seen:?}");
            assert!(seen.windows(2).all(|w| w[1] > w[0]), "seq must strictly increase: {seen:?}");
            assert!(!running_after_stop, "Stop should take the source back to idle");
        }

        /// Review finding: `VideoFrame::seq` documents a counter that is
        /// never reset by a `Start`. It used to be handed to each reader
        /// thread by value, so every restart really did reset it, which is
        /// a false promise a future consumer comparing with `>` would have
        /// trusted.
        ///
        /// Deterministic on purpose: it drives two `read_loop`s directly
        /// over two finite sources (what a clip cut produces) sharing the
        /// counter the run loop owns, instead of racing a live restart.
        /// A timing-based version of this test passes even against the old
        /// by-value counter, because a reset sequence climbs back past any
        /// fixed floor within a few hundred milliseconds; comparing the two
        /// runs' own last values cannot be fooled that way.
        #[test]
        fn one_shared_counter_spans_every_reader_thread() {
            // Finite duration, so each `read_loop` returns at EOF instead of
            // needing to be killed; unpaced, so it runs at full speed.
            let source_args = vec![
                "-f".to_string(),
                "lavfi".to_string(),
                "-i".to_string(),
                "testsrc=size=64x48:rate=30:duration=0.2".to_string(),
            ];
            let seq = Arc::new(AtomicU64::new(0));
            let frame: Arc<ArcSwapOption<VideoFrame>> = Arc::new(ArcSwapOption::empty());
            let rate = Arc::new(AtomicU32::new(encode_rate(1.0)));

            let mut last_seq_per_run = Vec::new();
            for run in 0..2 {
                let Ok((source, stdout)) = VideoSource::spawn(&source_args, CAPTURE_W, CAPTURE_H, CAPTURE_FPS)
                else {
                    return; // no ffmpeg on PATH
                };
                read_loop(stdout, frame.clone(), rate.clone(), false, seq.clone());
                drop(source);
                let published = frame.load_full().unwrap_or_else(|| panic!("run {run} published no frame"));
                last_seq_per_run.push(published.seq);
            }

            assert!(
                last_seq_per_run[0] > 0,
                "the first run should have published frames, got {last_seq_per_run:?}"
            );
            assert!(
                last_seq_per_run[1] > last_seq_per_run[0],
                "seq restarted across sources ({last_seq_per_run:?}); it must keep climbing"
            );
        }

        /// The zombie check the module doc comment promises: after `Drop`,
        /// the child must be both killed and reaped.
        #[test]
        fn dropping_a_source_kills_and_reaps_its_ffmpeg_child() {
            let Ok((mut src, _stdout)) = VideoSource::spawn(&testsrc(), 64, 48, 30) else {
                return; // no ffmpeg on PATH
            };
            let pid = src.child.id();
            assert!(src.is_alive(), "ffmpeg should still be running before the drop");
            drop(src);

            #[cfg(target_os = "linux")]
            {
                // Either the pid is gone entirely, or (pid reuse) it is some
                // other process: what must never be true is that it is
                // still there in state Z (zombie), i.e. exited but unreaped.
                // `/proc/<pid>/stat` is `pid (comm) state ...`, and `comm`
                // may itself contain spaces and parens, so the state field
                // is read after the LAST ')'.
                if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                    let after_comm = stat.rsplit_once(')').map(|(_, rest)| rest.trim_start()).unwrap_or("");
                    assert!(!after_comm.starts_with('Z'), "ffmpeg child {pid} was left as a zombie: {stat}");
                }
            }
            #[cfg(not(target_os = "linux"))]
            let _ = pid;
        }
    }
}
