//! The Ableton Link I/O thread: bidirectional tempo/beat/phase sync via
//! `rusty_link`. This entire file only exists in a
//! build with the `link` Cargo feature enabled: see the `#[cfg(feature
//! = "link")]` on this module's `pub mod link;` declaration in `lib.rs`.
//! `rusty_link` wraps Ableton's official C++ Link library, which is
//! GPL-2.0-or-later; gating on the `mod` declaration itself (rather than
//! sprinkling `#[cfg]` through this file) means that with the feature
//! off, this file is never parsed, so no GPL code and no GPL symbol
//! reaches a default build.
//!
//! Mirrors OpenDrop-VJ `main.cjs:306-361`'s `link:start`/`link:stop`/
//! `link:set-tempo` IPC handlers: `enable(true)` + `enable_start_stop_
//! sync(true)` on start, poll every 50ms for tempo/beat/phase(quantum=
//! 4.0)/peers, and an outward `set_tempo` to push a tempo INTO the Link
//! session.
//!
//! `rusty_link`'s API (`AblLink`/`SessionState`, confirmed against its
//! own `link_hut` example) is synchronous: a thin FFI wrapper over the
//! C++ Link library, which runs its own background threads for peer
//! discovery internally. No tokio needed here, unlike `remote_ws`/`obs`/
//! `twitch`: same dedicated-`std::thread` + `ArcSwap` snapshot + `mpsc`
//! control-channel shape as `io::midi`/`io::osc` (see `midi::handle`'s
//! doc comment for the full architecture writeup), blocking on
//! `control_rx.recv()` while idle and switching to a
//! `recv_timeout(POLL_TICK)` loop once started: same idle/active split
//! as `osc::run`.
//!
//! The thread never touches `Show`/`Clock` directly: it only publishes
//! `(tempo, phase01, ...)` via `LinkSnapshot`; `app`'s own per-frame
//! wiring (in `main.rs`) is what calls `Clock::
//! sync_external` once per frame with the latest snapshot, same "thread
//! doesn't own Show" convention used by every other `io` module.
//!
//! Never panics: `rusty_link`'s public API (confirmed via its docs.rs
//! method listing) has no `Result`-returning entry point: there is no
//! connect/bind/recv-style fallible call here, unlike every other `io`
//! thread.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use rusty_link::{AblLink, SessionState};

/// How often the run loop polls the Link session for tempo/beat/phase/
/// peers once started: mirrors `main.cjs:328`'s `setInterval(..., 50)`
/// exactly.
const POLL_TICK: Duration = Duration::from_millis(50);

/// Quantum (in beats) phase/beat are queried against: mirrors
/// `main.cjs:323`'s `linkInst.getPhase(4.0)` exactly (a 4-beat bar).
const PHASE_QUANTUM: f64 = 4.0;

/// Tempo a freshly-constructed `AblLink` starts with, before any peer or
/// `SetTempo` call changes it: mirrors `main.cjs:311`'s `bpm || 120`
/// fallback.
const DEFAULT_BPM: f64 = 120.0;

/// Continuous state published via `LinkHandle::latest()`: never blocks,
/// always the latest known value (mirrors `OscSnapshot`).
pub struct LinkSnapshot {
    pub enabled: bool,
    pub tempo: f64,
    /// Continuously-incrementing beat count within the current
    /// `PHASE_QUANTUM`-beat bar (`SessionState::beat_at_time`'s raw
    /// value): for display only, mirroring `main.cjs`'s `beat` field.
    pub beat: f64,
    /// Phase within the *current beat*, 0..1: already converted from
    /// `SessionState::phase_at_time`'s raw 0..`PHASE_QUANTUM` range (see
    /// `link_phase01`'s doc comment) so `app` can feed it straight into
    /// `Clock::sync_external(bpm, phase01)` without any further math.
    pub phase01: f64,
    pub peers: u64,
}

impl LinkSnapshot {
    pub fn idle() -> Self {
        LinkSnapshot { enabled: false, tempo: 0.0, beat: 0.0, phase01: 0.0, peers: 0 }
    }
}

/// Outward control messages sent to the Link thread.
pub enum LinkControl {
    /// Enables the Link session (`enable(true)` + `enable_start_stop_
    /// sync(true)`) and starts the 50ms poll: mirrors `main.cjs`'s
    /// `link:start` handler. A no-op if already enabled.
    Start,
    /// Disables the Link session (`enable(false)`) and stops polling:
    /// a no-op while already idle. Mirrors `link:stop`.
    Stop,
    /// Pushes `bpm` into the Link session's app session state: mirrors
    /// `link:set-tempo` (`main.cjs:352-357`). Works whether or not Link
    /// is currently enabled, same as the JS reference (which only
    /// checks the instance exists, not `isEnabled()`).
    SetTempo(f64),
}

/// Handle to the running Link thread. Mirrors `OscHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block.
pub struct LinkHandle {
    state: Arc<ArcSwap<LinkSnapshot>>,
    pub control_tx: Sender<LinkControl>,
}

impl LinkHandle {
    /// Never blocks: an atomic load of the current Arc (mirrors
    /// `OscHandle::latest`).
    pub fn latest(&self) -> Arc<LinkSnapshot> {
        self.state.load_full()
    }
}

/// Spawns the dedicated Link thread and returns immediately. The thread
/// starts idle (`enabled: false`, no poll) until it receives
/// `LinkControl::Start`: mirrors `OscHandle::spawn`'s "starts idle"
/// pattern.
pub fn spawn() -> LinkHandle {
    let state = Arc::new(ArcSwap::from_pointee(LinkSnapshot::idle()));
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, control_rx)
    });
    LinkHandle { state, control_tx }
}

fn publish_idle(state: &Arc<ArcSwap<LinkSnapshot>>) {
    state.store(Arc::new(LinkSnapshot::idle()));
}

fn publish_polled(state: &Arc<ArcSwap<LinkSnapshot>>, tempo: f64, beat: f64, phase01: f64, peers: u64) {
    state.store(Arc::new(LinkSnapshot { enabled: true, tempo, beat, phase01, peers }));
}

/// Converts `SessionState::phase_at_time`'s raw phase (0..`PHASE_
/// QUANTUM`, wrapping at the bar) into the 0..1-within-one-beat value
/// `Clock::sync_external` expects (`core/src/clock.rs:57-67`'s
/// `phase01` field is the fraction of a single beat, not of a
/// `PHASE_QUANTUM`-beat bar). Quantum-agnostic on purpose: Link's raw
/// phase is `beat mod quantum`, so its fractional part is the same "how
/// far into the current beat" value no matter which quantum it was
/// queried with: this only takes that fractional part.
fn link_phase01(raw_phase_in_beats: f64) -> f64 {
    raw_phase_in_beats.rem_euclid(1.0)
}

/// Pushes `bpm` into the Link session's app session state: mirrors
/// `link:set-tempo`'s `linkInst.setTempo(bpm)`. Capture/set/commit
/// against the *app* session state specifically (not audio), the same
/// half `capture_app_session_state`/`commit_app_session_state` the poll
/// tick below reads from.
fn set_tempo(link: &AblLink, session_state: &mut SessionState, bpm: f64) {
    link.capture_app_session_state(session_state);
    session_state.set_tempo(bpm, link.clock_micros());
    link.commit_app_session_state(session_state);
}

fn run(state: Arc<ArcSwap<LinkSnapshot>>, control_rx: Receiver<LinkControl>) {
    let link = AblLink::new(DEFAULT_BPM);
    let mut session_state = SessionState::new();
    let mut enabled = false;

    loop {
        if !enabled {
            // Idle: nothing to poll, block on the control channel
            // instead of busy-polling, same as `osc::run`'s idle branch.
            match control_rx.recv() {
                Ok(LinkControl::Start) => {
                    link.enable(true);
                    link.enable_start_stop_sync(true);
                    enabled = true;
                }
                Ok(LinkControl::Stop) => {} // already stopped, no-op
                Ok(LinkControl::SetTempo(bpm)) => set_tempo(&link, &mut session_state, bpm),
                Err(_) => break, // LinkHandle (and control_tx) dropped: shut down.
            }
            continue;
        }

        match control_rx.recv_timeout(POLL_TICK) {
            Ok(LinkControl::Start) => {} // already enabled, no-op
            Ok(LinkControl::Stop) => {
                link.enable(false);
                enabled = false;
                publish_idle(&state);
            }
            Ok(LinkControl::SetTempo(bpm)) => set_tempo(&link, &mut session_state, bpm),
            Err(RecvTimeoutError::Timeout) => {
                // The 50ms poll tick itself: mirrors `main.cjs:322-330`.
                link.capture_app_session_state(&mut session_state);
                let now = link.clock_micros();
                let tempo = session_state.tempo();
                let beat = session_state.beat_at_time(now, PHASE_QUANTUM);
                let phase = session_state.phase_at_time(now, PHASE_QUANTUM);
                let peers = link.num_peers();
                publish_polled(&state, tempo, beat, link_phase01(phase), peers);
            }
            Err(RecvTimeoutError::Disconnected) => break, // LinkHandle dropped: shut down.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_snapshot_is_idle() {
        let s = LinkSnapshot::idle();
        assert!(!s.enabled);
        assert_eq!(s.tempo, 0.0);
        assert_eq!(s.peers, 0);
    }

    #[test]
    fn phase01_takes_the_fractional_beat() {
        assert!((link_phase01(0.0) - 0.0).abs() < 1e-9);
        assert!((link_phase01(1.25) - 0.25).abs() < 1e-9);
        assert!((link_phase01(3.75) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn phase01_is_quantum_agnostic() {
        // Same fractional-beat position, whether it came from a 1-beat
        // or 4-beat quantum query: only the integer part (which bar
        // beat we're in) differs, and that's discarded on purpose.
        assert!((link_phase01(0.6) - link_phase01(4.6)).abs() < 1e-9);
    }

    #[test]
    fn phase01_never_reaches_one() {
        // phase_at_time never returns exactly an integer beat while
        // still in that beat, but a value just under it should map
        // close to 1.0, not negative or >= 1.
        let p = link_phase01(3.999);
        assert!(p > 0.99 && p < 1.0);
    }
}
