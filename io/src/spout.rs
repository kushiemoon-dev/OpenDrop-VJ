//! Spout output (Windows): the composite plus up to four per-deck streams,
//! shared with other applications such as Resolume or OBS through Spout's
//! DirectX 11 sender.
//!
//! The app already reads each texture back to RGBA bytes for NDI; those same
//! bytes are what a Spout sender takes, so no graphics interop with the
//! render contexts is needed. A dedicated thread owns the senders: the render
//! loop only pushes frames onto a bounded channel and never blocks. When the
//! thread falls behind, frames are dropped rather than queued, so a slow
//! receiver cannot grow memory.
//!
//! Everything platform specific sits behind [`SpoutSink`], so the thread's
//! behaviour is testable on any OS; off Windows, starting a stream just
//! reports that Spout is unavailable.

use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;

use arc_swap::ArcSwap;

/// Number of per-deck streams, matching the decks.
pub const SPOUT_DECKS: usize = 4;
/// Frames waiting for the sender thread before new ones are dropped.
const FRAME_QUEUE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpoutSlot {
    Composite,
    Deck(usize),
}

impl SpoutSlot {
    fn index(self) -> Option<usize> {
        match self {
            SpoutSlot::Composite => Some(0),
            SpoutSlot::Deck(i) if i < SPOUT_DECKS => Some(i + 1),
            SpoutSlot::Deck(_) => None,
        }
    }
}

/// A destination for frames. One per running stream, owned by the sender
/// thread.
pub trait SpoutSink {
    fn send(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), String>;
}

type SinkFactory = fn(&str) -> Result<Box<dyn SpoutSink>, String>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpoutSnapshot {
    pub composite_active: bool,
    pub deck_active: [bool; SPOUT_DECKS],
    /// Why the last start or send failed, until the next successful start.
    pub last_error: Option<String>,
}

enum Message {
    Start(SpoutSlot, String),
    Stop(SpoutSlot),
    Frame { slot: SpoutSlot, width: u32, height: u32, rgba: Vec<u8> },
    #[cfg(test)]
    Sync(mpsc::Sender<()>),
}

pub struct SpoutHandle {
    state: Arc<ArcSwap<SpoutSnapshot>>,
    tx: SyncSender<Message>,
}

impl SpoutHandle {
    pub fn latest(&self) -> Arc<SpoutSnapshot> {
        self.state.load_full()
    }

    /// Opens a sender called `name` for `slot`. Failures show up in
    /// [`SpoutSnapshot::last_error`], never as a panic.
    pub fn start(&self, slot: SpoutSlot, name: String) {
        let _ = self.tx.send(Message::Start(slot, name));
    }

    pub fn stop(&self, slot: SpoutSlot) {
        let _ = self.tx.send(Message::Stop(slot));
    }

    /// Hands one RGBA frame to the sender thread. Never blocks: if the
    /// thread is behind, the frame is dropped.
    pub fn send_frame(&self, slot: SpoutSlot, width: u32, height: u32, rgba: Vec<u8>) {
        match self.tx.try_send(Message::Frame { slot, width, height, rgba }) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    #[cfg(test)]
    fn sync(&self) {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.tx.send(Message::Sync(ack_tx)).unwrap();
        ack_rx.recv().unwrap();
    }
}

/// Starts the sender thread with the platform's real sink.
pub fn spawn() -> SpoutHandle {
    spawn_with(platform::open)
}

fn spawn_with(factory: SinkFactory) -> SpoutHandle {
    let state = Arc::new(ArcSwap::from_pointee(SpoutSnapshot::default()));
    let (tx, rx) = mpsc::sync_channel(FRAME_QUEUE);
    // Control messages share the bounded channel; they are rare, so a full
    // queue only ever delays them by a few frames.
    std::thread::spawn({
        let state = state.clone();
        move || run(rx, state, factory)
    });
    SpoutHandle { state, tx }
}

fn run(rx: Receiver<Message>, state: Arc<ArcSwap<SpoutSnapshot>>, factory: SinkFactory) {
    let mut sinks: [Option<Box<dyn SpoutSink>>; SPOUT_DECKS + 1] = std::array::from_fn(|_| None);
    let mut snapshot = SpoutSnapshot::default();
    while let Ok(message) = rx.recv() {
        match message {
            Message::Start(slot, name) => {
                let Some(i) = slot.index() else { continue };
                match factory(&name) {
                    Ok(sink) => {
                        sinks[i] = Some(sink);
                        snapshot.last_error = None;
                    }
                    Err(e) => {
                        eprintln!("[spout] failed to start '{name}': {e}");
                        sinks[i] = None;
                        snapshot.last_error = Some(e);
                    }
                }
            }
            Message::Stop(slot) => {
                if let Some(i) = slot.index() {
                    sinks[i] = None;
                }
            }
            Message::Frame { slot, width, height, rgba } => {
                let Some(i) = slot.index() else { continue };
                // A frame for a stream that is not running (a toggle just
                // flipped, or the sender failed to start) is discarded.
                let Some(sink) = sinks[i].as_mut() else { continue };
                if let Err(e) = sink.send(&rgba, width, height) {
                    eprintln!("[spout] dropping stream: {e}");
                    sinks[i] = None;
                    snapshot.last_error = Some(e);
                }
            }
            #[cfg(test)]
            Message::Sync(ack) => {
                let _ = ack.send(());
            }
        }
        snapshot.composite_active = sinks[0].is_some();
        snapshot.deck_active = std::array::from_fn(|d| sinks[d + 1].is_some());
        state.store(Arc::new(snapshot.clone()));
    }
}

#[cfg(windows)]
mod platform {
    use super::SpoutSink;

    struct DxSink(spout2::dx::Sender);

    impl SpoutSink for DxSink {
        fn send(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
            self.0.send_image(rgba, width, height).map_err(|e| e.to_string())
        }
    }

    pub(super) fn open(name: &str) -> Result<Box<dyn SpoutSink>, String> {
        let mut sender = spout2::dx::Sender::new(name).map_err(|e| e.to_string())?;
        // The readback is RGBA; Spout's default texture format is BGRA, which
        // would swap red and blue.
        sender.set_format(spout2::dx::format::R8G8B8A8_UNORM);
        Ok(Box::new(DxSink(sender)))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::SpoutSink;

    pub(super) fn open(_name: &str) -> Result<Box<dyn SpoutSink>, String> {
        Err("Spout is only available on Windows".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static RECEIVED: Mutex<Vec<(u32, u32, usize)>> = Mutex::new(Vec::new());
    static FAIL_NEXT_SEND: Mutex<bool> = Mutex::new(false);

    struct FakeSink;

    impl SpoutSink for FakeSink {
        fn send(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
            if *FAIL_NEXT_SEND.lock().unwrap() {
                return Err("device lost".to_string());
            }
            RECEIVED.lock().unwrap().push((width, height, rgba.len()));
            Ok(())
        }
    }

    fn ok_factory(_name: &str) -> Result<Box<dyn SpoutSink>, String> {
        Ok(Box::new(FakeSink))
    }

    fn failing_factory(_name: &str) -> Result<Box<dyn SpoutSink>, String> {
        Err("no GPU".to_string())
    }

    // The fake sink records into process-wide statics, so the scenarios run
    // as one test instead of racing each other.
    #[test]
    fn streams_start_receive_frames_fail_and_stop() {
        RECEIVED.lock().unwrap().clear();
        *FAIL_NEXT_SEND.lock().unwrap() = false;

        let handle = spawn_with(ok_factory);
        handle.send_frame(SpoutSlot::Composite, 2, 1, vec![0; 8]);
        handle.sync();
        assert!(RECEIVED.lock().unwrap().is_empty(), "a frame before start is discarded");

        handle.start(SpoutSlot::Composite, "OpenDrop".into());
        handle.start(SpoutSlot::Deck(2), "OpenDrop Deck 3".into());
        handle.sync();
        let snapshot = handle.latest();
        assert!(snapshot.composite_active);
        assert_eq!(snapshot.deck_active, [false, false, true, false]);

        handle.send_frame(SpoutSlot::Composite, 2, 1, vec![0; 8]);
        handle.send_frame(SpoutSlot::Deck(2), 1, 1, vec![0; 4]);
        handle.send_frame(SpoutSlot::Deck(0), 1, 1, vec![0; 4]);
        handle.sync();
        assert_eq!(*RECEIVED.lock().unwrap(), vec![(2, 1, 8), (1, 1, 4)]);

        handle.stop(SpoutSlot::Composite);
        handle.sync();
        assert!(!handle.latest().composite_active);

        *FAIL_NEXT_SEND.lock().unwrap() = true;
        handle.send_frame(SpoutSlot::Deck(2), 1, 1, vec![0; 4]);
        handle.sync();
        let snapshot = handle.latest();
        assert!(!snapshot.deck_active[2], "a failed send ends that stream");
        assert_eq!(snapshot.last_error.as_deref(), Some("device lost"));
        *FAIL_NEXT_SEND.lock().unwrap() = false;
    }

    #[test]
    fn a_stream_that_cannot_start_reports_why_and_stays_inactive() {
        let handle = spawn_with(failing_factory);
        handle.start(SpoutSlot::Composite, "OpenDrop".into());
        handle.sync();
        let snapshot = handle.latest();
        assert!(!snapshot.composite_active);
        assert_eq!(snapshot.last_error.as_deref(), Some("no GPU"));
    }

    #[test]
    fn an_out_of_range_deck_is_ignored() {
        let handle = spawn_with(ok_factory);
        handle.start(SpoutSlot::Deck(9), "bad".into());
        handle.sync();
        assert_eq!(*handle.latest(), SpoutSnapshot::default());
    }

    #[test]
    fn off_windows_the_real_sink_reports_unavailable() {
        #[cfg(not(windows))]
        assert_eq!(platform::open("x").err().as_deref(), Some("Spout is only available on Windows"));
    }
}
