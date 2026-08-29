//! The Twitch IRC chat client thread: connect/disconnect to a Twitch
//! channel's chat and forward every incoming message onto the shared chat
//! stream. Mirrors `main.cjs:429-457`'s `twitch:connect`/`twitch:disconnect`
//! IPC handlers (there implemented with `tmi.js`; here with the
//! purpose-built `twitch-irc` crate: see PHASE5-IO.PLAN's Task 17
//! découpage note for why that crate over the generic `irc` one).
//!
//! Same dedicated-thread-with-its-own-`current_thread`-runtime pattern as
//! `obs`/`remote_ws`: `spawn()` returns immediately, the thread never
//! leaves its runtime, `control_tx`/`latest()` are the only things `app`
//! touches (no tokio type in `AppState`).
//!
//! Unlike `obs`'s lenient "connect without a password if none is stored",
//! a missing or unreadable Twitch OAuth token REFUSES the connection
//! outright, with no attempt made: Twitch chat requires authenticating as
//! *some* account to join, so there is no equivalent to OBS's "connect
//! anonymously" fallback. Mirrors `main.cjs:431-432`'s `if (!oauthToken)
//! return { ok: false, error: 'No Twitch token registered.' }`.
//!
//! `twitch_irc::TwitchIRCClient::new` returns a
//! `(mpsc::UnboundedReceiver<ServerMessage>, TwitchIRCClient)` pair; a
//! dedicated `tokio::spawn`ed task (not the main control loop, which stays
//! a plain `while let Some(msg) = async_rx.recv().await` like `obs::run`'s,
//! since there's no long-running accept loop to `select!` against here
//! either) drains that receiver for the lifetime of one connection,
//! translating `ServerMessage::Privmsg` into `ChatMessage` and forwarding
//! it to the shared `chat_tx`. Its `JoinHandle` is stored so `Disconnect`
//! (or a `Connect` superseding an existing connection) can `.abort()` it
//! immediately, rather than relying purely on the crate's own "drop all
//! client handles to shut down" contract (documented on
//! `TwitchIRCClient`, `client/mod.rs`'s module doc comment) to eventually
//! stop it: both happen (the client handle is dropped too), but the abort
//! makes the cutover deterministic instead of racing background shutdown
//! timing. No generation-guard counter is needed here (contrast
//! `kick::KickHandle`'s doc comment): `twitch-irc` has a real, clean
//! shutdown path, so there is no stale-listener problem to guard against.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use arc_swap::ArcSwap;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::ServerMessage;
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient};

use crate::chat::{ChatMessage, ChatPlatform};
use crate::secrets;

/// Continuous state published via `TwitchHandle::latest()`: never blocks,
/// always the latest known value (mirrors `ObsSnapshot`).
pub struct TwitchSnapshot {
    pub connected: bool,
    /// Set when `Connect` is refused (no OAuth token registered, or a
    /// keyring lookup failure): whole-branch review Finding 1 (AC-12):
    /// this used to be an `eprintln!` only, invisible to a GUI user.
    /// Rendered in the Streaming panel. Cleared by a subsequent successful
    /// `Connect` or by `Disconnect`.
    pub last_error: Option<String>,
}

impl TwitchSnapshot {
    pub fn idle() -> Self {
        TwitchSnapshot { connected: false, last_error: None }
    }
}

/// Outward control messages sent to the Twitch thread.
pub enum TwitchControl {
    /// Connects to Twitch IRC and joins `channel` (also the identity used
    /// to log in: Twitch IRC's `username` is the bot/channel account,
    /// matching `main.cjs`'s `identity: { username: channel, password:
    /// oauthToken }`). Unconditional, even if already connected: drops any
    /// existing connection first, same as `obs::ObsControl::Connect`'s doc
    /// comment.
    Connect(String),
    /// Disconnects, if connected. A no-op while already idle.
    Disconnect,
}

/// Handle to the running Twitch thread. Mirrors `ObsHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block.
pub struct TwitchHandle {
    state: Arc<ArcSwap<TwitchSnapshot>>,
    pub control_tx: Sender<TwitchControl>,
}

impl TwitchHandle {
    /// Never blocks: an atomic load of the current Arc (mirrors
    /// `ObsHandle::latest`).
    pub fn latest(&self) -> Arc<TwitchSnapshot> {
        self.state.load_full()
    }
}

/// Spawns the dedicated Twitch thread and returns immediately. The thread
/// starts idle (`connected: false`) until it receives `TwitchControl::
/// Connect`: mirrors `ObsHandle::spawn`'s "starts idle" pattern.
///
/// `chat_tx` is a clone of the one shared channel `app`'s bootstrap creates
/// for both `twitch::spawn` and `kick::spawn` (see `chat`'s module doc
/// comment): every `ServerMessage::Privmsg` this thread receives is
/// translated to a `ChatMessage` and pushed there.
pub fn spawn(chat_tx: Sender<ChatMessage>) -> TwitchHandle {
    let state = Arc::new(ArcSwap::from_pointee(TwitchSnapshot::idle()));
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, control_rx, chat_tx)
    });
    TwitchHandle { state, control_tx }
}

fn publish_idle(state: &Arc<ArcSwap<TwitchSnapshot>>, last_error: Option<String>) {
    state.store(Arc::new(TwitchSnapshot { last_error, ..TwitchSnapshot::idle() }));
}

fn publish_connected(state: &Arc<ArcSwap<TwitchSnapshot>>) {
    state.store(Arc::new(TwitchSnapshot { connected: true, last_error: None }));
}

/// Builds and runs this thread's own tokio runtime for its entire
/// lifetime. Never panics: a runtime-build failure is logged once and the
/// thread exits immediately (leaving `state` at its initial idle value):
/// mirrors `obs::run`.
fn run(state: Arc<ArcSwap<TwitchSnapshot>>, control_rx: Receiver<TwitchControl>, chat_tx: Sender<ChatMessage>) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("opendrop-io: twitch failed to build tokio runtime: {e}");
            return;
        }
    };
    rt.block_on(async_run(state, control_rx, chat_tx));
}

async fn async_run(state: Arc<ArcSwap<TwitchSnapshot>>, control_rx: Receiver<TwitchControl>, chat_tx: Sender<ChatMessage>) {
    // Bridges the synchronous `control_rx` into an async-friendly channel,
    // once, for this thread's whole lifetime: see `remote_ws::async_run`'s
    // doc comment for the full rationale. Ends (and the loop below with
    // it) once `control_tx` is dropped, i.e. `TwitchHandle` is dropped.
    let (async_tx, mut async_rx) = tokio::sync::mpsc::unbounded_channel::<TwitchControl>();
    tokio::task::spawn_blocking(move || {
        while let Ok(msg) = control_rx.recv() {
            if async_tx.send(msg).is_err() {
                break; // async side gone
            }
        }
    });

    // The message-forwarding task, if any: held across loop iterations so
    // `Disconnect` can act on the same session `Connect` established.
    // `None` means idle, mirroring `osc::run`'s `Option<UdpSocket>`. The
    // `TwitchIRCClient` handle itself is moved into the task (see the
    // `Connect` arm below) rather than kept in a separate outer variable:
    // it's never read back (there's no `SendMessage`-style feature here to
    // use it for), only held for its RAII effect: dropping the last
    // `TwitchIRCClient` handle is what ends the connection (`client/
    // mod.rs`'s "Close the client" doc comment): so `task.abort()` alone
    // both stops the forwarding loop AND drops the client, no separate
    // `client = None` needed.
    let mut forward_task: Option<tokio::task::JoinHandle<()>> = None;

    while let Some(msg) = async_rx.recv().await {
        match msg {
            TwitchControl::Connect(channel) => {
                // Unconditional restart: abort any existing connection's
                // task (dropping its client with it) before creating the
                // new one, same as `osc::bind`'s doc comment.
                if let Some(task) = forward_task.take() {
                    task.abort();
                }

                match oauth_token_for_connect(secrets::get_secret(secrets::TWITCH_OAUTH_TOKEN)) {
                    Ok(oauth_token) => {
                        let config = ClientConfig::new_simple(StaticLoginCredentials::new(channel.clone(), Some(oauth_token)));
                        let (mut incoming, new_client) = TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);
                        match new_client.join(channel) {
                            Ok(()) => {
                                let chat_tx = chat_tx.clone();
                                forward_task = Some(tokio::spawn(async move {
                                    // Keeps the connection alive for the
                                    // task's lifetime: see the comment on
                                    // `forward_task` above.
                                    let _client_keep_alive = new_client;
                                    while let Some(server_msg) = incoming.recv().await {
                                        if let ServerMessage::Privmsg(p) = server_msg {
                                            let _ = chat_tx.send(ChatMessage {
                                                platform: ChatPlatform::Twitch,
                                                user_id: p.sender.id,
                                                username: p.sender.name,
                                                content: p.message_text,
                                            });
                                        }
                                    }
                                }));
                                publish_connected(&state);
                            }
                            Err(e) => {
                                eprintln!("opendrop-io: twitch join failed: {e}");
                                publish_idle(&state, None);
                                // new_client (and incoming) drop here.
                            }
                        }
                    }
                    Err(e) => {
                        // Whole-branch review Finding 1 (AC-12): this
                        // refusal used to be an `eprintln!` only, invisible
                        // to a GUI user: now also surfaced via
                        // `TwitchSnapshot::last_error`.
                        eprintln!("opendrop-io: twitch connect refused: {e}");
                        publish_idle(&state, Some(e));
                    }
                }
            }
            TwitchControl::Disconnect => {
                if let Some(task) = forward_task.take() {
                    task.abort(); // drops the client held inside: see the comment on `forward_task` above
                }
                publish_idle(&state, None);
            }
        }
    }
}

/// Maps a `secrets::get_secret` result to the OAuth token used to connect,
/// or an explicit refusal reason. Unlike `obs::password_for_connect`'s
/// lenient "missing means connect anyway" behavior, `Ok(None)` (no token
/// registered) and `Err` (a keyring lookup failure) are both refused here:
/// mirrors the JS reference's `if (!oauthToken) return { ok: false, error:
/// 'No Twitch token registered.' }` (`main.cjs:431-432`).
fn oauth_token_for_connect(result: Result<Option<String>, String>) -> Result<String, String> {
    match result {
        Ok(Some(token)) => Ok(token),
        Ok(None) => Err("No Twitch token registered.".to_string()),
        Err(e) => Err(format!("Twitch token lookup failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_snapshot_is_idle() {
        let s = TwitchSnapshot::idle();
        assert!(!s.connected);
        assert!(s.last_error.is_none());
    }

    #[test]
    fn stored_token_is_passed_through() {
        assert_eq!(oauth_token_for_connect(Ok(Some("abc123".to_string()))), Ok("abc123".to_string()));
    }

    #[test]
    fn no_stored_token_refuses_connection() {
        assert_eq!(oauth_token_for_connect(Ok(None)), Err("No Twitch token registered.".to_string()));
    }

    #[test]
    fn keyring_lookup_failure_refuses_connection() {
        let err = oauth_token_for_connect(Err("no Secret Service daemon running".to_string()));
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("no Secret Service daemon running"));
    }
}
