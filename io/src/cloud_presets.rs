//! CloudPresets: a REST client for the CloudPresets backend Worker
//! (`workers/presets-cloud/` in the web app's repo, see the `legacy-web`
//! branch, `src/index.ts`+`src/handlers.ts`, already deployed) plus the dedicated
//! background thread that drives it from the UI panel
//! (`app::ui::cloud_presets`) without blocking egui's render loop.
//!
//! Same async-thread-per-integration pattern as `obs`/`remote_ws` (see
//! `io::obs`'s module doc comment for the full architecture writeup): a
//! dedicated `std::thread` builds its own single-threaded tokio runtime and
//! never leaves it, bridging the synchronous `control_tx`/`control_rx` into
//! the async world once via `tokio::task::spawn_blocking`. Every
//! `CloudPresetsClient` call here is a one-shot request/response RPC (no
//! persistent connection to hold across control messages, unlike OBS's
//! `obws::Client`), so the run loop is a plain `while let Some(msg) =
//! async_rx.recv().await { ... }`, same as `obs::async_run`.
//!
//! Identity: one anonymous token per device (`secrets::CLOUD_PRESETS_TOKEN`
//! in the OS keyring), generated lazily on first use if none is stored yet.
//! It ports `cloud-presets.ts`'s `getOrCreateCloudToken()` (`crypto.
//! randomUUID()` + `localStorage`), except the random value itself is 16
//! hex-encoded random bytes via `rand` (mirrors `remote_ws::
//! generate_token`'s technique) rather than an actual RFC 4122 UUID: no
//! `uuid` crate is a direct workspace dependency, and the token is an
//! opaque bearer value on both ends (the Worker only ever uses it as an R2
//! key prefix, `handlers.ts`'s `presets/${token}/...`), never parsed as a
//! real UUID anywhere.
//!
//! `CLOUD_PRESET_PREFIX` ("☁ ") is prepended to the name on every upload/
//! rename call, exactly mirroring `cloud-presets.ts:72,115`: the server
//! stores (and returns) names with the prefix already baked in, it is not
//! a display-only client-side decoration.
//!
//! Preset format: cloud presets are Butterchurn JSON (`cloud-presets.ts`'s
//! `parsePresetFile` doc comment: "the uploaded file must already be in
//! Butterchurn format"), the web engine's own preset format, not this
//! native app's `.milk`/projectM format. This app's loader only reaches
//! projectM through `projectm_load_preset_file`/`projectm_load_preset_data`
//! (both take `.milk` text), and no Butterchurn->`.milk` converter exists
//! or is scoped anywhere in this 14-task plan: confirmed, not merely
//! unverified; building one is explicitly out of scope for this step. So
//! `Download` here only writes the raw JSON to a local cache file; it does
//! not attempt to insert anything into `Show::preset_catalog` or make the
//! download playable on a deck. `ui::cloud_presets` (the panel) surfaces
//! this gap directly to the end user (a `warn_banner` above the preset
//! list), not just here.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use arc_swap::ArcSwap;
use rand::Rng;

use crate::secrets;

/// Sole anti-collision guard against the ~16k static local preset names:
/// mirrors `OpenDrop-VJ/src/lib/engine/cloud-presets.ts:18` byte-for-byte.
/// Prepended by `CloudPresetsClient::upload`/`rename`, never by the caller.
pub const CLOUD_PRESET_PREFIX: &str = "☁ ";

/// One entry in the cloud index: mirrors `workers/presets-cloud/src/
/// handlers.ts`'s `IndexEntry` (also `CloudPresetEntry` in the web client)
/// field-for-field, including the wire's camelCase names. `Serialize` is
/// only needed by this module's own mock-server tests (the real Worker is
/// what serializes this on the wire, this client only ever deserializes
/// it), kept on the real type rather than a test-only shadow struct.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: u64,
    /// Milliseconds since the Unix epoch (`Date.now()` on the Worker side).
    #[serde(rename = "uploadedAt")]
    pub uploaded_at: i64,
}

#[derive(serde::Deserialize)]
struct ErrorBody {
    error: String,
}

#[derive(serde::Deserialize)]
struct UploadResponse {
    id: String,
}

/// REST client for one `base_url` and `token` pair. Stateless: every
/// method opens its own `reqwest::Client`, same `reqwest::Client`,
/// `.header(...)`, `.json().await` pattern as `kick.rs:306-321`'s
/// `discover_chatroom_id`: the one difference is that a non-2xx response
/// here first tries to decode the Worker's `{error: string}` body
/// (`handlers.ts` always returns one on a non-2xx), falling back to a
/// plain `HTTP {status}` message only if that decode fails, which
/// `kick.rs`'s target endpoint has no equivalent of.
pub struct CloudPresetsClient {
    base_url: String,
    token: String,
}

impl CloudPresetsClient {
    pub fn new(base_url: String, token: String) -> Self {
        Self { base_url, token }
    }

    fn presets_url(&self) -> String {
        format!("{}/presets", self.base_url.trim_end_matches('/'))
    }

    fn preset_url(&self, id: &str) -> String {
        format!("{}/presets/{id}", self.base_url.trim_end_matches('/'))
    }

    /// Maps a non-2xx `Response` to an error message, preferring the
    /// Worker's decoded `{error: string}` body (`handlers.ts`'s error
    /// shape) over a generic status line.
    async fn error_from_response(response: reqwest::Response) -> String {
        let status = response.status();
        match response.json::<ErrorBody>().await {
            Ok(body) => body.error,
            Err(_) => format!("HTTP {status}"),
        }
    }

    pub async fn list(&self) -> Result<Vec<IndexEntry>, String> {
        let client = reqwest::Client::new();
        let response = client
            .get(self.presets_url())
            .header("X-Cloud-Token", &self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(Self::error_from_response(response).await);
        }
        response.json().await.map_err(|e| e.to_string())
    }

    /// Downloads the raw JSON body of preset `id`: `GET /presets/:id`
    /// returns the preset's data verbatim (an arbitrary Butterchurn JSON
    /// object, not a fixed Rust shape), so this returns the raw text
    /// rather than deserializing it into anything.
    pub async fn get(&self, id: &str) -> Result<String, String> {
        let client = reqwest::Client::new();
        let response = client
            .get(self.preset_url(id))
            .header("X-Cloud-Token", &self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err("preset not found".to_string());
        }
        if !response.status().is_success() {
            return Err(Self::error_from_response(response).await);
        }
        response.text().await.map_err(|e| e.to_string())
    }

    /// `data` is the picked file's raw JSON text: parsed here only to
    /// validate it is a JSON object (mirrors `parsePresetFile`'s "must be
    /// a JSON object, not an array" check) before being embedded under the
    /// request body's `data` field. `name` is prefixed with
    /// `CLOUD_PRESET_PREFIX` here, exactly like `uploadPreset`,
    /// `cloud-presets.ts:72`.
    pub async fn upload(&self, name: &str, data: &str) -> Result<String, String> {
        let data_value: serde_json::Value = serde_json::from_str(data).map_err(|e| format!("invalid preset JSON: {e}"))?;
        if !data_value.is_object() {
            return Err("invalid preset file: must be a JSON object".to_string());
        }
        let body = serde_json::json!({ "name": format!("{CLOUD_PRESET_PREFIX}{name}"), "data": data_value });
        let client = reqwest::Client::new();
        let response = client
            .post(self.presets_url())
            .header("X-Cloud-Token", &self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(Self::error_from_response(response).await);
        }
        let parsed: UploadResponse = response.json().await.map_err(|e| e.to_string())?;
        Ok(parsed.id)
    }

    /// `name` is prefixed with `CLOUD_PRESET_PREFIX` here, exactly like
    /// `renameCloudPreset`, `cloud-presets.ts:115`.
    pub async fn rename(&self, id: &str, name: &str) -> Result<(), String> {
        let client = reqwest::Client::new();
        let response = client
            .patch(self.preset_url(id))
            .header("X-Cloud-Token", &self.token)
            .json(&serde_json::json!({ "name": format!("{CLOUD_PRESET_PREFIX}{name}") }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(Self::error_from_response(response).await);
        }
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<(), String> {
        let client = reqwest::Client::new();
        let response = client
            .delete(self.preset_url(id))
            .header("X-Cloud-Token", &self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(Self::error_from_response(response).await);
        }
        Ok(())
    }
}

/// Continuous state published via `CloudPresetsHandle::latest()`.
#[derive(Clone)]
pub struct CloudPresetsSnapshot {
    pub entries: Vec<IndexEntry>,
    pub busy: bool,
    pub last_error: Option<String>,
    /// Set after a successful `Download`: the local cache file the raw
    /// preset JSON was written to. See this module's doc comment for why
    /// nothing beyond writing that file happens here.
    pub last_downloaded: Option<PathBuf>,
}

impl CloudPresetsSnapshot {
    pub fn idle() -> Self {
        CloudPresetsSnapshot { entries: Vec::new(), busy: false, last_error: None, last_downloaded: None }
    }
}

/// Outward control messages sent to the CloudPresets thread. `base_url`
/// travels on every variant (rather than a separate one-time "configure"
/// message) since there is no persistent connection to hold open between
/// calls: each is an independent one-shot RPC, and the caller (the panel)
/// already has `base_url` at hand from its own `AppState` field.
pub enum CloudPresetsControl {
    List { base_url: String },
    Upload { base_url: String, name: String, data: String },
    Rename { base_url: String, id: String, name: String },
    Delete { base_url: String, id: String },
    Download { base_url: String, id: String },
}

/// Handle to the running CloudPresets thread. Mirrors `ObsHandle`'s shape:
/// `latest()` never blocks, `control_tx` sends never block.
pub struct CloudPresetsHandle {
    state: Arc<ArcSwap<CloudPresetsSnapshot>>,
    pub control_tx: Sender<CloudPresetsControl>,
}

impl CloudPresetsHandle {
    /// Never blocks: an atomic load of the current Arc (mirrors
    /// `ObsHandle::latest`).
    pub fn latest(&self) -> Arc<CloudPresetsSnapshot> {
        self.state.load_full()
    }
}

/// Spawns the dedicated CloudPresets thread and returns immediately. The
/// thread starts idle (no entries, no error) until it receives its first
/// control message: mirrors `ObsHandle::spawn`'s "starts idle" pattern.
pub fn spawn() -> CloudPresetsHandle {
    let state = Arc::new(ArcSwap::from_pointee(CloudPresetsSnapshot::idle()));
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, control_rx)
    });
    CloudPresetsHandle { state, control_tx }
}

/// Replaces the published snapshot with a copy mutated by `f`: every
/// control-message handler below goes through this rather than
/// constructing a whole new `CloudPresetsSnapshot` literal by hand each
/// time, since (unlike `ObsSnapshot`'s binary connected/idle shape)
/// several fields here need to survive untouched across most transitions
/// (e.g. `entries` during a `Download`).
fn update(state: &Arc<ArcSwap<CloudPresetsSnapshot>>, f: impl FnOnce(&mut CloudPresetsSnapshot)) {
    let mut next = (*state.load_full()).clone();
    f(&mut next);
    state.store(Arc::new(next));
}

/// Builds and runs this thread's own tokio runtime for its entire
/// lifetime. Never panics: a runtime-build failure is logged once and the
/// thread exits immediately (leaving `state` at its initial idle value,
/// same as never having started): mirrors `obs::run`.
fn run(state: Arc<ArcSwap<CloudPresetsSnapshot>>, control_rx: Receiver<CloudPresetsControl>) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("opendrop-io: cloud_presets failed to build tokio runtime: {e}");
            return;
        }
    };
    rt.block_on(async_run(state, control_rx));
}

async fn async_run(state: Arc<ArcSwap<CloudPresetsSnapshot>>, control_rx: Receiver<CloudPresetsControl>) {
    // Bridges the synchronous `control_rx` into an async-friendly channel,
    // once, for this thread's whole lifetime: see `obs::async_run`'s doc
    // comment (via `remote_ws::async_run`) for the full rationale.
    let (async_tx, mut async_rx) = tokio::sync::mpsc::unbounded_channel::<CloudPresetsControl>();
    tokio::task::spawn_blocking(move || {
        while let Ok(msg) = control_rx.recv() {
            if async_tx.send(msg).is_err() {
                break; // async side gone
            }
        }
    });

    while let Some(msg) = async_rx.recv().await {
        match msg {
            CloudPresetsControl::List { base_url } => {
                update(&state, |s| s.busy = true);
                let result = list_with_token(base_url).await;
                update(&state, |s| {
                    s.busy = false;
                    match result {
                        Ok(entries) => {
                            s.entries = entries;
                            s.last_error = None;
                        }
                        Err(e) => {
                            eprintln!("opendrop-io: cloud_presets list failed: {e}");
                            s.last_error = Some(e);
                        }
                    }
                });
            }
            CloudPresetsControl::Upload { base_url, name, data } => {
                update(&state, |s| s.busy = true);
                // Folds "upload" and "refresh the list" into one control
                // message, same as `ObsControl::Connect` folding connect +
                // GetSceneList: the panel always wants the freshly
                // uploaded entry to show up immediately.
                let result = upload_with_token(&base_url, &name, &data).await;
                let refreshed = if result.is_ok() { Some(list_with_token(base_url).await) } else { None };
                update(&state, |s| {
                    s.busy = false;
                    match result {
                        Ok(()) => {
                            s.last_error = None;
                            if let Some(Ok(entries)) = refreshed {
                                s.entries = entries;
                            }
                        }
                        Err(e) => {
                            eprintln!("opendrop-io: cloud_presets upload failed: {e}");
                            s.last_error = Some(e);
                        }
                    }
                });
            }
            CloudPresetsControl::Rename { base_url, id, name } => {
                update(&state, |s| s.busy = true);
                let result = rename_with_token(&base_url, &id, &name).await;
                let refreshed = if result.is_ok() { Some(list_with_token(base_url).await) } else { None };
                update(&state, |s| {
                    s.busy = false;
                    match result {
                        Ok(()) => {
                            s.last_error = None;
                            if let Some(Ok(entries)) = refreshed {
                                s.entries = entries;
                            }
                        }
                        Err(e) => {
                            eprintln!("opendrop-io: cloud_presets rename failed: {e}");
                            s.last_error = Some(e);
                        }
                    }
                });
            }
            CloudPresetsControl::Delete { base_url, id } => {
                update(&state, |s| s.busy = true);
                let result = delete_with_token(&base_url, &id).await;
                let refreshed = if result.is_ok() { Some(list_with_token(base_url).await) } else { None };
                update(&state, |s| {
                    s.busy = false;
                    match result {
                        Ok(()) => {
                            s.last_error = None;
                            if let Some(Ok(entries)) = refreshed {
                                s.entries = entries;
                            }
                        }
                        Err(e) => {
                            eprintln!("opendrop-io: cloud_presets delete failed: {e}");
                            s.last_error = Some(e);
                        }
                    }
                });
            }
            CloudPresetsControl::Download { base_url, id } => {
                update(&state, |s| s.busy = true);
                let result = download_with_token(base_url, id).await;
                update(&state, |s| {
                    s.busy = false;
                    match result {
                        Ok(path) => {
                            s.last_downloaded = Some(path);
                            s.last_error = None;
                        }
                        Err(e) => {
                            eprintln!("opendrop-io: cloud_presets download failed: {e}");
                            s.last_error = Some(e);
                        }
                    }
                });
            }
        }
    }
}

async fn list_with_token(base_url: String) -> Result<Vec<IndexEntry>, String> {
    let token = ensure_token()?;
    CloudPresetsClient::new(base_url, token).list().await
}

async fn upload_with_token(base_url: &str, name: &str, data: &str) -> Result<(), String> {
    let token = ensure_token()?;
    CloudPresetsClient::new(base_url.to_string(), token).upload(name, data).await.map(|_id| ())
}

async fn rename_with_token(base_url: &str, id: &str, name: &str) -> Result<(), String> {
    let token = ensure_token()?;
    CloudPresetsClient::new(base_url.to_string(), token).rename(id, name).await
}

async fn delete_with_token(base_url: &str, id: &str) -> Result<(), String> {
    let token = ensure_token()?;
    CloudPresetsClient::new(base_url.to_string(), token).delete(id).await
}

async fn download_with_token(base_url: String, id: String) -> Result<PathBuf, String> {
    let token = ensure_token()?;
    download_and_cache(base_url, token, id, cloud_presets_cache_dir()).await
}

/// Downloads preset `id`'s raw JSON and writes it to `<cache_dir>/<id>.json`,
/// split out from `download_with_token` so tests can exercise this
/// against a real mock server with a fixed token and a scratch directory,
/// without touching the real OS keyring (`ensure_token`) or the real cache
/// dir (`cloud_presets_cache_dir`).
async fn download_and_cache(base_url: String, token: String, id: String, cache_dir: PathBuf) -> Result<PathBuf, String> {
    let json_text = CloudPresetsClient::new(base_url, token).get(&id).await?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("failed to create cache dir: {e}"))?;
    let path = cache_dir.join(format!("{id}.json"));
    std::fs::write(&path, &json_text).map_err(|e| format!("failed to write cache file: {e}"))?;
    Ok(path)
}

/// Returns the device's cloud identity token, generating and persisting a
/// fresh one on first use: ports `getOrCreateCloudToken`, see the module
/// doc comment for the one deviation (hex bytes, not a real UUID). `pub`
/// so `ui::cloud_presets` can call it directly too (e.g. a "Copy token"
/// button needs a token to exist even before any network call has ever
/// run): a synchronous keyring lookup, same reasoning `secrets::
/// get_secret`/`set_secret` are called directly from the Streaming panel
/// for its own OBS/Twitch/Kick fields rather than routed through a thread.
pub fn ensure_token() -> Result<String, String> {
    match secrets::get_secret(secrets::CLOUD_PRESETS_TOKEN)? {
        Some(token) if !token.is_empty() => Ok(token),
        _ => {
            let token = generate_token();
            secrets::set_secret(secrets::CLOUD_PRESETS_TOKEN, &token)?;
            Ok(token)
        }
    }
}

/// 16 hex-encoded random bytes (128 bits, same entropy class as a v4
/// UUID): see the module doc comment for why this isn't an actual UUID.
/// Mirrors `remote_ws::generate_token`'s technique (byte count differs: 16
/// here vs 12 there, since this is a long-lived identity, not a
/// per-session token).
fn generate_token() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Disk-cache root for downloaded cloud presets: `$XDG_CACHE_HOME/opendrop/
/// cloud-presets`, falling back to `$HOME/.cache/opendrop/cloud-presets`.
/// Same convention as `app`'s own `thumbnail_cache_dir`: duplicated
/// rather than shared since this crate can't depend on `app` (wrong
/// direction) and `app`'s version isn't `pub`.
fn cloud_presets_cache_dir() -> PathBuf {
    cloud_presets_cache_dir_from(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))
}

/// The env-reading half of `cloud_presets_cache_dir`, split out so the
/// fallback order is testable without mutating process-global environment
/// state: same split as `app::thumbnail_cache_dir_from`.
fn cloud_presets_cache_dir_from(xdg_cache_home: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>) -> PathBuf {
    let xdg = xdg_cache_home.map(PathBuf::from);
    let home_cache = home.map(PathBuf::from).map(|h| h.join(".cache"));
    xdg.into_iter()
        .chain(home_cache)
        .find(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir)
        .join("opendrop")
        .join("cloud-presets")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::get;
    use axum::{Json, Router};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;

    #[test]
    fn cloud_preset_prefix_matches_the_web_reference() {
        assert_eq!(CLOUD_PRESET_PREFIX, "☁ ");
    }

    #[test]
    fn generate_token_is_32_hex_chars_and_varies_across_calls() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 32); // 16 bytes, hex-encoded
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "token must be freshly generated on each call");
    }

    #[test]
    fn index_entry_deserializes_the_documented_wire_shape() {
        let json = r#"{"id":"abc","name":"☁ Foo","sizeBytes":1234,"uploadedAt":1700000000000}"#;
        let entry: IndexEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry, IndexEntry { id: "abc".into(), name: "☁ Foo".into(), size_bytes: 1234, uploaded_at: 1_700_000_000_000 });
    }

    #[test]
    fn upload_rejects_invalid_json_without_any_network_call() {
        block_on(async {
            let client = CloudPresetsClient::new("http://127.0.0.1:1".to_string(), "t".to_string());
            let err = client.upload("x", "not json").await.unwrap_err();
            assert!(err.contains("invalid preset JSON"), "unexpected error: {err}");
        });
    }

    #[test]
    fn upload_rejects_non_object_json_without_any_network_call() {
        block_on(async {
            let client = CloudPresetsClient::new("http://127.0.0.1:1".to_string(), "t".to_string());
            let err = client.upload("x", "[1,2,3]").await.unwrap_err();
            assert!(err.contains("must be a JSON object"), "unexpected error: {err}");
        });
    }

    // --- Mock server: a minimal in-memory stand-in for `workers/
    // presets-cloud/src/handlers.ts`, exercising `CloudPresetsClient`
    // against a real bound socket rather than just asserting on request
    // construction. No new dependency: `axum` is already a direct `io`
    // dependency (used by `remote_ws`), just reused here for tests. This
    // is the "local mock" option this task's brief calls out: no real
    // CloudPresets backend URL is available to test against yet
    // (Override 4 in the plan).

    #[derive(Clone, Default)]
    struct MockBackend(Arc<Mutex<MockState>>);

    #[derive(Default)]
    struct MockState {
        entries: Vec<IndexEntry>,
        data: HashMap<String, serde_json::Value>,
        next_id: u32,
        last_token_seen: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct UploadBody {
        name: String,
        data: serde_json::Value,
    }

    #[derive(serde::Deserialize)]
    struct RenameBody {
        name: String,
    }

    fn mock_router(backend: MockBackend) -> Router {
        Router::new()
            .route("/presets", get(mock_list).post(mock_upload))
            .route("/presets/{id}", get(mock_get).patch(mock_rename).delete(mock_delete))
            .with_state(backend)
    }

    fn record_token(backend: &MockBackend, headers: &HeaderMap) {
        backend.0.lock().unwrap().last_token_seen = headers.get("X-Cloud-Token").and_then(|v| v.to_str().ok()).map(String::from);
    }

    async fn mock_list(State(backend): State<MockBackend>, headers: HeaderMap) -> Json<Vec<IndexEntry>> {
        record_token(&backend, &headers);
        Json(backend.0.lock().unwrap().entries.clone())
    }

    async fn mock_upload(State(backend): State<MockBackend>, headers: HeaderMap, Json(body): Json<UploadBody>) -> Json<serde_json::Value> {
        record_token(&backend, &headers);
        let mut s = backend.0.lock().unwrap();
        s.next_id += 1;
        let id = format!("id-{}", s.next_id);
        s.data.insert(id.clone(), body.data);
        s.entries.push(IndexEntry { id: id.clone(), name: body.name, size_bytes: 0, uploaded_at: 0 });
        Json(serde_json::json!({ "id": id }))
    }

    async fn mock_get(State(backend): State<MockBackend>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
        backend.0.lock().unwrap().data.get(&id).cloned().map(Json).ok_or(StatusCode::NOT_FOUND)
    }

    async fn mock_rename(State(backend): State<MockBackend>, Path(id): Path<String>, Json(body): Json<RenameBody>) -> Json<serde_json::Value> {
        let mut s = backend.0.lock().unwrap();
        if let Some(entry) = s.entries.iter_mut().find(|e| e.id == id) {
            entry.name = body.name;
        }
        Json(serde_json::json!({ "ok": true }))
    }

    async fn mock_delete(State(backend): State<MockBackend>, Path(id): Path<String>) -> Json<serde_json::Value> {
        let mut s = backend.0.lock().unwrap();
        s.entries.retain(|e| e.id != id);
        s.data.remove(&id);
        Json(serde_json::json!({ "ok": true }))
    }

    /// Builds a fresh single-threaded runtime and blocks on `fut`: same
    /// runtime shape as production `run()`'s own `rt.block_on(...)`, no
    /// `#[tokio::test]` needed (the `macros` tokio feature isn't enabled
    /// in this crate, see `io/Cargo.toml`).
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(fut)
    }

    /// Binds the mock router to an OS-assigned localhost port and spawns
    /// it on the calling runtime (`tokio::spawn`, not a new thread, a
    /// `new_current_thread` runtime still runs spawned tasks
    /// cooperatively alongside whatever awaits the caller does next).
    /// Returns the `http://127.0.0.1:{port}` base URL.
    async fn start_mock_server(backend: MockBackend) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = mock_router(backend);
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}")
    }

    #[test]
    fn list_upload_get_rename_delete_round_trip_against_a_real_mock_server() {
        block_on(async {
            let backend = MockBackend::default();
            let base_url = start_mock_server(backend.clone()).await;
            let client = CloudPresetsClient::new(base_url, "test-token-123".to_string());

            assert_eq!(client.list().await.unwrap(), Vec::new());

            let id = client.upload("MyPreset", r#"{"shapes":[]}"#).await.unwrap();
            let entries = client.list().await.unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, format!("{CLOUD_PRESET_PREFIX}MyPreset"));
            assert_eq!(entries[0].id, id);

            // The header actually carried the token, on every route hit above.
            assert_eq!(backend.0.lock().unwrap().last_token_seen.as_deref(), Some("test-token-123"));

            let downloaded = client.get(&id).await.unwrap();
            let value: serde_json::Value = serde_json::from_str(&downloaded).unwrap();
            assert_eq!(value, serde_json::json!({"shapes": []}));

            client.rename(&id, "Renamed").await.unwrap();
            let entries = client.list().await.unwrap();
            assert_eq!(entries[0].name, format!("{CLOUD_PRESET_PREFIX}Renamed"));

            client.delete(&id).await.unwrap();
            assert_eq!(client.list().await.unwrap(), Vec::new());
        });
    }

    #[test]
    fn get_missing_id_returns_a_not_found_error() {
        block_on(async {
            let base_url = start_mock_server(MockBackend::default()).await;
            let client = CloudPresetsClient::new(base_url, "t".to_string());
            let err = client.get("does-not-exist").await.unwrap_err();
            assert_eq!(err, "preset not found");
        });
    }

    /// Scratch directory for the `download_and_cache` tests below: same
    /// pid-suffixed-under-`temp_dir` convention as `app/src/config.rs`'s
    /// `save_then_load_round_trips_through_the_real_filesystem`, kept
    /// distinct per test (suffix) so the two tests can't collide.
    fn scratch_cache_dir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opendrop-io-test-cloud-presets-cache-{}-{suffix}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn download_and_cache_writes_the_raw_json_to_the_given_cache_dir() {
        block_on(async {
            let backend = MockBackend::default();
            backend.0.lock().unwrap().data.insert("id-1".to_string(), serde_json::json!({"shapes": []}));
            let base_url = start_mock_server(backend).await;
            let cache_dir = scratch_cache_dir("hit");

            let path = download_and_cache(base_url, "t".to_string(), "id-1".to_string(), cache_dir.clone()).await.unwrap();

            assert_eq!(path, cache_dir.join("id-1.json"));
            let written = std::fs::read_to_string(&path).unwrap();
            let value: serde_json::Value = serde_json::from_str(&written).unwrap();
            assert_eq!(value, serde_json::json!({"shapes": []}));

            let _ = std::fs::remove_dir_all(&cache_dir);
        });
    }

    #[test]
    fn download_and_cache_surfaces_a_missing_id_as_an_error_without_writing_anything() {
        block_on(async {
            let base_url = start_mock_server(MockBackend::default()).await;
            let cache_dir = scratch_cache_dir("miss");

            let err = download_and_cache(base_url, "t".to_string(), "does-not-exist".to_string(), cache_dir.clone()).await.unwrap_err();

            assert_eq!(err, "preset not found");
            assert!(!cache_dir.exists(), "must not create the cache dir on a failed download");
        });
    }

    mod cloud_presets_cache_dir_tests {
        use super::*;

        // Only used by the POSIX-only fixtures below; gated the same way
        // `app::thumbnail_cache_dir_tests` gates its own `os` helper, so
        // it doesn't trip an unused-function warning on Windows.
        #[cfg(not(target_os = "windows"))]
        fn os(s: &str) -> Option<std::ffi::OsString> {
            Some(std::ffi::OsString::from(s))
        }

        // Mirrors `app::thumbnail_cache_dir_tests` exactly (same fallback
        // logic, deliberately duplicated: see `cloud_presets_cache_dir`'s
        // doc comment): POSIX-absolute literals exercise the
        // `.is_absolute()` branch, which requires a drive/UNC prefix on
        // Windows, so these 3 are POSIX-only.
        #[test]
        #[cfg(not(target_os = "windows"))]
        fn prefers_xdg_cache_home() {
            let dir = cloud_presets_cache_dir_from(os("/xdg"), os("/home/u"));
            assert_eq!(dir, PathBuf::from("/xdg/opendrop/cloud-presets"));
        }

        #[test]
        #[cfg(not(target_os = "windows"))]
        fn falls_back_to_home_dot_cache() {
            let dir = cloud_presets_cache_dir_from(None, os("/home/u"));
            assert_eq!(dir, PathBuf::from("/home/u/.cache/opendrop/cloud-presets"));
        }

        #[test]
        #[cfg(not(target_os = "windows"))]
        fn ignores_a_relative_xdg_cache_home() {
            let dir = cloud_presets_cache_dir_from(os("relative/cache"), os("/home/u"));
            assert_eq!(dir, PathBuf::from("/home/u/.cache/opendrop/cloud-presets"));
        }

        #[test]
        fn never_lands_directly_in_a_shared_tmp_root() {
            // Even the last-resort branch nests under its own subdirectory
            // rather than a bare, world-predictable path.
            let dir = cloud_presets_cache_dir_from(None, None);
            assert!(dir.ends_with("opendrop/cloud-presets"));
        }
    }

    /// Full thread-wiring smoke test: `spawn()`, send `List`, poll
    /// `latest()` against a real mock server: exercises the
    /// tokio-runtime-in-a-thread wiring itself (`run`/`async_run`), not
    /// just `CloudPresetsClient`'s pure HTTP logic exercised above.
    /// Ignored by default: `ensure_token()` touches the real OS keyring,
    /// which may be unavailable in this repo's minimal Hyprland dev
    /// session (see `secrets.rs`'s own
    /// `round_trip_secret_when_keyring_available` for the same caveat).
    /// Run explicitly with `cargo test -- --ignored` on a machine with a
    /// working keyring backend.
    #[test]
    #[ignore = "requires a real OS keyring/Secret Service backend"]
    fn spawned_thread_lists_against_a_real_mock_server() {
        block_on(async {
            let backend = MockBackend::default();
            backend.0.lock().unwrap().entries.push(IndexEntry {
                id: "id-1".into(),
                name: format!("{CLOUD_PRESET_PREFIX}Existing"),
                size_bytes: 10,
                uploaded_at: 0,
            });
            let base_url = start_mock_server(backend).await;

            let handle = spawn();
            handle.control_tx.send(CloudPresetsControl::List { base_url }).unwrap();

            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                let snap = handle.latest();
                if !snap.entries.is_empty() {
                    assert_eq!(snap.entries[0].name, format!("{CLOUD_PRESET_PREFIX}Existing"));
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "list never completed");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
    }
}
