//! The OBS-WebSocket-v5 client thread: connect/disconnect and one-way
//! app->OBS scene control. Mirrors `main.cjs:376-400`'s `obs:connect` /
//! `obs:disconnect` / `obs:get-scenes` / `obs:set-scene` IPC handlers.
//!
//! App->OBS direction only: `main.cjs`'s `obs.on('CurrentProgramSceneChanged',
//! ...)` (OBS->app) is deliberately NOT ported: nothing in the existing
//! app consumes that event, so there is no event-listening/broadcast path
//! here at all.
//!
//! Same async-thread-per-integration pattern as `remote_ws`: a dedicated
//! `std::thread` builds its own single-threaded tokio runtime and
//! never leaves it for the thread's whole lifetime, bridging the
//! synchronous `control_tx`/`control_rx` into the async world once via
//! `tokio::task::spawn_blocking` (see that module's doc comment for the
//! full architecture writeup): no shared runtime, no tokio type in
//! `AppState`.
//!
//! Unlike `remote_ws`, there is no long-running accept loop to race a
//! control message against: `obws::Client` calls are one-shot request/
//! response RPCs, so the run loop is a plain `while let Some(msg) =
//! async_rx.recv().await { ... }` handling one control message to
//! completion at a time: no `select!` needed.
//!
//! Never panics on a connect/RPC error: logged once. Any such error
//! (whether the initial connect, the post-connect `GetSceneList`, or a
//! later `SetCurrentProgramScene`) drops the held `obws::Client` (if any)
//! and publishes an idle snapshot (`connected: false`), matching `osc::
//! run`'s "a real recv error drops the socket and reports not-listening"
//! convention: an RPC error on an established connection is treated the
//! same as a dead connection, not as a transient hiccup to retry silently.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::secrets;

/// Continuous state published via `ObsHandle::latest()`: never blocks,
/// always the latest known value (mirrors `RemoteWsSnapshot`).
pub struct ObsSnapshot {
    pub connected: bool,
    /// Scene names, mirroring `main.cjs`'s `obs:get-scenes` handler's
    /// `scenes.map((s) => s.sceneName)`. Fetched once, right after a
    /// successful `Connect` (there is no separate `ObsControl` variant to
    /// re-fetch it later: see `ObsControl::Connect`'s doc comment).
    /// Empty while not connected.
    pub scenes: Vec<String>,
    /// Set when the OBS password keyring lookup fails during `Connect`
    /// (AC-12: this used to be an `eprintln!` only, invisible to a GUI
    /// user). Rendered in the
    /// Streaming panel. Cleared by a subsequent successful `Connect` or by
    /// `Disconnect`; carried through this same `Connect` attempt's outcome
    /// (success or failure) rather than dropped, so it isn't lost before
    /// the user ever sees it.
    pub last_error: Option<String>,
}

impl ObsSnapshot {
    pub fn idle() -> Self {
        ObsSnapshot { connected: false, scenes: Vec::new(), last_error: None }
    }
}

/// Outward control messages sent to the OBS thread.
pub enum ObsControl {
    /// Connects to `ws://{host}:{port}` and, on success, immediately fetches
    /// the scene list (`GetSceneList`) to populate `ObsSnapshot::scenes`:
    /// folding together what the JS reference exposes as two separate IPC
    /// calls (`obs:connect` then `obs:get-scenes`), since this thread's
    /// snapshot always carries both fields together. The OBS WebSocket
    /// password is looked up from the OS keyring at call time (see
    /// `password_for_connect`), never taken from this message.
    ///
    /// Unconditional, even if already connected: drops any existing
    /// connection first, same as `osc::OscControl::Start`'s doc comment.
    Connect(String, u16),
    /// Disconnects, if connected. A no-op while already idle.
    Disconnect,
    /// Sets the current program scene by name (`SetCurrentProgramScene`).
    /// A no-op while not connected: mirrors a control message arriving
    /// with nothing to act on, same as `OscControl::Stop` while idle.
    SetScene(String),
}

/// Handle to the running OBS thread. Mirrors `RemoteWsHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block. No `events`
/// field: see the module doc comment, app->OBS direction only.
pub struct ObsHandle {
    state: Arc<ArcSwap<ObsSnapshot>>,
    pub control_tx: Sender<ObsControl>,
}

impl ObsHandle {
    /// Never blocks: an atomic load of the current Arc (mirrors
    /// `RemoteWsHandle::latest`).
    pub fn latest(&self) -> Arc<ObsSnapshot> {
        self.state.load_full()
    }
}

/// Spawns the dedicated OBS thread and returns immediately. The thread
/// starts idle (`connected: false`, no scenes) until it receives
/// `ObsControl::Connect`: mirrors `RemoteWsHandle::spawn`'s "starts idle"
/// pattern.
pub fn spawn() -> ObsHandle {
    let state = Arc::new(ArcSwap::from_pointee(ObsSnapshot::idle()));
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, control_rx)
    });
    ObsHandle { state, control_tx }
}

fn publish_idle(state: &Arc<ArcSwap<ObsSnapshot>>, last_error: Option<String>) {
    state.store(Arc::new(ObsSnapshot { last_error, ..ObsSnapshot::idle() }));
}

fn publish_connected(state: &Arc<ArcSwap<ObsSnapshot>>, scenes: Vec<String>, last_error: Option<String>) {
    state.store(Arc::new(ObsSnapshot { connected: true, scenes, last_error }));
}

/// Builds and runs this thread's own tokio runtime for its entire
/// lifetime. Never panics: a runtime-build failure is logged once and the
/// thread exits immediately (leaving `state` at its initial idle value,
/// same as never having started): mirrors `remote_ws::run`.
fn run(state: Arc<ArcSwap<ObsSnapshot>>, control_rx: Receiver<ObsControl>) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("opendrop-io: obs failed to build tokio runtime: {e}");
            return;
        }
    };
    rt.block_on(async_run(state, control_rx));
}

async fn async_run(state: Arc<ArcSwap<ObsSnapshot>>, control_rx: Receiver<ObsControl>) {
    // Bridges the synchronous `control_rx` into an async-friendly channel,
    // once, for this thread's whole lifetime: see `remote_ws::async_run`'s
    // doc comment for the full rationale (cancel-safe, reusable across
    // iterations). Ends (and the loop below with it) once `control_tx` is
    // dropped, i.e. `ObsHandle` is dropped.
    let (async_tx, mut async_rx) = tokio::sync::mpsc::unbounded_channel::<ObsControl>();
    tokio::task::spawn_blocking(move || {
        while let Ok(msg) = control_rx.recv() {
            if async_tx.send(msg).is_err() {
                break; // async side gone
            }
        }
    });

    // The live connection, if any: held across loop iterations so
    // `SetScene`/`Disconnect` can act on the same session `Connect`
    // established. `None` means idle, mirroring `osc::run`'s `Option<
    // UdpSocket>`.
    let mut client: Option<obws::Client> = None;

    while let Some(msg) = async_rx.recv().await {
        match msg {
            ObsControl::Connect(host, port) => {
                // Unconditional restart: drop any existing connection
                // before creating the new one, same as `osc::bind`'s doc
                // comment.
                if let Some(mut old) = client.take() {
                    old.disconnect().await;
                }
                let secret_result = secrets::get_secret(secrets::OBS_PASSWORD);
                // Captured (not just logged) per AC-12: a keyring lookup
                // failure used to be an `eprintln!` only, invisible to a
                // GUI user launched from a desktop environment. Still
                // lenient (the connect attempt below proceeds without a password either way), but now
                // surfaced via `ObsSnapshot::last_error` regardless of
                // whether that attempt goes on to succeed or fail.
                let keyring_error = secret_result.as_ref().err().map(|e| {
                    eprintln!("opendrop-io: obs password lookup failed, connecting without a password: {e}");
                    format!("OBS password lookup failed, connecting without a password: {e}")
                });
                let password = password_for_connect(secret_result);
                match obws::Client::connect(host, port, password).await {
                    Ok(new_client) => match new_client.scenes().list().await {
                        Ok(scene_list) => {
                            let names = scene_list.scenes.into_iter().map(|s| s.id.name).collect();
                            client = Some(new_client);
                            publish_connected(&state, names, keyring_error);
                        }
                        Err(e) => {
                            eprintln!("opendrop-io: obs GetSceneList failed: {e}");
                            publish_idle(&state, keyring_error);
                            // new_client drops here: its own Drop impl
                            // aborts the background read task.
                        }
                    },
                    Err(e) => {
                        eprintln!("opendrop-io: obs connect failed: {e}");
                        publish_idle(&state, keyring_error);
                    }
                }
            }
            ObsControl::Disconnect => {
                if let Some(mut c) = client.take() {
                    c.disconnect().await;
                }
                publish_idle(&state, None);
            }
            ObsControl::SetScene(name) => {
                // `None`: not connected, no-op, mirrors OscControl::Stop while idle.
                if let Some(c) = client.as_ref() {
                    if let Err(e) = c.scenes().set_current_program_scene(name.as_str()).await {
                        eprintln!("opendrop-io: obs SetCurrentProgramScene failed: {e}");
                        client = None; // drop() aborts the background task, see above
                        publish_idle(&state, None);
                    }
                }
            }
        }
    }
}

/// Maps a `secrets::get_secret` result to the password `obws::Client::
/// connect` is called with. `Ok(None)` (no password stored) and `Err`
/// (a keyring lookup failure, e.g. no Secret Service daemon running,
/// which `secrets`'s module doc comment flags as a real possibility on
/// this project's minimal Hyprland dev session) are both mapped to `None`,
/// matching the JS reference's lenient `secretsStore.getSecret('obs-
/// password') || undefined`: neither should abort the connect attempt.
/// Only `Ok(Some(_))` yields an actual password.
///
/// Pure on purpose (no logging here): the `Err` case is logged once by
/// `ObsControl::Connect`'s handler, right before calling this, so the
/// distinction between "no password configured" and "couldn't check" isn't
/// lost: only the password value itself is discarded here.
fn password_for_connect(result: Result<Option<String>, String>) -> Option<String> {
    result.ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_snapshot_is_idle() {
        let s = ObsSnapshot::idle();
        assert!(!s.connected);
        assert!(s.scenes.is_empty());
        assert!(s.last_error.is_none());
    }

    #[test]
    fn stored_password_is_passed_through() {
        assert_eq!(password_for_connect(Ok(Some("hunter2".to_string()))), Some("hunter2".to_string()));
    }

    #[test]
    fn no_stored_password_yields_none() {
        assert_eq!(password_for_connect(Ok(None)), None);
    }

    #[test]
    fn keyring_lookup_failure_yields_none_not_abort() {
        assert_eq!(password_for_connect(Err("no Secret Service daemon running".to_string())), None);
    }
}
