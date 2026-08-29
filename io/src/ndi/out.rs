//! The NDI output thread: composite (1) + per-deck (up to `DECK_COUNT`) NDI
//! senders, fed by the `mpsc::Receiver<Vec<u8>>` channels Step 5 already
//! wired up in `app`'s render loop (one per `FrameReadback`). This module
//! only owns the sending side: it does not create its own frame-data
//! channels (unlike `audio::spawn_capture`, which owns its device-selection
//! channel internally): those 5 receivers already exist upstream in `app`
//! and are handed to [`spawn`] once, by value, at construction time.
//!
//! **Correctness requirement driving the whole run loop**: Step 5 gates its
//! GPU readback behind `ndi_active || v4l2_active`: an OR over the whole
//! feature, not per-slot: so this thread can receive frames on any of the
//! 5 channels even when that particular slot's NDI stream was never
//! started (e.g. only v4l2loopback is active, or NDI is active for the
//! composite but not deck 2). Every receiver is drained every tick
//! regardless of whether its slot is started ([`drain_slot`]); an unstarted
//! slot's bytes are simply discarded. Skipping this would leave the
//! upstream `mpsc::Sender` (unbounded) filling up forever: a real memory
//! leak, not a hypothetical one.
//!
//! **RGBA, not BGRA, for both streams**: Step 4/5's readback is native GL
//! RGBA8 (no swizzle) for the compositor's texture and all 4 deck
//! textures. This is a deliberate departure from the OpenDrop-VJ (Electron)
//! reference, which sent the composite over NDI as BGRA: that choice was
//! forced by `Electron.capturePage()`, which has no equivalent here; the
//! per-deck streams in that reference already used RGBA
//! (`electron/main.cjs:642-644`), and that choice is simply extended to the
//! composite stream too. See [`PixelFormat::RGBA`].
//!
//! Mirrors `midi::spawn`/`MidiHandle`'s shape: a dedicated `std::thread`
//! publishes continuous state via `ArcSwap` and never panics on an NDI SDK
//! error (SDK not found, sender creation failure): it logs once and
//! leaves the affected slot's [`NdiSnapshot`] showing inactive.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use grafton_ndi::{Finder, PixelFormat, Sender as GraftonSender, SenderOptions, VideoFrame, NDI};

use super::in_::{self, NdiFrame, NdiSource};

/// Mirrors `engine::compositor::COMP_W`/`COMP_H`: hardcoded here rather
/// than depending on `opendrop-engine` (a GL/projectM crate) from this
/// I/O-only crate just for 4 integers. `audio` and `midi` don't depend on
/// `engine` either, and a mismatch would fail loudly at Task 10's call site
/// (an `[mpsc::Receiver<Vec<u8>>; N]` size mismatch), not silently.
///
/// Not auto-synced: if `engine::compositor::COMP_W`/`COMP_H` ever change,
/// these need updating here by hand too. A drift is still safe, just not
/// silent: `SlotSender::send`'s `VideoFrame::replace_data` call rejects a
/// wrong-sized buffer, logs it, and drops that one frame, rather than
/// panicking or sending corrupt data.
const COMP_W: i32 = 1920;
const COMP_H: i32 = 1080;
/// Mirrors `engine::deck::DECK_W`/`DECK_H`/`DECK_COUNT`, same manual-sync
/// caveat as [`COMP_W`].
const DECK_W: i32 = 1280;
const DECK_H: i32 = 720;
const DECK_COUNT: usize = 4;

/// `frameRateN`/`frameRateD` from the OpenDrop-VJ reference
/// (`electron/main.cjs:566-591`): 29.97fps, used for both streams.
const NDI_FRAME_RATE_N: i32 = 30000;
const NDI_FRAME_RATE_D: i32 = 1001;

/// How often the run loop wakes up to drain the 5 frame receivers even when
/// no control message has arrived. Tighter than `midi`'s 20ms tick: this is
/// a video path, and every tick of added latency here is added latency on
/// every NDI frame, not just on discrete MIDI events.
const POLL_TICK: Duration = Duration::from_millis(5);

/// Handle to the running NDI thread. Mirrors `MidiHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block. `control_tx` is
/// a public field, not wrapped in setter methods, for the same reason as
/// `MidiHandle::control_tx`: Task 10 sends one of several `NdiControl`
/// variants directly (`ndi.control_tx.send(StartDeck(slot, name))`).
pub struct NdiHandle {
    state: Arc<ArcSwap<NdiSnapshot>>,
    pub control_tx: Sender<NdiControl>,
    /// Receive end of the channel [`in_::ActiveReceive`] pushes captured
    /// frames on (RGBA bytes plus resolution: see [`NdiFrame`]; NDI
    /// sources can be any resolution, unlike the fixed-size compositor/deck
    /// channels). The channel is created in this module (`spawn`, alongside
    /// `control_tx`'s channel); the `Sender` half stays internal to the NDI
    /// thread (owned by `ThreadState`, cloned into each `ActiveReceive`),
    /// and this `Receiver` half is handed out here for `app` (Task 12) to
    /// read frames from directly: same shape as `control_tx`: a public
    /// field, no wrapper method, since there is nothing to validate on
    /// either send or receive.
    pub frame_rx: Receiver<NdiFrame>,
}

impl NdiHandle {
    /// Never blocks: an atomic load of the current Arc (mirrors
    /// `AudioHandle::latest`/`MidiHandle::latest`).
    pub fn latest(&self) -> Arc<NdiSnapshot> {
        self.state.load_full()
    }
}

/// Published once per run-loop tick: which NDI streams are currently
/// sending. `deck_active[slot]` mirrors `NdiControl::StartDeck(slot, _)` /
/// `StopDeck(slot)`, and also goes back to `false` on its own if that
/// slot's sender ever failed to start (SDK error, etc.): never a panic,
/// just "not active".
#[derive(Debug, Clone, Default)]
pub struct NdiSnapshot {
    pub composite_active: bool,
    pub deck_active: [bool; DECK_COUNT],
    /// Sources currently known to the discovery `Finder`, refreshed every
    /// run-loop tick while discovery is active (see `in_::find`): empty
    /// when discovery was never started or has been stopped. Polled, not
    /// event-driven: there is no separate "source list changed" signal,
    /// `app` (Task 12) just re-reads this on its own cadence via `latest()`.
    pub sources: Vec<NdiSource>,
    /// Whether a receive session is currently connected (mirrors
    /// `composite_active`/`deck_active`'s convention), also going back to
    /// `false` on its own if the receiver ever failed to start.
    pub receive_active: bool,
}

/// Outward control requests, sent non-blocking via `NdiHandle::control_tx`.
/// `slot` is a deck index, 0..`DECK_COUNT`; an out-of-range slot is ignored
/// (logged, no panic): see [`is_valid_deck_slot`].
pub enum NdiControl {
    StartComposite(String),
    StopComposite,
    StartDeck(usize, String),
    StopDeck(usize),
    /// Starts (or restarts) source discovery. No filter argument: mirrors
    /// the reference (`grandiose.find({}, ...)`, no filter either): see
    /// `in_::find`'s doc comment.
    StartDiscovery,
    StopDiscovery,
    /// Connects to the given source and starts pushing its frames on
    /// `NdiHandle::frame_rx`. Replaces any receive session already in
    /// progress.
    StartReceive(NdiSource),
    StopReceive,
}

/// Spawns the dedicated NDI thread and returns immediately. The thread
/// starts with every slot inactive (mirrors `MidiHandle`'s start-idle
/// convention): no NDI sender exists, and the process-global `NDI` runtime
/// itself is only acquired lazily, on the first `StartComposite`/
/// `StartDeck`, so a session that never touches NDI (v4l2loopback-only)
/// never pays for it and never logs a spurious "SDK not found".
///
/// `compositor_rx`/`deck_rx` are the Step 5 `FrameReadback` channels,
/// passed in by value: this function does not create them (see the module
/// doc comment).
pub fn spawn(compositor_rx: Receiver<Vec<u8>>, deck_rx: [Receiver<Vec<u8>>; DECK_COUNT]) -> NdiHandle {
    let state = Arc::new(ArcSwap::from_pointee(NdiSnapshot::default()));
    let (control_tx, control_rx) = mpsc::channel();
    let (frame_tx, frame_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, compositor_rx, deck_rx, control_rx, frame_tx)
    });
    NdiHandle { state, control_tx, frame_rx }
}

/// One active NDI stream: the SDK sender plus a reusable [`VideoFrame`]
/// sized/formatted once at start time. Reused across every frame via
/// `VideoFrame::replace_data` (a checked move of the received `Vec<u8>`,
/// not a copy) rather than rebuilding the frame from scratch each time.
struct SlotSender {
    sender: GraftonSender,
    frame: VideoFrame,
}

impl SlotSender {
    fn new(ndi: &NDI, name: &str, width: i32, height: i32) -> grafton_ndi::Result<Self> {
        let options = SenderOptions::builder(name).clock_video(false).clock_audio(false).build();
        let sender = GraftonSender::new(ndi, &options)?;
        let frame = VideoFrame::builder()
            .resolution(width, height)
            .pixel_format(PixelFormat::RGBA)
            .frame_rate(NDI_FRAME_RATE_N, NDI_FRAME_RATE_D)
            .aspect_ratio(width as f32 / height as f32)
            .build()?;
        Ok(Self { sender, frame })
    }

    /// Pushes one already-captured RGBA8 frame out over NDI. `bytes` must
    /// be exactly `width*height*4`: Step 5's `FrameReadback` always
    /// produces that for a given slot's fixed resolution: a mismatch is
    /// logged and the frame dropped rather than panicking.
    fn send(&mut self, bytes: Vec<u8>) {
        if let Err(e) = self.frame.replace_data(bytes) {
            eprintln!("[ndi] dropping frame with unexpected size: {e}");
            return;
        }
        self.sender.send_video(&self.frame);
    }
}

/// `slot` indices from `NdiControl::StartDeck`/`StopDeck` come from Task
/// 10's UI wiring, not from this thread: validate before indexing.
fn is_valid_deck_slot(slot: usize) -> bool {
    slot < DECK_COUNT
}

struct ThreadState {
    /// The process-global NDI runtime handle, acquired lazily on the first
    /// `StartComposite`/`StartDeck` (see `spawn`'s doc comment). `None`
    /// forever after a failed acquisition attempt: retried on the next
    /// Start request rather than cached as permanently broken, in case the
    /// SDK becomes loadable later (e.g. a `LD_LIBRARY_PATH` fix without
    /// restarting the app is unlikely, but this costs nothing to allow).
    ndi: Option<NDI>,
    composite: Option<SlotSender>,
    decks: [Option<SlotSender>; DECK_COUNT],
    /// Discovery `Finder`, kept alive across ticks (rather than recreated
    /// per-poll) so the SDK's own accumulated source list carries over
    /// between `in_::find` calls: see that function's doc comment.
    finder: Option<Finder>,
    /// Latest discovery snapshot, refreshed every tick while `finder` is
    /// `Some` and published verbatim into `NdiSnapshot::sources`.
    sources: Vec<NdiSource>,
    receive: Option<in_::ActiveReceive>,
    /// Sender half of the frame channel `spawn` created; cloned into each
    /// new `in_::ActiveReceive` on `StartReceive`.
    frame_tx: Sender<NdiFrame>,
}

impl ThreadState {
    fn new(frame_tx: Sender<NdiFrame>) -> Self {
        ThreadState {
            ndi: None,
            composite: None,
            decks: std::array::from_fn(|_| None),
            finder: None,
            sources: Vec::new(),
            receive: None,
            frame_tx,
        }
    }
}

fn run(
    state: Arc<ArcSwap<NdiSnapshot>>,
    compositor_rx: Receiver<Vec<u8>>,
    deck_rx: [Receiver<Vec<u8>>; DECK_COUNT],
    control_rx: Receiver<NdiControl>,
    frame_tx: Sender<NdiFrame>,
) {
    let mut ts = ThreadState::new(frame_tx);
    publish(&state, &ts);

    loop {
        let mut owner_gone = false;
        match control_rx.recv_timeout(POLL_TICK) {
            Ok(ctrl) => handle_control(&mut ts, ctrl),
            Err(RecvTimeoutError::Timeout) => {}
            // Never actually happens: every sender (compositor_rx's/deck_rx's
            // producer side, and control_tx) is held alive by `app`'s
            // AppState/NdiHandle for the whole process lifetime.
            Err(RecvTimeoutError::Disconnected) => owner_gone = true,
        }
        while !owner_gone {
            match control_rx.try_recv() {
                Ok(ctrl) => handle_control(&mut ts, ctrl),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => owner_gone = true,
            }
        }

        // Drained every tick regardless of active state: see the module
        // doc comment's correctness requirement.
        drain_slot(&compositor_rx, ts.composite.as_mut());
        for (rx, slot) in deck_rx.iter().zip(ts.decks.iter_mut()) {
            drain_slot(rx, slot.as_mut());
        }

        // Input side, same every-tick cadence: poll discovery (non-blocking,
        // `timeout_ms = 0`, see `in_::find`) and forward one receive poll:
        // never block this shared thread on either.
        if let Some(finder) = ts.finder.as_ref() {
            ts.sources = in_::find(finder, None, 0);
        }
        if let Some(active_receive) = ts.receive.as_ref() {
            active_receive.poll();
        }

        publish(&state, &ts);

        if owner_gone {
            break; // NdiHandle (and its control_tx) dropped: shut down.
        }
    }
}

/// Fully drains `rx` this tick. When `slot` is `Some` (that stream was
/// started), every received frame is forwarded to NDI; when `None`, the
/// bytes are simply discarded: draining still happens either way so the
/// upstream (unbounded) `mpsc::Sender` never backs up.
fn drain_slot(rx: &Receiver<Vec<u8>>, slot: Option<&mut SlotSender>) {
    match slot {
        Some(sender) => {
            while let Ok(bytes) = rx.try_recv() {
                sender.send(bytes);
            }
        }
        None => while rx.try_recv().is_ok() {},
    }
}

fn publish(state: &Arc<ArcSwap<NdiSnapshot>>, ts: &ThreadState) {
    state.store(Arc::new(NdiSnapshot {
        composite_active: ts.composite.is_some(),
        deck_active: std::array::from_fn(|i| ts.decks[i].is_some()),
        sources: ts.sources.clone(),
        receive_active: ts.receive.is_some(),
    }));
}

fn handle_control(ts: &mut ThreadState, ctrl: NdiControl) {
    match ctrl {
        NdiControl::StartComposite(name) => start_composite(ts, &name),
        NdiControl::StopComposite => ts.composite = None,
        NdiControl::StartDeck(slot, name) => start_deck(ts, slot, &name),
        NdiControl::StopDeck(slot) => {
            if is_valid_deck_slot(slot) {
                ts.decks[slot] = None;
            }
        }
        NdiControl::StartDiscovery => start_discovery(ts),
        NdiControl::StopDiscovery => {
            ts.finder = None;
            ts.sources.clear();
        }
        NdiControl::StartReceive(source) => start_receive(ts, source),
        NdiControl::StopReceive => ts.receive = None,
    }
}

/// Ensures `ts.ndi` is populated, acquiring the runtime on first use.
/// Returns `false` (logging once) if the SDK can't be initialized.
fn ensure_ndi(ts: &mut ThreadState) -> bool {
    if ts.ndi.is_some() {
        return true;
    }
    match NDI::new() {
        Ok(ndi) => {
            ts.ndi = Some(ndi);
            true
        }
        Err(e) => {
            eprintln!("[ndi] failed to initialize the NDI runtime: {e}: NDI unavailable");
            false
        }
    }
}

fn start_composite(ts: &mut ThreadState, name: &str) {
    if !ensure_ndi(ts) {
        ts.composite = None;
        return;
    }
    let ndi = ts.ndi.as_ref().expect("ensure_ndi just guaranteed Some");
    match SlotSender::new(ndi, name, COMP_W, COMP_H) {
        Ok(s) => ts.composite = Some(s),
        Err(e) => {
            eprintln!("[ndi] failed to start composite sender '{name}': {e}");
            ts.composite = None;
        }
    }
}

fn start_deck(ts: &mut ThreadState, slot: usize, name: &str) {
    if !is_valid_deck_slot(slot) {
        eprintln!("[ndi] ignoring StartDeck for out-of-range slot {slot}");
        return;
    }
    if !ensure_ndi(ts) {
        ts.decks[slot] = None;
        return;
    }
    let ndi = ts.ndi.as_ref().expect("ensure_ndi just guaranteed Some");
    match SlotSender::new(ndi, name, DECK_W, DECK_H) {
        Ok(s) => ts.decks[slot] = Some(s),
        Err(e) => {
            eprintln!("[ndi] failed to start deck {slot} sender '{name}': {e}");
            ts.decks[slot] = None;
        }
    }
}

fn start_discovery(ts: &mut ThreadState) {
    if !ensure_ndi(ts) {
        ts.finder = None;
        return;
    }
    let ndi = ts.ndi.as_ref().expect("ensure_ndi just guaranteed Some");
    ts.finder = in_::open_finder(ndi);
}

fn start_receive(ts: &mut ThreadState, source: NdiSource) {
    if !ensure_ndi(ts) {
        ts.receive = None;
        return;
    }
    let ndi = ts.ndi.as_ref().expect("ensure_ndi just guaranteed Some");
    ts.receive = in_::ActiveReceive::start(ndi, source, ts.frame_tx.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ThreadState::new()`/a fresh `NdiSnapshot` must start fully
    /// inactive: no panics, no sender assumed.
    #[test]
    fn fresh_thread_state_is_inactive() {
        let (frame_tx, _frame_rx) = mpsc::channel();
        let ts = ThreadState::new(frame_tx);
        assert!(ts.ndi.is_none());
        assert!(ts.composite.is_none());
        assert!(ts.decks.iter().all(Option::is_none));
        assert!(ts.finder.is_none());
        assert!(ts.sources.is_empty());
        assert!(ts.receive.is_none());

        let snapshot = NdiSnapshot::default();
        assert!(!snapshot.composite_active);
        assert!(snapshot.deck_active.iter().all(|&a| !a));
        assert!(snapshot.sources.is_empty());
        assert!(!snapshot.receive_active);
    }

    #[test]
    fn deck_slot_validation() {
        assert!(is_valid_deck_slot(0));
        assert!(is_valid_deck_slot(DECK_COUNT - 1));
        assert!(!is_valid_deck_slot(DECK_COUNT));
        assert!(!is_valid_deck_slot(usize::MAX));
    }

    /// The dimension constants mirrored from `engine::compositor`/
    /// `engine::deck` (see their doc comments) must line up with what
    /// `PixelFormat::RGBA` expects a frame's buffer to be, since Step 5's
    /// `FrameReadback` publishes exactly `w*h*4` RGBA8 bytes and this
    /// module never infers dimensions from the buffer length.
    #[test]
    fn frame_dimensions_match_readback_buffer_size() {
        let comp_size = PixelFormat::RGBA.try_buffer_size(COMP_W, COMP_H).unwrap();
        assert_eq!(comp_size, (COMP_W as usize) * (COMP_H as usize) * 4);

        let deck_size = PixelFormat::RGBA.try_buffer_size(DECK_W, DECK_H).unwrap();
        assert_eq!(deck_size, (DECK_W as usize) * (DECK_H as usize) * 4);
    }
}
