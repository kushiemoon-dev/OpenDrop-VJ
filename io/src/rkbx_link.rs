//! Second, independent OSC UDP listener for the `rkbx_link` community bridge
//! (https://github.com/grufkork/rkbx_link, ticket #10 "Synchronised music
//! video playback"). Same dedicated-thread + `ArcSwap` snapshot + `mpsc`
//! control-channel architecture as `crate::osc` (see that module's doc
//! comment for the full write-up); a second listener rather than an
//! extension of `crate::osc`'s parser because rkbx_link's message shape is
//! entirely different: address-per-deck (`/{deck}/...`), not
//! `/opendrop/<command>`, and its `track/*` fields are OSC strings, not the
//! single float `crate::osc::dispatch_from_message` requires.
//!
//! Two kinds of data, published two different ways:
//! - **`/{deck}/time`** (elapsed playback seconds) arrives continuously
//!   while `osc.msg.n/time` is on in the user's rkbx_link config: published
//!   as continuous state (`RkbxLinkSnapshot::deck_time`), latest-wins, same
//!   shape as `VideoCaptureSnapshot`/`MidiSnapshot`.
//! - **`/{deck}/track/title`/`/{deck}/track/artist`** arrive once per real
//!   track change (rkbx_link's own `track_changed` event, never per-tick):
//!   naturally event-shaped, so they are reported over `track_events`, an
//!   `mpsc::Receiver`, same idiom as `OscHandle::events`. `album` is
//!   recognized as a known address but never stored: matching (ticket #10)
//!   is title+artist only.

use std::net::UdpSocket;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use rosc::{OscPacket, OscType};

/// Up to 4 DJ decks (`keeper.decks` in rkbx_link's own config is 2 or 4).
pub const MAX_DJ_DECKS: usize = 4;

const POLL_TICK: Duration = Duration::from_millis(20);
const MAX_PACKET_LEN: usize = 65536;

/// Continuous state published via `RkbxLinkHandle::latest()`.
pub struct RkbxLinkSnapshot {
    pub listening: bool,
    pub port: u16,
    /// Latest known elapsed-playback time (seconds) per DJ deck index, or
    /// `None` if no `/{deck}/time` has arrived yet for that deck (including
    /// while the listener itself is idle).
    pub deck_time: [Option<f64>; MAX_DJ_DECKS],
}

impl RkbxLinkSnapshot {
    pub fn idle() -> Self {
        RkbxLinkSnapshot { listening: false, port: 0, deck_time: [None; MAX_DJ_DECKS] }
    }
}

/// One rkbx_link `track_changed` event: DJ deck `deck`'s newly-reported
/// title/artist. `album` is deliberately not carried: nothing uses it
/// (matching is title+artist only, see `app::video_clips::match_clip_by_track`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RkbxTrackChanged {
    pub deck: usize,
    pub title: String,
    pub artist: String,
}

pub enum RkbxLinkControl {
    Start(u16),
    Stop,
}

pub struct RkbxLinkHandle {
    state: Arc<ArcSwap<RkbxLinkSnapshot>>,
    pub track_events: Receiver<RkbxTrackChanged>,
    pub control_tx: Sender<RkbxLinkControl>,
}

impl RkbxLinkHandle {
    pub fn latest(&self) -> Arc<RkbxLinkSnapshot> {
        self.state.load_full()
    }
}

pub fn spawn() -> RkbxLinkHandle {
    let state = Arc::new(ArcSwap::from_pointee(RkbxLinkSnapshot::idle()));
    let (track_tx, track_events) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, track_tx, control_rx)
    });
    RkbxLinkHandle { state, track_events, control_tx }
}

// bind()/run() mirror io::osc::bind/run almost verbatim: same "close old
// socket before rebind", same read-timeout poll tick, same never-panic
// recv-error handling, same Start/Stop control polling after each recv. The
// two differences: `run` also threads a mutable per-thread
// `[Option<f64>; MAX_DJ_DECKS]` (the deck-time half of the snapshot, since
// only `run` mutates it tick-to-tick) and, on a decoded message, updates
// either that array + republishes, or sends a `RkbxTrackChanged` event,
// instead of pushing to a single event channel.
fn run(state: Arc<ArcSwap<RkbxLinkSnapshot>>, track_tx: Sender<RkbxTrackChanged>, control_rx: Receiver<RkbxLinkControl>) {
    let mut socket: Option<UdpSocket> = None;
    let mut port: u16 = 0;
    let mut deck_time: [Option<f64>; MAX_DJ_DECKS] = [None; MAX_DJ_DECKS];
    // Held so a title arriving before its artist (or vice versa) can still
    // be combined into one `RkbxTrackChanged` once both halves are known;
    // see `decode_message`'s doc comment on why there is no explicit
    // "track_changed" OSC message to key off instead.
    let mut deck_title: [Option<String>; MAX_DJ_DECKS] = Default::default();
    let mut deck_artist: [Option<String>; MAX_DJ_DECKS] = Default::default();

    loop {
        let Some(sock) = &socket else {
            match control_rx.recv() {
                Ok(RkbxLinkControl::Start(p)) => {
                    port = p;
                    bind(&mut socket, p, &state, &deck_time);
                }
                Ok(RkbxLinkControl::Stop) => {}
                Err(_) => break,
            }
            continue;
        };

        let mut buf = [0u8; MAX_PACKET_LEN];
        match sock.recv_from(&mut buf) {
            Ok((len, _src)) => {
                if let Some(msg) = decode_packet(&buf[..len]) {
                    match msg {
                        RkbxMessage::Time { deck, seconds } => {
                            deck_time[deck] = Some(seconds);
                            publish(&state, true, port, deck_time);
                        }
                        RkbxMessage::TrackTitle { deck, title } => {
                            deck_title[deck] = Some(title.clone());
                            if let Some(artist) = deck_artist[deck].clone() {
                                let _ = track_tx.send(RkbxTrackChanged { deck, title, artist });
                            }
                        }
                        RkbxMessage::TrackArtist { deck, artist } => {
                            deck_artist[deck] = Some(artist.clone());
                            if let Some(title) = deck_title[deck].clone() {
                                let _ = track_tx.send(RkbxTrackChanged { deck, title, artist });
                            }
                        }
                    }
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => {
                eprintln!("opendrop-io: rkbx_link recv error, stopping: {e}");
                socket = None;
                deck_time = [None; MAX_DJ_DECKS];
                publish(&state, false, 0, deck_time);
                continue;
            }
        }

        match control_rx.try_recv() {
            Ok(RkbxLinkControl::Start(p)) => {
                port = p;
                bind(&mut socket, p, &state, &deck_time);
            }
            Ok(RkbxLinkControl::Stop) => {
                socket = None;
                deck_time = [None; MAX_DJ_DECKS];
                publish(&state, false, 0, deck_time);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }
    }
}

fn publish(state: &Arc<ArcSwap<RkbxLinkSnapshot>>, listening: bool, port: u16, deck_time: [Option<f64>; MAX_DJ_DECKS]) {
    state.store(Arc::new(RkbxLinkSnapshot { listening, port, deck_time }));
}

fn bind(socket: &mut Option<UdpSocket>, port: u16, state: &Arc<ArcSwap<RkbxLinkSnapshot>>, deck_time: &[Option<f64>; MAX_DJ_DECKS]) {
    *socket = None;
    match UdpSocket::bind(("0.0.0.0", port)) {
        Ok(sock) => {
            if let Err(e) = sock.set_read_timeout(Some(POLL_TICK)) {
                eprintln!("opendrop-io: rkbx_link set_read_timeout failed: {e}");
            }
            *socket = Some(sock);
            publish(state, true, port, *deck_time);
        }
        Err(e) => {
            eprintln!("opendrop-io: rkbx_link bind on port {port} failed: {e}");
            *socket = None;
            publish(state, false, 0, [None; MAX_DJ_DECKS]);
        }
    }
}

enum RkbxMessage {
    TrackTitle { deck: usize, title: String },
    TrackArtist { deck: usize, artist: String },
    Time { deck: usize, seconds: f64 },
}

fn decode_packet(buf: &[u8]) -> Option<RkbxMessage> {
    let (_, packet) = rosc::decoder::decode_udp(buf).ok()?;
    let OscPacket::Message(msg) = packet else { return None };
    decode_message(&msg.addr, &msg.args)
}

/// Pure address/argument logic (mirrors `io::osc::dispatch_from_message`'s
/// factoring), so it's unit-testable without a socket. Address shape is
/// `/{deck}/track/title`, `/{deck}/track/artist`, or `/{deck}/time`; `deck`
/// is a bare 0-based decimal index. Anything else — an unparseable/
/// out-of-range deck index, `/{deck}/track/album`, `/master/*`, `/{deck}/bpm`
/// etc. — falls through to `None` (silently ignored), matching rkbx_link's
/// own "messages for any other address are ignored for v1" scoping: no
/// special-casing of `album` or the `/master/*` aliases is needed, they are
/// simply never matched by any arm below.
fn decode_message(addr: &str, args: &[OscType]) -> Option<RkbxMessage> {
    let rest = addr.strip_prefix('/')?;
    let (deck_str, tail) = rest.split_once('/')?;
    let deck: usize = deck_str.parse().ok()?;
    if deck >= MAX_DJ_DECKS {
        return None;
    }
    match tail {
        "track/title" => {
            let OscType::String(title) = args.first()? else { return None };
            Some(RkbxMessage::TrackTitle { deck, title: title.clone() })
        }
        "track/artist" => {
            let OscType::String(artist) = args.first()? else { return None };
            Some(RkbxMessage::TrackArtist { deck, artist: artist.clone() })
        }
        "time" => {
            let OscType::Float(seconds) = args.first()? else { return None };
            Some(RkbxMessage::Time { deck, seconds: *seconds as f64 })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_title_is_recognized_for_deck_0_and_deck_3() {
        assert!(matches!(
            decode_message("/0/track/title", &[OscType::String("One More Time".into())]),
            Some(RkbxMessage::TrackTitle { deck: 0, title }) if title == "One More Time"
        ));
        assert!(matches!(
            decode_message("/3/track/title", &[OscType::String("One More Time".into())]),
            Some(RkbxMessage::TrackTitle { deck: 3, title }) if title == "One More Time"
        ));
    }

    #[test]
    fn track_artist_is_recognized() {
        assert!(matches!(
            decode_message("/1/track/artist", &[OscType::String("Daft Punk".into())]),
            Some(RkbxMessage::TrackArtist { deck: 1, artist }) if artist == "Daft Punk"
        ));
    }

    #[test]
    fn time_is_recognized() {
        assert!(matches!(decode_message("/2/time", &[OscType::Float(12.5)]), Some(RkbxMessage::Time { deck: 2, seconds }) if seconds == 12.5));
    }

    #[test]
    fn a_non_numeric_deck_segment_is_ignored() {
        assert!(decode_message("/master/time", &[OscType::Float(1.0)]).is_none());
    }

    #[test]
    fn an_out_of_range_deck_index_is_ignored() {
        assert!(decode_message("/4/time", &[OscType::Float(1.0)]).is_none());
    }

    #[test]
    fn wrong_osc_type_is_ignored_for_each_address() {
        assert!(decode_message("/0/track/title", &[OscType::Float(1.0)]).is_none());
        assert!(decode_message("/0/track/artist", &[OscType::Float(1.0)]).is_none());
        assert!(decode_message("/0/time", &[OscType::String("nope".into())]).is_none());
    }

    #[test]
    fn album_is_ignored() {
        assert!(decode_message("/0/track/album", &[OscType::String("Discovery".into())]).is_none());
    }

    #[test]
    fn an_address_with_no_deck_segment_is_ignored() {
        assert!(decode_message("/time", &[OscType::Float(1.0)]).is_none());
    }

    #[test]
    fn decode_packet_round_trips_each_recognized_address() {
        let title = OscPacket::Message(rosc::OscMessage { addr: "/0/track/title".to_string(), args: vec![OscType::String("Around the World".into())] });
        let bytes = rosc::encoder::encode(&title).expect("valid packet encodes");
        assert!(matches!(decode_packet(&bytes), Some(RkbxMessage::TrackTitle { deck: 0, .. })));

        let artist = OscPacket::Message(rosc::OscMessage { addr: "/0/track/artist".to_string(), args: vec![OscType::String("Daft Punk".into())] });
        let bytes = rosc::encoder::encode(&artist).expect("valid packet encodes");
        assert!(matches!(decode_packet(&bytes), Some(RkbxMessage::TrackArtist { deck: 0, .. })));

        let time = OscPacket::Message(rosc::OscMessage { addr: "/0/time".to_string(), args: vec![OscType::Float(3.5)] });
        let bytes = rosc::encoder::encode(&time).expect("valid packet encodes");
        assert!(matches!(decode_packet(&bytes), Some(RkbxMessage::Time { deck: 0, .. })));
    }

    #[test]
    fn decode_packet_on_garbage_bytes_does_not_panic() {
        assert!(decode_packet(&[0xff, 0x00, 0x13, 0x37]).is_none());
        assert!(decode_packet(&[]).is_none());
    }

    #[test]
    fn fresh_snapshot_is_idle() {
        let s = RkbxLinkSnapshot::idle();
        assert!(!s.listening);
        assert_eq!(s.port, 0);
        assert_eq!(s.deck_time, [None; MAX_DJ_DECKS]);
    }

    /// Adapted from `io::osc`'s own regression test: `bind()` must drop the
    /// old socket before attempting the new one, or an unconditional
    /// same-port restart fails with `EADDRINUSE`.
    #[test]
    fn bind_can_rebind_the_same_port_without_erroring() {
        let probe = UdpSocket::bind(("0.0.0.0", 0)).expect("OS has a free ephemeral port");
        let port = probe.local_addr().expect("bound socket has a local addr").port();
        drop(probe);

        let state = Arc::new(ArcSwap::from_pointee(RkbxLinkSnapshot::idle()));
        let mut socket: Option<UdpSocket> = None;
        let deck_time = [None; MAX_DJ_DECKS];

        bind(&mut socket, port, &state, &deck_time);
        assert!(socket.is_some(), "first bind on {port} should succeed");
        assert!(state.load().listening);
        assert_eq!(state.load().port, port);

        bind(&mut socket, port, &state, &deck_time);
        assert!(socket.is_some(), "rebind on the same port {port} should also succeed");
        assert!(state.load().listening);
        assert_eq!(state.load().port, port);
    }
}
