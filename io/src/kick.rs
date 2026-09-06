//! The Kick chat client thread: an unofficial, reverse-engineered protocol
//! (no official Kick API/crate exists: confirmed, no crates.io result for
//! a Kick client) ported directly from the compiled `@retconned/kick-js`
//! npm package's `dist/index.cjs` (installed under
//! `OpenDrop-VJ/node_modules/@retconned/kick-js`, read at implementation
//! time: every URL/header/message-shape decision below cites the exact
//! line traced there). Mirrors `main.cjs:460-514`'s `kick:connect`/
//! `kick:disconnect` IPC handlers.
//!
//! # Protocol, as traced from `@retconned/kick-js@0.5.4`'s compiled source
//!
//! `login({type: 'tokens', credentials: {bearerToken, xsrfToken, cookies}})`
//! (`client.ts`, compiled `index.cjs:438-450`) stores the 3 credentials and
//! calls `initialize()` (`index.cjs:460-527`), which:
//!
//! 1. Fetches `https://kick.com/api/v2/channels/{channel}` (`kickApi.ts`,
//!    `index.cjs:55-60`) to resolve the channel name to a chatroom ID
//!    (`channelInfo.chatroom.id`). **Ported here as a plain `reqwest` GET**:
//!    the JS reference instead drives a headless Puppeteer browser (with
//!    a stealth plugin) at this URL specifically because the endpoint sits
//!    behind Cloudflare bot-protection (`index.cjs:61-65`: `if (response
//!    ?.status() === 403) throw new Error('Request blocked by Cloudflare
//!    protection...')`) and a real browser fingerprint is needed to pass
//!    it. **This is the single biggest unverified assumption in this
//!    module**: a plain `reqwest` request has no such fingerprint and may
//!    simply be blocked where Puppeteer would succeed: there is no way to
//!    replicate a full headless-browser TLS/JS fingerprint with `reqwest`
//!    alone, and no headless-browser crate was introduced here (out of
//!    scope for an I/O crate, and verifying this against the real service
//!    wasn't attempted). A realistic `User-Agent` is sent as the only
//!    mitigation attempted.
//! 2. Opens a WebSocket to Kick's third-party Pusher app instance:
//!    `wss://ws-us2.pusher.com/app/32cbd69e4b950bf97679?protocol=7&client=
//!    js&version=8.4.0&flash=false` (`websocket.ts`, `index.cjs:224-232`)
//!    and, once open, sends `{"event":"pusher:subscribe","data":{"auth":
//!    "","channel":"chatrooms.{chatroomId}.v2"}}` (`index.cjs:235-239`):
//!    an **empty** `auth` field, meaning this Pusher channel is public: the
//!    3 Kick secrets are never actually attached to this WS handshake or
//!    subscribe frame in the JS reference either.
//! 3. Incoming WS frames are a Pusher envelope `{"event": "...", "data":
//!    "<json-encoded-string>"}`: `data` is JSON-*encoded text*, not a
//!    nested object, requiring a second `JSON.parse` (`index.cjs:277-283`).
//!    A chat message carries `event: "App\\Events\\ChatMessageEvent"`; its
//!    decoded `data` is `{id, chatroom_id, content, type, created_at,
//!    sender: {id: number, username: string, slug, identity}}` (the
//!    `MessageData` interface, `index.d.cts:161-186`): `sender.id`/
//!    `sender.username`/`content` are exactly the 3 fields `main.cjs:487`
//!    reads off `message.sender.id`/`message.sender.username`/
//!    `message.content`.
//!
//! So: the 3 Kick secrets (`kick-bearer-token`/`kick-xsrf-token`/
//! `kick-cookies`) are required and checked for presence here (mirroring
//! `main.cjs:472-473`'s `if (!bearerToken || !xsrfToken || !cookies)
//! return {...}` and `client.ts`'s `checkAuth()`/`isLoggedIn` gate) but,
//! per the trace above, are **not actually sent** on either the discovery
//! GET or the WS subscribe frame for this read-only chat-listening
//! feature: same as the JS reference's own `login('tokens')` ->
//! `initialize()` path. They are still required and validated here, both
//! to match the JS reference's refusal behavior byte-for-byte and because
//! a real Kick account's credentials are exactly what `checkAuth()` guards
//! (a future authenticated write path, sending messages, banning, would
//! need them for real; none of that is implemented here, mirroring
//! `obs`'s app->OBS-only, no-write-path scope note).
//!
//! # Generation-guard: not ported, real cancellation used instead
//!
//! The JS reference's `kickGeneration: AtomicU64`-equivalent counter
//! (`main.cjs:423,472-494`) exists purely because `@retconned/kick-js`
//! exposes no `disconnect()`/`off()`: confirmed by reading the object
//! `createClient()` returns (`index.cjs:813-829`: `{login, on, user, vod,
//! sendMessage, banUser, unbanUser, deleteMessage, slowMode, getPoll,
//! getLeaderboards}`, no close/disconnect/unsubscribe method at all), so a
//! `kick:disconnect` can only ever *pretend* the old listener stopped by
//! having it self-check a generation counter: the underlying socket and
//! its `'ChatMessage'` listener keep running forever.
//!
//! This Rust port owns the `tokio-tungstenite` connection directly instead
//! of going through an unclosable third-party client object, so that
//! problem doesn't exist here: the WS-reading loop runs in its own
//! `tokio::spawn`ed task, and its `JoinHandle` is stored so `Disconnect`
//! (or a `Connect` superseding an existing one) can `.abort()` it, which
//! drops the `WebSocketStream` (and with it the underlying TCP/TLS socket)
//! immediately. A generation counter would only add complexity for a
//! problem that doesn't exist on this side of the port: real
//! cancellation is strictly better here, so it's what's used (same
//! stored-`JoinHandle`-plus-`.abort()` shape as `twitch::async_run`'s
//! forwarding task, for the same reason).
//!
//! Same dedicated-thread-with-its-own-`current_thread`-runtime pattern as
//! `obs`/`twitch`/`remote_ws`.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use arc_swap::ArcSwap;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::chat::{ChatMessage, ChatPlatform};
use crate::secrets;

const PUSHER_WS_URL: &str = "wss://ws-us2.pusher.com/app/32cbd69e4b950bf97679?protocol=7&client=js&version=8.4.0&flash=false";

/// A realistic desktop-browser User-Agent, sent on the discovery GET as the
/// only attempted mitigation against Cloudflare bot-protection: see this
/// module's doc comment for why this is not expected to reliably work
/// (the JS reference uses a full headless browser for this exact reason).
const DISCOVERY_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Continuous state published via `KickHandle::latest()`: never blocks,
/// always the latest known value (mirrors `ObsSnapshot`/`TwitchSnapshot`).
pub struct KickSnapshot {
    pub connected: bool,
    /// Set when `Connect` is refused (missing bearer/xsrf/cookies, or a
    /// keyring lookup failure). AC-12:
    /// this used to be an `eprintln!` only, invisible to a GUI user.
    /// Rendered in the Streaming panel. Cleared by a subsequent successful
    /// `Connect` or by `Disconnect`.
    pub last_error: Option<String>,
}

impl KickSnapshot {
    pub fn idle() -> Self {
        KickSnapshot { connected: false, last_error: None }
    }
}

/// Outward control messages sent to the Kick thread.
pub enum KickControl {
    /// Resolves `channel`'s chatroom ID and connects to its Pusher chat
    /// WebSocket. Unconditional, even if already connected: aborts any
    /// existing connection's task first, same as `obs::ObsControl::
    /// Connect`'s doc comment.
    Connect(String),
    /// Disconnects, if connected. A no-op while already idle.
    Disconnect,
}

/// Handle to the running Kick thread. Mirrors `TwitchHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block.
pub struct KickHandle {
    state: Arc<ArcSwap<KickSnapshot>>,
    pub control_tx: Sender<KickControl>,
}

impl KickHandle {
    /// Never blocks: an atomic load of the current Arc (mirrors
    /// `TwitchHandle::latest`).
    pub fn latest(&self) -> Arc<KickSnapshot> {
        self.state.load_full()
    }
}

/// Spawns the dedicated Kick thread and returns immediately. The thread
/// starts idle (`connected: false`) until it receives `KickControl::
/// Connect`: mirrors `TwitchHandle::spawn`'s "starts idle" pattern.
///
/// `chat_tx` is a clone of the one shared channel `app`'s bootstrap creates
/// for both `twitch::spawn` and `kick::spawn`: see `chat`'s module doc
/// comment.
pub fn spawn(chat_tx: Sender<ChatMessage>) -> KickHandle {
    let state = Arc::new(ArcSwap::from_pointee(KickSnapshot::idle()));
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, control_rx, chat_tx)
    });
    KickHandle { state, control_tx }
}

fn publish_idle(state: &Arc<ArcSwap<KickSnapshot>>, last_error: Option<String>) {
    state.store(Arc::new(KickSnapshot { last_error, ..KickSnapshot::idle() }));
}

fn publish_connected(state: &Arc<ArcSwap<KickSnapshot>>) {
    state.store(Arc::new(KickSnapshot { connected: true, last_error: None }));
}

/// Builds and runs this thread's own tokio runtime for its entire
/// lifetime. Never panics: a runtime-build failure is logged once and the
/// thread exits immediately (leaving `state` at its initial idle value):
/// mirrors `obs::run`/`twitch::run`.
fn run(state: Arc<ArcSwap<KickSnapshot>>, control_rx: Receiver<KickControl>, chat_tx: Sender<ChatMessage>) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("opendrop-io: kick failed to build tokio runtime: {e}");
            return;
        }
    };
    rt.block_on(async_run(state, control_rx, chat_tx));
}

async fn async_run(state: Arc<ArcSwap<KickSnapshot>>, control_rx: Receiver<KickControl>, chat_tx: Sender<ChatMessage>) {
    // Bridges the synchronous `control_rx` into an async-friendly channel,
    // once, for this thread's whole lifetime: see `remote_ws::async_run`'s
    // doc comment for the full rationale. Ends (and the loop below with
    // it) once `control_tx` is dropped, i.e. `KickHandle` is dropped.
    let (async_tx, mut async_rx) = tokio::sync::mpsc::unbounded_channel::<KickControl>();
    tokio::task::spawn_blocking(move || {
        while let Ok(msg) = control_rx.recv() {
            if async_tx.send(msg).is_err() {
                break; // async side gone
            }
        }
    });

    // The running WS-read task, if any: its `JoinHandle` is the real
    // cancellation mechanism this module's doc comment describes (no
    // client object to hold onto, unlike `client: Option<...>` in
    // `obs`/`twitch`).
    let mut ws_task: Option<tokio::task::JoinHandle<()>> = None;

    while let Some(msg) = async_rx.recv().await {
        match msg {
            KickControl::Connect(channel) => {
                // Unconditional restart: abort any existing connection's
                // task before creating the new one, same as `osc::bind`'s
                // doc comment.
                if let Some(task) = ws_task.take() {
                    task.abort();
                }

                match check_kick_credentials(
                    secrets::get_secret(secrets::KICK_BEARER_TOKEN),
                    secrets::get_secret(secrets::KICK_XSRF_TOKEN),
                    secrets::get_secret(secrets::KICK_COOKIES),
                ) {
                    Ok(()) => match discover_chatroom_id(&channel).await {
                        Ok(chatroom_id) => match tokio_tungstenite::connect_async(PUSHER_WS_URL).await {
                            Ok((ws, _response)) => {
                                let chat_tx = chat_tx.clone();
                                let task_state = state.clone();
                                ws_task = Some(tokio::spawn(async move {
                                    run_ws(ws, chatroom_id, chat_tx, task_state).await;
                                }));
                                publish_connected(&state);
                            }
                            Err(e) => {
                                eprintln!("opendrop-io: kick websocket connect failed: {e}");
                                publish_idle(&state, None);
                            }
                        },
                        Err(e) => {
                            eprintln!("opendrop-io: kick channel discovery failed: {e}");
                            publish_idle(&state, None);
                        }
                    },
                    Err(e) => {
                        // AC-12: this
                        // refusal used to be an `eprintln!` only, invisible
                        // to a GUI user: now also surfaced via
                        // `KickSnapshot::last_error`.
                        eprintln!("opendrop-io: kick connect refused: {e}");
                        publish_idle(&state, Some(e));
                    }
                }
            }
            KickControl::Disconnect => {
                if let Some(task) = ws_task.take() {
                    task.abort();
                }
                publish_idle(&state, None);
            }
        }
    }
}

/// Checks all 3 Kick secrets are present, mirroring the JS reference's
/// `if (!bearerToken || !xsrfToken || !cookies) return { ok: false, error:
/// 'Missing Kick credentials (token + xsrf + cookies).' }` gate
/// (`main.cjs:472-473`). Refuses on ANY missing secret or keyring lookup
/// failure: same "can't proceed without knowing" reasoning as `twitch::
/// oauth_token_for_connect`. See this module's doc comment for why the
/// values themselves aren't threaded any further once this check passes.
fn check_kick_credentials(
    bearer: Result<Option<String>, String>,
    xsrf: Result<Option<String>, String>,
    cookies: Result<Option<String>, String>,
) -> Result<(), String> {
    for result in [&bearer, &xsrf, &cookies] {
        if let Err(e) = result {
            return Err(format!("Kick credentials lookup failed: {e}"));
        }
    }
    if bearer.unwrap().is_some() && xsrf.unwrap().is_some() && cookies.unwrap().is_some() {
        Ok(())
    } else {
        Err("Missing Kick credentials (token + xsrf + cookies).".to_string())
    }
}

#[derive(serde::Deserialize)]
struct KickChannelInfo {
    chatroom: KickChatroom,
}

#[derive(serde::Deserialize)]
struct KickChatroom {
    id: u64,
}

/// Resolves `channel`'s chatroom ID via `GET https://kick.com/api/v2/
/// channels/{channel}`: see this module's doc comment for the Cloudflare
/// caveat around this specific call.
async fn discover_chatroom_id(channel: &str) -> Result<u64, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!("https://kick.com/api/v2/channels/{channel}"))
        .header(reqwest::header::USER_AGENT, DISCOVERY_USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Kick channel lookup returned HTTP {}", response.status()));
    }

    let info: KickChannelInfo = response.json().await.map_err(|e| e.to_string())?;
    Ok(info.chatroom.id)
}

#[derive(serde::Deserialize)]
struct PusherEnvelope {
    event: String,
    #[serde(default)]
    data: Option<String>,
}

#[derive(serde::Deserialize)]
struct KickSender {
    id: u64,
    username: String,
}

#[derive(serde::Deserialize)]
struct KickChatMessageData {
    content: String,
    sender: KickSender,
}

/// Parses one incoming Pusher WS text frame into a `ChatMessage`, or
/// `None` for anything that isn't a chat message (subscription
/// acks/pings/other event types: the JS reference's `default: console.log
/// ('Unknown event type', ...); return null` branch, `index.cjs:327-329`,
/// is the same "ignore anything unrecognized" behavior). Pure and
/// side-effect-free so it's unit-testable without a real socket.
fn parse_kick_chat_message(text: &str) -> Option<ChatMessage> {
    let envelope: PusherEnvelope = serde_json::from_str(text).ok()?;
    if envelope.event != "App\\Events\\ChatMessageEvent" {
        return None;
    }
    let data: KickChatMessageData = serde_json::from_str(&envelope.data?).ok()?;
    Some(ChatMessage {
        platform: ChatPlatform::Kick,
        user_id: data.sender.id.to_string(),
        username: data.sender.username,
        content: data.content,
    })
}

type KickWs = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Sends the Pusher `pusher:subscribe` frame for `chatroom_id`'s chat
/// channel, then forwards every parsed chat message to `chat_tx` until the
/// socket closes or errors (or this task is `.abort()`ed by `Disconnect`/
/// a superseding `Connect`: see this module's doc comment). Publishes
/// `idle` on its own when the loop ends on its own (server-closed socket,
/// read error): mirrors `obs::run`'s "an RPC error on an established
/// connection is treated the same as a dead connection" convention.
async fn run_ws(mut ws: KickWs, chatroom_id: u64, chat_tx: Sender<ChatMessage>, state: Arc<ArcSwap<KickSnapshot>>) {
    let subscribe = serde_json::json!({
        "event": "pusher:subscribe",
        "data": { "auth": "", "channel": format!("chatrooms.{chatroom_id}.v2") }
    })
    .to_string();
    if let Err(e) = ws.send(Message::text(subscribe)).await {
        eprintln!("opendrop-io: kick pusher subscribe failed: {e}");
        publish_idle(&state, None);
        return;
    }

    while let Some(next) = ws.next().await {
        match next {
            Ok(Message::Text(text)) => {
                if let Some(chat_message) = parse_kick_chat_message(&text) {
                    let _ = chat_tx.send(chat_message);
                }
            }
            Ok(_) => {} // ping/pong/binary/close frames: no handler for these, matches the JS reference only acting on parsed chat events
            Err(e) => {
                eprintln!("opendrop-io: kick websocket read error: {e}");
                break;
            }
        }
    }
    publish_idle(&state, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_snapshot_is_idle() {
        let s = KickSnapshot::idle();
        assert!(!s.connected);
        assert!(s.last_error.is_none());
    }

    #[test]
    fn all_three_secrets_present_is_ok() {
        assert_eq!(
            check_kick_credentials(Ok(Some("b".to_string())), Ok(Some("x".to_string())), Ok(Some("c".to_string()))),
            Ok(())
        );
    }

    #[test]
    fn any_missing_secret_refuses_connection() {
        assert!(check_kick_credentials(Ok(None), Ok(Some("x".to_string())), Ok(Some("c".to_string()))).is_err());
        assert!(check_kick_credentials(Ok(Some("b".to_string())), Ok(None), Ok(Some("c".to_string()))).is_err());
        assert!(check_kick_credentials(Ok(Some("b".to_string())), Ok(Some("x".to_string())), Ok(None)).is_err());
    }

    #[test]
    fn keyring_lookup_failure_refuses_connection() {
        let err = check_kick_credentials(Err("no Secret Service daemon running".to_string()), Ok(Some("x".to_string())), Ok(Some("c".to_string())));
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("no Secret Service daemon running"));
    }

    #[test]
    fn parses_chat_message_event() {
        let inner = serde_json::json!({
            "id": "msg-1",
            "chatroom_id": 123,
            "content": "hello chat",
            "type": "message",
            "created_at": "2026-01-01T00:00:00Z",
            "sender": { "id": 456, "username": "someviewer", "slug": "someviewer", "identity": { "color": "#fff", "badges": [] } }
        })
        .to_string();
        let envelope = serde_json::json!({
            "event": "App\\Events\\ChatMessageEvent",
            "data": inner
        })
        .to_string();

        let msg = parse_kick_chat_message(&envelope).expect("should parse");
        assert_eq!(msg.platform, ChatPlatform::Kick);
        assert_eq!(msg.user_id, "456");
        assert_eq!(msg.username, "someviewer");
        assert_eq!(msg.content, "hello chat");
    }

    #[test]
    fn ignores_non_chat_events() {
        let envelope = serde_json::json!({
            "event": "pusher_internal:subscription_succeeded",
            "data": "{}"
        })
        .to_string();
        assert!(parse_kick_chat_message(&envelope).is_none());
    }

    #[test]
    fn ignores_malformed_json() {
        assert!(parse_kick_chat_message("not json").is_none());
    }
}
