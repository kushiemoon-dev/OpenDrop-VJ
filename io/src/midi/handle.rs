//! The MIDI I/O thread: port open/hotplug, learn-mode mapping, LED
//! feedback, clock. Mirrors `audio::capture::spawn`/`AudioHandle`'s shape
//! exactly (see the task report for the full architecture writeup): a
//! dedicated `std::thread` owns every `midir` connection, publishes
//! continuous state via `ArcSwap`, and never panics on a connection error.
//!
//! Control-message handling (`Connect`/`Disconnect`/`SelectPort`/`PushLed`)
//! lives in `control.rs`, split out to keep this file under the ~400-line
//! convention. This file owns the run loop itself, incoming-message
//! dispatch (mapping resolution + learn mode), and the clock/hotplug
//! timers.
//!
//! One deliberate departure from the OpenDrop-VJ (WebMIDI) reference:
//! WebMIDI auto-connects to *every* input port; this thread connects to at
//! most one input port at a time, selected explicitly via
//! `MidiControl::SelectPort` (mirroring the dropdown Task 8's brief
//! describes: "liste déroulante port in"). `midir` also has no hotplug
//! *callback* (unlike WebMIDI's `onstatechange`): output-port reconnection
//! is detected by polling, not a callback; see `check_hotplug` below and
//! the task report for why this isn't a `BLOCKED`.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use opendrop_core::commands::CommandId;

use super::clock_sync::MidiClockSync;
use super::control::{self, handle_control};
use super::message::{MidiEvent, MidiParser};
use super::types::{
    bind_trigger, is_note_off, resolve_mapping, trigger_key_and_value, MidiControl, MidiDispatch, MidiMapping, MidiSnapshot,
};

/// How often the run loop wakes up to service control messages and the
/// periodic timeout/hotplug checks even when no MIDI byte has arrived.
const POLL_TICK: Duration = Duration::from_millis(20);
/// No `0xF8` clock byte for this long => report the clock stopped (bpm 0).
/// Mirrors the JS `setTimeout(..., 2000)` in `midi-connection-actions.ts:132`.
const CLOCK_TIMEOUT: Duration = Duration::from_millis(2000);
/// How often to re-enumerate ports for hotplug detection (device_names
/// refresh + "did our output port come back" check). `midir` has no
/// hotplug callback, so this is a deliberate poll, not the brief's assumed
/// "callback midir": see the module doc comment and the task report.
const HOTPLUG_POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// Handle to the running MIDI thread. Mirrors `AudioHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block. `events` and
/// `control_tx` are public fields (not wrapped in accessor methods, unlike
/// `AudioHandle`) because Task 8's brief drains/sends through them directly
/// (`midi.events.try_recv()`, `control_tx.send(StartLearn(id))`): see the
/// task report for this judgment call.
pub struct MidiHandle {
    state: Arc<ArcSwap<MidiSnapshot>>,
    pub events: Receiver<MidiDispatch>,
    pub control_tx: Sender<MidiControl>,
}

impl MidiHandle {
    /// Never blocks: an atomic load of the current Arc (mirrors `AudioHandle::latest`).
    pub fn latest(&self) -> Arc<MidiSnapshot> {
        self.state.load_full()
    }

    /// Requests the thread to send raw on/off LED feedback for `id`'s
    /// mapped trigger, if any. Never blocks: a control message, like every
    /// other outward request (`let _ = ...send(...)`).
    pub fn push_led(&self, id: CommandId, on: bool) {
        let _ = self.control_tx.send(MidiControl::PushLed(id, on));
    }
}

/// Spawns the dedicated MIDI thread and returns immediately. The thread
/// starts idle (no port open, `connected: false`) until it receives
/// `MidiControl::Connect`: unlike `audio::spawn_capture`, which opens the
/// default device immediately, MIDI waits for an explicit connect (mirrors
/// WebMIDI's `requestMIDIAccess()` gate in the JS reference).
pub fn spawn() -> MidiHandle {
    let state = Arc::new(ArcSwap::from_pointee(MidiSnapshot::disconnected()));
    let (events_tx, events_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, events_tx, control_rx)
    });
    MidiHandle { state, events: events_rx, control_tx }
}

/// A raw MIDI byte message delivered from a `midir` input-connection
/// callback (which runs on `midir`'s own backend thread) into this
/// thread's run loop.
pub(super) struct RawMidi {
    pub(super) device_name: String,
    /// Microseconds since an arbitrary, connection-lifetime-stable epoch:
    /// `midir`'s own callback timestamp, used as-is for
    /// `MidiClockSync::on_pulse` (BPM only depends on deltas between
    /// consecutive pulses, not the absolute epoch, and we only ever feed
    /// one connection's timestamps into one `MidiClockSync` instance at a
    /// time: reset together on every `SelectPort`/`Disconnect`). This is
    /// the driver-timestamped edge, jitter-free versus timestamping on
    /// arrival in this thread.
    pub(super) timestamp_us: u64,
    pub(super) data: Vec<u8>,
}

/// Everything the run loop owns, mutated in place; `publish()` mirrors the
/// relevant fields into the public `ArcSwap` snapshot once per loop tick.
/// Fields are `pub(super)` (visible throughout `midi::*`) so `control.rs`
/// can mutate them from its own control-message handlers.
pub(super) struct ThreadState {
    pub(super) input_conn: Option<(String, midir::MidiInputConnection<()>)>,
    pub(super) output_conn: Option<(String, midir::MidiOutputConnection)>,
    pub(super) mapping: MidiMapping,
    pub(super) mapping_path: Option<PathBuf>,
    pub(super) learning: Option<CommandId>,
    pub(super) parser: MidiParser,
    pub(super) clock: MidiClockSync,
    pub(super) last_clock_pulse_at: Option<Instant>,
    pub(super) last_hotplug_poll_at: Instant,

    pub(super) connected: bool,
    pub(super) device_names: Vec<String>,
    pub(super) clock_bpm: f64,
    /// Monotonic; never reset (see `MidiSnapshot::clock_beat_count` doc).
    pub(super) clock_beat_count: u64,
    /// Monotonic; never reset (see `MidiSnapshot::hotplug_epoch` doc).
    pub(super) hotplug_epoch: u64,
}

impl ThreadState {
    pub(super) fn new() -> Self {
        ThreadState {
            input_conn: None,
            output_conn: None,
            mapping: MidiMapping::new(),
            mapping_path: None,
            learning: None,
            parser: MidiParser::new(),
            clock: MidiClockSync::new(),
            last_clock_pulse_at: None,
            last_hotplug_poll_at: Instant::now(),
            connected: false,
            device_names: Vec::new(),
            clock_bpm: 0.0,
            clock_beat_count: 0,
            hotplug_epoch: 0,
        }
    }
}

fn run(state: Arc<ArcSwap<MidiSnapshot>>, events_tx: Sender<MidiDispatch>, control_rx: Receiver<MidiControl>) {
    let (midi_tx, midi_rx) = mpsc::channel::<RawMidi>();
    let mut ts = ThreadState::new();
    publish(&state, &ts);

    loop {
        match midi_rx.recv_timeout(POLL_TICK) {
            Ok(raw) => handle_raw_midi(&mut ts, &events_tx, raw),
            Err(RecvTimeoutError::Timeout) => {}
            // Never actually happens: every sender is a closure held alive
            // by `ts.input_conn`, itself owned by this same loop.
            Err(RecvTimeoutError::Disconnected) => {}
        }

        let mut owner_gone = false;
        loop {
            match control_rx.try_recv() {
                Ok(ctrl) => handle_control(&mut ts, ctrl, &midi_tx),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    owner_gone = true;
                    break;
                }
            }
        }

        check_clock_timeout(&mut ts);
        check_hotplug(&mut ts);
        publish(&state, &ts);

        if owner_gone {
            break; // MidiHandle (and its control_tx) dropped: shut down.
        }
    }
}

fn publish(state: &Arc<ArcSwap<MidiSnapshot>>, ts: &ThreadState) {
    state.store(Arc::new(MidiSnapshot {
        connected: ts.connected,
        device_names: ts.device_names.clone(),
        clock_bpm: ts.clock_bpm,
        clock_beat_count: ts.clock_beat_count,
        hotplug_epoch: ts.hotplug_epoch,
        mapping: ts.mapping.clone(),
    }));
}

// ---------------------------------------------------------------------
// Incoming MIDI messages (mapping resolution, learn mode, clock feed)
// ---------------------------------------------------------------------

fn handle_raw_midi(ts: &mut ThreadState, events_tx: &Sender<MidiDispatch>, raw: RawMidi) {
    let Some(event) = ts.parser.handle(&raw.device_name, &raw.data) else { return };

    if let MidiEvent::Clock = event {
        feed_clock(ts, raw.timestamp_us);
        return;
    }

    let note_off = is_note_off(&event);
    let Some((key, value01)) = trigger_key_and_value(&raw.device_name, event) else { return };

    if let Some(learning_id) = ts.learning {
        if note_off {
            return; // wait for the next non-note-off message, mirrors midi-connection-actions.ts:61
        }
        // Whole-branch review Finding M6: `bind_trigger` evicts any OTHER
        // command already bound to this same `key` first, so a trigger has
        // at most one owner and `resolve_mapping`'s reverse lookup stays
        // unambiguous.
        bind_trigger(&mut ts.mapping, learning_id, key);
        ts.learning = None;
        super::mapping::save_mapping(ts.mapping_path.as_deref(), &ts.mapping);
        return;
    }

    if note_off {
        return; // a mapped note's note-off never dispatches, mirrors midi-connection-actions.ts:74
    }

    if let Some(id) = resolve_mapping(&ts.mapping, &key) {
        let _ = events_tx.send((id, value01));
    }
}

fn feed_clock(ts: &mut ThreadState, timestamp_us: u64) {
    ts.last_clock_pulse_at = Some(Instant::now());
    let now_ms = timestamp_us as f64 / 1000.0;
    let (bpm, beat_fired) = ts.clock.on_pulse(now_ms);
    if let Some(bpm) = bpm {
        ts.clock_bpm = bpm;
    }
    if beat_fired {
        ts.clock_beat_count = ts.clock_beat_count.wrapping_add(1);
    }
}

/// The caller-owned 2000ms-no-pulse inactivity timer `MidiClockSync`
/// documents itself as needing (it owns no timer of its own).
fn check_clock_timeout(ts: &mut ThreadState) {
    let Some(last) = ts.last_clock_pulse_at else { return };
    if last.elapsed() >= CLOCK_TIMEOUT {
        ts.clock.on_timeout();
        ts.clock_bpm = 0.0;
        ts.last_clock_pulse_at = None;
    }
}

// ---------------------------------------------------------------------
// Hotplug (polled: midir has no hotplug callback, see module doc comment)
// ---------------------------------------------------------------------

fn check_hotplug(ts: &mut ThreadState) {
    if ts.last_hotplug_poll_at.elapsed() < HOTPLUG_POLL_INTERVAL {
        return;
    }
    ts.last_hotplug_poll_at = Instant::now();

    if let Ok(input) = midir::MidiInput::new(control::CLIENT_NAME) {
        let names = control::list_port_names(&input);
        if names != ts.device_names {
            ts.device_names = names;
        }
    }

    // Only relevant once a port is selected and its output counterpart
    // isn't open yet (never opened, or dropped because it was unplugged).
    if ts.output_conn.is_none() {
        if let Some((input_name, _)) = &ts.input_conn {
            let input_name = input_name.clone();
            if control::try_open_output(ts, &input_name) {
                ts.hotplug_epoch = ts.hotplug_epoch.wrapping_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ThreadState::new()` must start fully neutral/disconnected: no
    /// panics, no port assumed, matching `MidiSnapshot::disconnected()`.
    #[test]
    fn fresh_thread_state_is_disconnected() {
        let ts = ThreadState::new();
        assert!(!ts.connected);
        assert!(ts.device_names.is_empty());
        assert_eq!(ts.clock_bpm, 0.0);
        assert_eq!(ts.clock_beat_count, 0);
        assert_eq!(ts.hotplug_epoch, 0);
        assert!(ts.input_conn.is_none());
        assert!(ts.output_conn.is_none());
    }
}
