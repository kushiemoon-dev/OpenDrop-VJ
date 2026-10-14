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
// is a local mock: no real CloudPresets backend URL is available to
// test against yet.

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
