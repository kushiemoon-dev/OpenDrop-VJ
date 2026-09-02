//! The remote-WS I/O thread: a single axum HTTP server that serves the
//! phone-remote-control SPA (`OpenDrop-VJ/src/routes/remote/+page.svelte`,
//! ported once as static assets under `io/assets/remote/`, not touched
//! here) AND accepts the WebSocket command channel that page opens, both on
//! the same OS-assigned port. Mirrors `main.cjs:182-247`'s
//! `remote:start`/`remote:stop`/`ws.on('message', ...)` protocol exactly:
//! JSON `{token, cmd, value}`, silently dropped on a token mismatch, `cmd`
//! resolved via `command_names::parse_command_id`, `value` clamped 0..1.
//!
//! Unlike `osc`/`midi` (plain `std::thread` doing blocking I/O directly),
//! this is the first *async* integration in this codebase: the dedicated
//! `std::thread` spawned by `spawn()` builds its own single-threaded tokio
//! runtime and never leaves it for the thread's whole lifetime: "every
//! async integration builds its own tokio runtime inside its own
//! dedicated thread", no shared runtime, no tokio type in `AppState`
//! (`RemoteWsHandle`'s public fields are the same `std::sync::mpsc` types
//! `OscHandle`/`MidiHandle` use, so `app` never has to know tokio exists).
//!
//! Bridging the synchronous `control_tx`/`control_rx` (used by `app`'s
//! non-async egui code, same as OSC/MIDI) into the async world happens
//! once, via a single `tokio::task::spawn_blocking` loop that forwards
//! every `RemoteWsControl` onto a `tokio::sync::mpsc::UnboundedReceiver`
//! the async run loop can `.recv().await` (cancel-safe, reusable across
//! `tokio::select!` iterations: see `serve_until_stopped`). This lives for
//! the thread's whole life, not per-connection.
//!
//! Never panics on bind/serve failure: logged once, `listening` left
//! `false` (same contract as `osc::bind`).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use opendrop_core::commands::CommandId;
use rand::Rng;
use tower_http::services::{ServeDir, ServeFile};

use crate::command_names::parse_command_id;

/// Continuous state published via `RemoteWsHandle::latest()`: never
/// blocks, always the latest known value (mirrors `OscSnapshot`).
pub struct RemoteWsSnapshot {
    pub listening: bool,
    pub port: u16,
    /// First non-loopback IPv4 found on the machine (mirrors `getLanIp`,
    /// `main.cjs:186-193`), for display in the panel: `None` if none was
    /// found (deliberately not a `127.0.0.1` fallback like the JS
    /// reference: that address would never actually work for a phone on
    /// the LAN, so showing "not detected" is more honest than showing a
    /// dead address). Re-resolved on every start, same as the JS
    /// reference's `getLanIp()` call inside `remote:start`.
    pub ip: Option<String>,
    /// Hex-encoded 12 random bytes, regenerated on every start (including
    /// a restart that interrupts an already-running server: see
    /// `serve_until_stopped`'s `Start`-while-serving branch). Empty while
    /// not listening.
    pub token: String,
}

impl RemoteWsSnapshot {
    pub fn idle() -> Self {
        RemoteWsSnapshot { listening: false, port: 0, ip: None, token: String::new() }
    }
}

/// Outward control messages sent to the remote-WS thread.
pub enum RemoteWsControl {
    /// (Re)binds an OS-assigned port and starts serving. Unconditional,
    /// even if already listening: mirrors `main.cjs`'s `if (wsServer) {
    /// wsServer.close(); wsServer = null }` before creating the new one,
    /// same as `osc::OscControl::Start`'s doc comment.
    Start,
    /// Stops serving, if listening. A no-op while already idle.
    Stop,
}

/// Handle to the running remote-WS thread. Mirrors `OscHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block, `events` and
/// `control_tx` are public fields the app's panel/`about_to_wait` drain and
/// send through directly.
pub struct RemoteWsHandle {
    state: Arc<ArcSwap<RemoteWsSnapshot>>,
    pub events: Receiver<(CommandId, f64)>,
    pub control_tx: Sender<RemoteWsControl>,
}

impl RemoteWsHandle {
    /// Never blocks: an atomic load of the current Arc (mirrors
    /// `OscHandle::latest`).
    pub fn latest(&self) -> Arc<RemoteWsSnapshot> {
        self.state.load_full()
    }
}

/// The static-asset directory: `io/assets/remote` resolved relative to
/// this crate's own `Cargo.toml` at *build* time via `CARGO_MANIFEST_DIR`.
/// This bakes the build machine's absolute path into the binary: fine for
/// dev (the brief is explicit packaging-time path resolution is out of
/// scope here, deferred to Phase 6), not meant to survive relocating the
/// binary to a different machine.
fn assets_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/remote"))
}

/// Spawns the dedicated remote-WS thread and returns immediately. The
/// thread starts idle (no server running, `listening: false`) until it
/// receives `RemoteWsControl::Start`: mirrors `OscHandle::spawn`'s
/// "starts idle" pattern.
pub fn spawn() -> RemoteWsHandle {
    let state = Arc::new(ArcSwap::from_pointee(RemoteWsSnapshot::idle()));
    let (events_tx, events_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, events_tx, control_rx)
    });
    RemoteWsHandle { state, events: events_rx, control_tx }
}

fn publish_idle(state: &Arc<ArcSwap<RemoteWsSnapshot>>) {
    state.store(Arc::new(RemoteWsSnapshot::idle()));
}

fn publish_listening(state: &Arc<ArcSwap<RemoteWsSnapshot>>, port: u16, ip: Option<String>, token: String) {
    state.store(Arc::new(RemoteWsSnapshot { listening: true, port, ip, token }));
}

/// Builds and runs this thread's own tokio runtime for its entire
/// lifetime. Never panics: a runtime-build failure is logged once and the
/// thread exits immediately (leaving `state` at its initial idle value,
/// same as never having started).
fn run(state: Arc<ArcSwap<RemoteWsSnapshot>>, events_tx: Sender<(CommandId, f64)>, control_rx: Receiver<RemoteWsControl>) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("opendrop-io: remote-ws failed to build tokio runtime: {e}");
            return;
        }
    };
    rt.block_on(async_run(state, events_tx, control_rx));
}

async fn async_run(state: Arc<ArcSwap<RemoteWsSnapshot>>, events_tx: Sender<(CommandId, f64)>, control_rx: Receiver<RemoteWsControl>) {
    // Bridges the synchronous `control_rx` into an async-friendly channel,
    // once, for this thread's whole lifetime: see the module doc comment.
    // Ends (and the loop below with it) once `control_tx` is dropped, i.e.
    // `RemoteWsHandle` is dropped.
    let (async_tx, mut async_rx) = tokio::sync::mpsc::unbounded_channel::<RemoteWsControl>();
    tokio::task::spawn_blocking(move || {
        while let Ok(msg) = control_rx.recv() {
            if async_tx.send(msg).is_err() {
                break; // async side gone
            }
        }
    });

    loop {
        match async_rx.recv().await {
            Some(RemoteWsControl::Start) => serve_until_stopped(&state, &events_tx, &mut async_rx).await,
            Some(RemoteWsControl::Stop) => {} // already idle, no-op
            None => break,                    // RemoteWsHandle dropped: shut down.
        }
    }
}

/// Binds an OS-assigned port and serves until a `Stop` (or the handle
/// being dropped) is received, then returns. A `Start` received while
/// already serving restarts unconditionally (fresh token, fresh
/// OS-assigned port) instead of returning: mirrors the JS reference's
/// unconditional close-then-recreate.
///
/// Never panics on bind/serve failure: logged once, `state` published back
/// to idle, and this function returns (control is back with the caller's
/// loop, which goes back to waiting on the next control message).
async fn serve_until_stopped(
    state: &Arc<ArcSwap<RemoteWsSnapshot>>,
    events_tx: &Sender<(CommandId, f64)>,
    control_rx: &mut tokio::sync::mpsc::UnboundedReceiver<RemoteWsControl>,
) {
    loop {
        let listener = match tokio::net::TcpListener::bind(("0.0.0.0", 0)).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("opendrop-io: remote-ws bind failed: {e}");
                publish_idle(state);
                return;
            }
        };
        let port = match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(e) => {
                eprintln!("opendrop-io: remote-ws local_addr failed: {e}");
                publish_idle(state);
                return;
            }
        };
        let token = generate_token();
        let ip = first_lan_ipv4();
        publish_listening(state, port, ip, token.clone());

        // Fresh per generation (i.e. per loop pass / per `Start`): every
        // connection accepted during this generation subscribes its own
        // receiver in `handle_socket`. Dropping the `axum::serve` future
        // below (the control-message branch winning the `select!`) stops
        // the accept loop and closes the listener, but does NOT touch
        // already-open per-connection tasks: `axum::serve` spawns each
        // accepted connection as an independent tokio task, detached from
        // the `Serve` future itself. Broadcasting on `shutdown_tx` after
        // that branch wins is what actually closes those already-open
        // connections, so a stale token stops being usable the moment
        // `Stop`/restart is requested, not just "stops accepting new
        // connections under it".
        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
        let app = build_router(token, events_tx.clone(), shutdown_tx.clone());

        // `axum::serve`'s future resolves to `io::Result<()>` but per its
        // own docs never actually completes on its own (transient accept
        // errors are retried internally with a short sleep): the only
        // realistic way out of this `select!` is the control-message arm.
        // Cancel-safe: dropping the losing branch here drops `listener`
        // (moved into `axum::serve` by value), closing the socket before
        // any rebind on the `Start` branch below.
        tokio::select! {
            biased;
            msg = control_rx.recv() => {
                publish_idle(state);
                // No receivers subscribed yet (nobody ever connected) is a
                // normal, harmless `Err` here: ignored, same as every
                // other outward "best effort" send in this codebase.
                let _ = shutdown_tx.send(());
                match msg {
                    Some(RemoteWsControl::Start) => continue, // unconditional restart
                    Some(RemoteWsControl::Stop) | None => return,
                }
            }
            result = axum::serve(listener, app) => {
                if let Err(e) = result {
                    eprintln!("opendrop-io: remote-ws serve error: {e}");
                }
                publish_idle(state);
                let _ = shutdown_tx.send(());
                return;
            }
        }
    }
}

/// Hex-encoded 12 random bytes, freshly generated: never reused across
/// restarts (called once per `serve_until_stopped` loop pass, i.e. once
/// per `Start`).
fn generate_token() -> String {
    let mut bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// First non-loopback IPv4 among all network interfaces: mirrors
/// `getLanIp`, `main.cjs:186-193` (`family === 'IPv4' && !iface.internal`).
/// Deliberately not `local_ip_address::local_ip()` (which picks the
/// outbound-routing interface via a UDP connect trick and could land on a
/// VPN/tunnel interface): the brief asks for the exact `getLanIp` mirror,
/// enumerate-and-filter, not "whichever interface has a default route".
fn first_lan_ipv4() -> Option<String> {
    let ifaces = local_ip_address::list_afinet_netifas().ok()?;
    ifaces.into_iter().find_map(|(_name, ip)| match ip {
        std::net::IpAddr::V4(v4) if !v4.is_loopback() => Some(v4.to_string()),
        _ => None,
    })
}

fn build_router(token: String, events_tx: Sender<(CommandId, f64)>, shutdown_tx: tokio::sync::broadcast::Sender<()>) -> Router {
    let assets = assets_dir();
    let index_html = assets.join("index.html");
    Router::new()
        .route("/", get(ws_handler))
        .with_state(WsState { token, events_tx, shutdown_tx })
        .fallback_service(ServeDir::new(assets).fallback(ServeFile::new(index_html)))
}

#[derive(Clone)]
struct WsState {
    token: String,
    events_tx: Sender<(CommandId, f64)>,
    /// Broadcasts once when this generation (this `serve_until_stopped`
    /// loop pass) is being stopped or restarted: see that function's doc
    /// comment. Each connection subscribes its own receiver in
    /// `handle_socket`.
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

/// Mounted at `/` (root, no path segment) to match `+page.svelte`'s
/// `new WebSocket(\`ws://${host}:${port}\`)` exactly: that page is never
/// rewritten (REQUIREMENTS.md), so the server side has to meet it where it
/// is. A plain (non-upgrade) GET to `/` gets axum's standard
/// `WebSocketUpgrade` rejection response instead of the SPA's `index.html`
///: acceptable here: the phone is always given a `/remote?host=&port=&
/// token=` URL, never a bare `/`, and everything other than the exact `/`
/// path (including `/remote` and `/_app/...`) falls through to the
/// `fallback_service` below.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<WsState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Races the next WS message against `state.shutdown_tx` so a `Stop`/restart
/// on the server side closes this connection promptly instead of leaving it
/// running under a stale (already-superseded) token: see
/// `serve_until_stopped`'s doc comment for why this is needed at all
/// (`axum::serve` dropping its future does not touch already-open
/// connections, which are independent tokio tasks by the time they're
/// spawned).
async fn handle_socket(mut socket: WebSocket, state: WsState) {
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.recv() => return, // server stopping/restarting: drop the connection.
            msg = socket.recv() => {
                let Some(Ok(msg)) = msg else { return }; // closed or errored: same as the old `while let` exit.
                if let Message::Text(text) = msg {
                    if let Some(dispatch) = parse_remote_message(text.as_str(), &state.token) {
                        let _ = state.events_tx.send(dispatch);
                    }
                }
                // Binary/Ping/Pong/Close frames: nothing to do: mirrors the
                // JS reference, which only ever handles `ws.on('message', ...)`
                // (text/JSON) and lets the `ws` library's own automatic
                // ping/pong and close handling take care of the rest.
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct RemoteMessage {
    token: String,
    cmd: String,
    #[serde(default)]
    value: f64,
}

/// Pure JSON/token/command logic, factored out of `handle_socket` so it's
/// unit-testable without a real socket (mirrors `osc::dispatch_from_message`).
/// Returns `None`: silently ignored, no error response, so a probing
/// client learns nothing from a mismatch: for: malformed JSON, a
/// `token` that doesn't match `expected_token`, or an unrecognized `cmd`
/// (via `parse_command_id`, same kebab-case table OSC uses). `value` is
/// clamped 0..1, defaulting to 0.0 when the field is absent from the JSON
/// (mirrors `main.cjs`'s `typeof msg.value === 'number' ? ... : 0`).
fn parse_remote_message(text: &str, expected_token: &str) -> Option<(CommandId, f64)> {
    let msg: RemoteMessage = serde_json::from_str(text).ok()?;
    if msg.token != expected_token {
        return None;
    }
    let id = parse_command_id(&msg.cmd)?;
    Some((id, msg.value.clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_token_and_known_command_dispatches() {
        let json = r#"{"token":"abc123","cmd":"crossfader","value":0.5}"#;
        assert_eq!(parse_remote_message(json, "abc123"), Some((CommandId::Crossfader, 0.5)));
    }

    #[test]
    fn mismatched_token_is_silently_ignored() {
        let json = r#"{"token":"wrong","cmd":"crossfader","value":0.5}"#;
        assert_eq!(parse_remote_message(json, "abc123"), None);
    }

    #[test]
    fn unrecognized_command_is_silently_ignored() {
        let json = r#"{"token":"abc123","cmd":"not-a-real-command","value":0.5}"#;
        assert_eq!(parse_remote_message(json, "abc123"), None);
    }

    #[test]
    fn malformed_json_is_silently_ignored() {
        assert_eq!(parse_remote_message("not json", "abc123"), None);
        assert_eq!(parse_remote_message("", "abc123"), None);
    }

    #[test]
    fn value_above_one_is_clamped() {
        let json = r#"{"token":"abc123","cmd":"crossfader","value":1.7}"#;
        assert_eq!(parse_remote_message(json, "abc123"), Some((CommandId::Crossfader, 1.0)));
    }

    #[test]
    fn value_below_zero_is_clamped() {
        let json = r#"{"token":"abc123","cmd":"crossfader","value":-0.3}"#;
        assert_eq!(parse_remote_message(json, "abc123"), Some((CommandId::Crossfader, 0.0)));
    }

    #[test]
    fn missing_value_defaults_to_zero() {
        let json = r#"{"token":"abc123","cmd":"strobe-toggle"}"#;
        assert_eq!(parse_remote_message(json, "abc123"), Some((CommandId::StrobeToggle, 0.0)));
    }

    #[test]
    fn missing_token_field_is_silently_ignored() {
        let json = r#"{"cmd":"crossfader","value":0.5}"#;
        assert_eq!(parse_remote_message(json, "abc123"), None);
    }

    #[test]
    fn generate_token_is_24_hex_chars_and_varies_across_calls() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 24); // 12 bytes, hex-encoded
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "token must be regenerated (not reused) on every start");
    }

    #[test]
    fn fresh_snapshot_is_idle() {
        let s = RemoteWsSnapshot::idle();
        assert!(!s.listening);
        assert_eq!(s.port, 0);
        assert_eq!(s.ip, None);
        assert_eq!(s.token, "");
    }

    /// Polls `handle.latest()` until `listening` is true (or panics after
    /// 5s): shared by every test below that needs a real bound server.
    fn wait_until_listening(handle: &RemoteWsHandle) -> Arc<RemoteWsSnapshot> {
        use std::time::Duration;

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let s = handle.latest();
            if s.listening {
                return s;
            }
            assert!(std::time::Instant::now() < deadline, "server never reported listening");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Real end-to-end smoke test: spawns the thread, starts it, connects
    /// a real WS client to the OS-assigned port, sends a valid command,
    /// and checks it comes out the other end on `events`. Exercises the
    /// tokio-runtime-in-a-thread wiring and the router itself (not just
    /// the pure `parse_remote_message` logic above), at the cost of a real
    /// bound socket: kept to one such test, deliberately, for speed.
    #[test]
    fn spawned_server_dispatches_a_real_ws_message() {
        use std::time::Duration;

        let handle = spawn();
        let _ = handle.control_tx.send(RemoteWsControl::Start);
        let snapshot = wait_until_listening(&handle);

        let url = format!("ws://127.0.0.1:{}/", snapshot.port);
        let (mut socket, _resp) =
            tungstenite::connect(url).expect("connect to the just-started remote-ws server");
        let payload = format!(r#"{{"token":"{}","cmd":"crossfader","value":0.75}}"#, snapshot.token);
        socket.send(tungstenite::Message::Text(payload.into())).expect("send WS message");

        let (id, value01) = handle.events.recv_timeout(Duration::from_secs(5)).expect("dispatch arrives");
        assert_eq!(id, CommandId::Crossfader);
        assert_eq!(value01, 0.75);
    }

    /// Regression test for the review finding: `Stop` must actually
    /// terminate already-open connections, not just stop accepting new
    /// ones: otherwise a phone connected before `Stop` keeps dispatching
    /// commands under the now-superseded token forever. Connects a real
    /// client, proves it's live (a command dispatches), sends `Stop`,
    /// then proves the connection actually closes (the client's blocking
    /// `read()` returns an error/close instead of hanging) AND that a
    /// message sent afterwards never reaches `events`.
    #[test]
    fn stop_closes_an_already_open_connection() {
        use std::time::Duration;

        let handle = spawn();
        let _ = handle.control_tx.send(RemoteWsControl::Start);
        let snapshot = wait_until_listening(&handle);

        let url = format!("ws://127.0.0.1:{}/", snapshot.port);
        let (mut socket, _resp) = tungstenite::connect(url).expect("connect");

        // Prove the connection is live before stopping.
        let live_payload = format!(r#"{{"token":"{}","cmd":"crossfader","value":0.25}}"#, snapshot.token);
        socket.send(tungstenite::Message::Text(live_payload.into())).expect("send before stop");
        let (id, value01) = handle.events.recv_timeout(Duration::from_secs(5)).expect("dispatch arrives before stop");
        assert_eq!(id, CommandId::Crossfader);
        assert_eq!(value01, 0.25);

        let _ = handle.control_tx.send(RemoteWsControl::Stop);

        // Tungstenite's blocking `read()` returns once the connection is
        // closed (cleanly or not): it must not hang past a generous
        // deadline, proving the server side actually dropped this
        // now-stale connection rather than leaving it open.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match socket.read() {
                Err(_) => break,                             // connection actually terminated
                Ok(tungstenite::Message::Close(_)) => break, // clean close frame
                Ok(_) => {}                                  // ignore anything else, keep reading
            }
            assert!(std::time::Instant::now() < deadline, "connection was not closed after Stop");
        }

        // A message sent on the now-closed connection must never dispatch
        //: proves the stale token really did stop being usable, not just
        // that the socket eventually noticed a close on its own.
        let stale_payload = format!(r#"{{"token":"{}","cmd":"crossfader","value":0.9}}"#, snapshot.token);
        let _ = socket.send(tungstenite::Message::Text(stale_payload.into())); // expected to fail; ignored either way
        assert!(
            handle.events.recv_timeout(Duration::from_millis(500)).is_err(),
            "a stale connection must not still be dispatching after Stop"
        );
    }
}
