//! The OSC I/O thread: a UDP server that receives `/opendrop/<command>`
//! messages and dispatches them via `command_names::parse_command_id`
//! (Task 3). Mirrors `midi::handle`'s shape (see that module's doc
//! comment for the full architecture writeup): a dedicated `std::thread`
//! owns the `UdpSocket`, publishes continuous state via `ArcSwap`, and
//! never panics on a bind/recv error.
//!
//! Input only: no OSC output, mirroring OpenDrop-VJ's `main.cjs:112-139`,
//! which has none either.
//!
//! One deliberate departure from the later remote-WS task (Task 14, not
//! yet built on this branch): the OSC port is always chosen by the user,
//! never OS-assigned: `main.cjs`'s `ipcMain.handle('osc:start', {
//! port })` takes the port from the renderer's own input field, and the
//! brief is explicit this differs from remote WS's OS-assigned port.

use std::net::UdpSocket;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use opendrop_core::commands::CommandId;
use rosc::{OscPacket, OscType};

use crate::command_names::parse_command_id;

/// Every dispatched OSC command address is `/opendrop/<kebab-case-name>`:
/// mirrors `main.cjs:157`'s `address.startsWith('/opendrop/')` check
/// exactly.
const ADDRESS_PREFIX: &str = "/opendrop/";

/// How often the run loop's blocking `recv_from` wakes up (via
/// `UdpSocket::set_read_timeout`) to service control messages even when
/// no packet has arrived. Mirrors `midi::handle::POLL_TICK`'s role.
const POLL_TICK: Duration = Duration::from_millis(20);

/// Largest UDP payload this thread will attempt to decode. OSC messages
/// carrying a single float32 argument are a few dozen bytes at most;
/// this is a generous ceiling, not a protocol limit.
const MAX_PACKET_LEN: usize = 65536;

/// Continuous state published via `OscHandle::latest()`: never blocks,
/// always the latest known value (mirrors `MidiSnapshot`/`AudioSnapshot`).
pub struct OscSnapshot {
    pub listening: bool,
    pub port: u16,
}

impl OscSnapshot {
    pub fn idle() -> Self {
        OscSnapshot { listening: false, port: 0 }
    }
}

/// Outward control messages sent to the OSC thread.
pub enum OscControl {
    /// (Re)binds the UDP socket to `port`, chosen by the user (never
    /// OS-assigned: see the module doc comment). Closes any
    /// previously-bound socket first, same as `main.cjs`'s
    /// `if (oscServer) { oscServer.close(); oscServer = null }` before
    /// creating the new one: unconditional, even if the port is
    /// unchanged.
    Start(u16),
    /// Closes the socket, if any. A no-op while already idle.
    Stop,
}

/// Handle to the running OSC thread. Mirrors `MidiHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block. `events` and
/// `control_tx` are public fields (not wrapped in accessor methods),
/// matching `MidiHandle`'s judgment call: `app`'s panel and
/// `about_to_wait` drain/send through them directly.
pub struct OscHandle {
    state: Arc<ArcSwap<OscSnapshot>>,
    pub events: Receiver<(CommandId, f64)>,
    pub control_tx: Sender<OscControl>,
}

impl OscHandle {
    /// Never blocks: an atomic load of the current Arc (mirrors
    /// `MidiHandle::latest`).
    pub fn latest(&self) -> Arc<OscSnapshot> {
        self.state.load_full()
    }
}

/// Spawns the dedicated OSC thread and returns immediately. The thread
/// starts idle (no socket bound, `listening: false`) until it receives
/// `OscControl::Start(port)`: mirrors `MidiHandle::spawn`'s "starts idle
/// until an explicit connect" pattern; unlike MIDI there's no
/// hardware-access gate to wait for here, but binding an arbitrary port
/// unasked (before the user has typed one in) would be surprising, so the
/// thread waits the same way.
pub fn spawn() -> OscHandle {
    let state = Arc::new(ArcSwap::from_pointee(OscSnapshot::idle()));
    let (events_tx, events_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, events_tx, control_rx)
    });
    OscHandle { state, events: events_rx, control_tx }
}

fn publish(state: &Arc<ArcSwap<OscSnapshot>>, listening: bool, port: u16) {
    state.store(Arc::new(OscSnapshot { listening, port }));
}

/// Binds a fresh socket on `port`, replacing (and thus dropping/closing)
/// whatever was previously in `socket`. Never panics: a bind failure is
/// logged once and leaves `socket` at `None` / state at not-listening,
/// per the task's error-handling requirement.
///
/// The old socket is dropped *before* the new bind is attempted (not
/// after a successful one): std's `UdpSocket::bind` doesn't set
/// `SO_REUSEADDR`, so binding a second socket on the same address while
/// the first is still open would normally fail with `EADDRINUSE`, which
/// would wrongly report a bind failure on what should be an
/// unconditional rebind (including the same-port case).
fn bind(socket: &mut Option<UdpSocket>, port: u16, state: &Arc<ArcSwap<OscSnapshot>>) {
    *socket = None; // close the old socket first, see doc comment above
    match UdpSocket::bind(("0.0.0.0", port)) {
        Ok(sock) => {
            // Bounds how long `recv_from` blocks below so the run loop can
            // still service `Stop`/`Start` control messages promptly.
            if let Err(e) = sock.set_read_timeout(Some(POLL_TICK)) {
                eprintln!("opendrop-io: OSC set_read_timeout failed: {e}");
            }
            *socket = Some(sock);
            publish(state, true, port);
        }
        Err(e) => {
            eprintln!("opendrop-io: OSC bind on port {port} failed: {e}");
            *socket = None;
            publish(state, false, 0);
        }
    }
}

fn run(state: Arc<ArcSwap<OscSnapshot>>, events_tx: Sender<(CommandId, f64)>, control_rx: Receiver<OscControl>) {
    let mut socket: Option<UdpSocket> = None;

    loop {
        let Some(sock) = &socket else {
            // Idle: no socket bound, nothing to receive, block on the
            // control channel instead of busy-polling.
            match control_rx.recv() {
                Ok(OscControl::Start(port)) => bind(&mut socket, port, &state),
                Ok(OscControl::Stop) => {} // already stopped, no-op
                Err(_) => break,           // OscHandle (and control_tx) dropped: shut down.
            }
            continue;
        };

        let mut buf = [0u8; MAX_PACKET_LEN];
        match sock.recv_from(&mut buf) {
            Ok((len, _src)) => {
                if let Some(dispatch) = decode_packet(&buf[..len]) {
                    let _ = events_tx.send(dispatch);
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                // Expected: the read-timeout tick with no packet pending.
            }
            Err(e) => {
                // A real recv error (e.g. the interface went away). Log
                // once, drop the socket, and report not-listening: never
                // panic, per the task's error-handling requirement.
                eprintln!("opendrop-io: OSC recv error, stopping: {e}");
                socket = None;
                publish(&state, false, 0);
                continue;
            }
        }

        match control_rx.try_recv() {
            Ok(OscControl::Start(port)) => bind(&mut socket, port, &state),
            Ok(OscControl::Stop) => {
                socket = None;
                publish(&state, false, 0);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break, // OscHandle dropped: shut down.
        }
    }
}

/// Decodes one raw UDP payload into a `(CommandId, value01)` dispatch, or
/// `None` if the packet should be silently ignored: malformed OSC, an
/// OSC bundle (only bare messages carry a single dispatch, and the JS
/// reference's hand-rolled parser never handled bundles either), or
/// anything `dispatch_from_message` below rejects. No logging on a
/// decode failure: mirrors `parseOscPacket` returning `null` and the
/// caller's silent `if (!parsed) return`, `main.cjs:154`.
fn decode_packet(buf: &[u8]) -> Option<(CommandId, f64)> {
    let (_, packet) = rosc::decoder::decode_udp(buf).ok()?;
    let OscPacket::Message(msg) = packet else { return None };
    dispatch_from_message(&msg.addr, &msg.args)
}

/// Pure address/argument logic, deliberately factored out of `decode_packet`
/// so it's unit-testable without a real socket: strips the `/opendrop/`
/// prefix, looks the suffix up via `command_names::parse_command_id`
/// (`None` for an unrecognized name is silently ignored, mirroring the JS
/// reference's `if (!cmdId) return`, `main.cjs:157`), then takes the first
/// argument.
///
/// Strictness judgment call (the brief leaves this open): the first
/// argument must decode as `OscType::Float` (OSC's `f` float32 type tag):
/// a message with no arguments, or whose first argument is a different
/// OSC type (`Int`, `Double`, a string, ...), is treated as malformed and
/// the whole packet is ignored. This is a deliberate departure from the JS
/// reference, which defaults a missing argument to `0.0` and dispatches
/// anyway (`main.cjs:129-135`); requiring an actual float32 is stricter
/// but matches the brief's explicit "first argument float32" and its
/// "if the packet's first arg isn't a float, treat as malformed/ignore"
/// framing more directly than silently substituting a default. See the
/// task report for the full write-up of this choice.
fn dispatch_from_message(addr: &str, args: &[OscType]) -> Option<(CommandId, f64)> {
    let suffix = addr.strip_prefix(ADDRESS_PREFIX)?;
    let id = parse_command_id(suffix)?;
    let OscType::Float(value) = args.first()? else { return None };
    // `rosc` does not clamp argument values itself: mirrors
    // `Math.max(0, Math.min(1, value01))`, `main.cjs:133-135`.
    Some((id, (*value as f64).clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(addr: &str, args: Vec<OscType>) -> (String, Vec<OscType>) {
        (addr.to_string(), args)
    }

    #[test]
    fn opendrop_prefixed_address_with_float_arg_dispatches() {
        let (addr, args) = msg("/opendrop/crossfader", vec![OscType::Float(0.5)]);
        assert_eq!(dispatch_from_message(&addr, &args), Some((CommandId::Crossfader, 0.5)));
    }

    #[test]
    fn value_above_one_is_clamped() {
        let (addr, args) = msg("/opendrop/crossfader", vec![OscType::Float(1.7)]);
        assert_eq!(dispatch_from_message(&addr, &args), Some((CommandId::Crossfader, 1.0)));
    }

    #[test]
    fn value_below_zero_is_clamped() {
        let (addr, args) = msg("/opendrop/crossfader", vec![OscType::Float(-0.3)]);
        assert_eq!(dispatch_from_message(&addr, &args), Some((CommandId::Crossfader, 0.0)));
    }

    #[test]
    fn address_without_opendrop_prefix_is_ignored() {
        let (addr, args) = msg("/other/crossfader", vec![OscType::Float(0.5)]);
        assert_eq!(dispatch_from_message(&addr, &args), None);
    }

    #[test]
    fn unrecognized_command_name_is_ignored() {
        let (addr, args) = msg("/opendrop/not-a-real-command", vec![OscType::Float(0.5)]);
        assert_eq!(dispatch_from_message(&addr, &args), None);
    }

    #[test]
    fn missing_argument_is_ignored() {
        let (addr, args) = msg("/opendrop/crossfader", vec![]);
        assert_eq!(dispatch_from_message(&addr, &args), None);
    }

    #[test]
    fn non_float_first_argument_is_ignored() {
        let (addr, args) = msg("/opendrop/crossfader", vec![OscType::Int(1)]);
        assert_eq!(dispatch_from_message(&addr, &args), None);
    }

    #[test]
    fn extra_arguments_after_the_first_are_ignored() {
        let (addr, args) = msg("/opendrop/crossfader", vec![OscType::Float(0.25), OscType::String("ignored".to_string())]);
        assert_eq!(dispatch_from_message(&addr, &args), Some((CommandId::Crossfader, 0.25)));
    }

    /// Round-trips a real encoded OSC packet through `decode_packet`,
    /// exercising `rosc::decoder::decode_udp` itself (not just the pure
    /// address/argument logic above).
    #[test]
    fn decode_packet_round_trips_an_encoded_message() {
        let packet = OscPacket::Message(rosc::OscMessage {
            addr: "/opendrop/strobe-toggle".to_string(),
            args: vec![OscType::Float(0.5)],
        });
        let bytes = rosc::encoder::encode(&packet).expect("valid packet encodes");
        // 0.5 round-trips exactly through f32 -> f64, unlike most decimals.
        assert_eq!(decode_packet(&bytes), Some((CommandId::StrobeToggle, 0.5_f64)));
    }

    #[test]
    fn decode_packet_on_garbage_bytes_does_not_panic() {
        assert_eq!(decode_packet(&[0xff, 0x00, 0x13, 0x37]), None);
        assert_eq!(decode_packet(&[]), None);
    }

    #[test]
    fn fresh_snapshot_is_idle() {
        let s = OscSnapshot::idle();
        assert!(!s.listening);
        assert_eq!(s.port, 0);
    }

    /// Regression test for the review finding: `bind()` must drop the old
    /// socket *before* attempting the new one, or an unconditional
    /// same-port restart (`OscControl::Start(port)` while already bound to
    /// `port`) fails with `EADDRINUSE` (std's `UdpSocket` doesn't set
    /// `SO_REUSEADDR`) instead of succeeding as the doc comment promises.
    ///
    /// Uses a real `UdpSocket`/`bind()` (not mocked) against a concrete,
    /// currently-free port: an OS-assigned ephemeral socket is opened and
    /// immediately dropped just to learn a free port number, then `bind()`
    /// is exercised against that fixed port twice in a row.
    #[test]
    fn bind_can_rebind_the_same_port_without_erroring() {
        let probe = UdpSocket::bind(("0.0.0.0", 0)).expect("OS has a free ephemeral port");
        let port = probe.local_addr().expect("bound socket has a local addr").port();
        drop(probe);

        let state = Arc::new(ArcSwap::from_pointee(OscSnapshot::idle()));
        let mut socket: Option<UdpSocket> = None;

        bind(&mut socket, port, &state);
        assert!(socket.is_some(), "first bind on {port} should succeed");
        assert!(state.load().listening);
        assert_eq!(state.load().port, port);

        // Rebind on the exact same port while `socket` is still `Some(..)`:
        // this is what would hit `EADDRINUSE` without the fix.
        bind(&mut socket, port, &state);
        assert!(socket.is_some(), "rebind on the same port {port} should also succeed");
        assert!(state.load().listening);
        assert_eq!(state.load().port, port);
    }
}
