//! CloudVideo: a fetch-only REST client for a self-hosted static CDN
//! serving pre-rendered video loops, plus the dedicated background thread
//! that drives it from the UI panel (`app::ui::cloud_video`) without
//! blocking egui's render loop.
//!
//! Same async-thread-per-integration pattern as `cloud_presets`/`obs` (see
//! `io::obs`'s module doc comment for the full architecture writeup): a
//! dedicated `std::thread` builds its own single-threaded tokio runtime and
//! never leaves it, bridging the synchronous `control_tx`/`control_rx` into
//! the async world once via `tokio::task::spawn_blocking`.
//!
//! Unlike `cloud_presets`, there is no identity token and no Upload/Rename/
//! Delete: the CDN is a plain static file server (`GET /manifest.json` +
//! `GET /<slug>`), and this client only ever reads from it. A downloaded
//! clip is written straight into the same directory the native Video
//! panel's "+ Video" import button already uses
//! (`app::video_clips::user_clips_dir`); [`video_clips_dir`] below
//! duplicates that path computation rather than depending on the `app`
//! crate (wrong direction), same reasoning as `cloud_presets`'s
//! `cloud_presets_cache_dir` doc comment.
//!
//! Manifest shape (from the CDN's `manifest.json`):
//! ```json
//! { "version": 1, "count": 53, "entries": [{ "slug": "a.webm", "name": "A" }] }
//! ```
//! Only `entries` is read; `version`/`count` and any other top-level or
//! per-entry field are ignored (serde's default "ignore unknown fields"
//! behavior, no `deny_unknown_fields` anywhere here).

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use arc_swap::ArcSwap;
use futures_util::StreamExt;

/// One clip listed in the CDN's manifest.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ManifestEntry {
    pub slug: String,
    pub name: String,
}

#[derive(serde::Deserialize)]
struct Manifest {
    #[serde(default)]
    entries: Vec<ManifestEntry>,
}

/// Parses a `manifest.json` body into its entries. Malformed JSON is
/// reported as an error rather than panicking.
fn parse_manifest(body: &str) -> Result<Vec<ManifestEntry>, String> {
    let manifest: Manifest = serde_json::from_str(body).map_err(|e| format!("invalid manifest: {e}"))?;
    Ok(manifest.entries)
}

/// Outcome of a completed download attempt, distinct from an error: finding
/// the file already present locally is success, not a failure to report as
/// `SlugDownloadState::Failed`.
#[derive(Debug, Clone, PartialEq)]
enum DownloadOutcome {
    Downloaded(PathBuf),
    AlreadyDownloaded,
}

/// Downloads `{base_url}/{slug}` into `dest_dir/{slug}`, aborting and
/// deleting the partial file if the body exceeds `cap` bytes. Split from
/// [`download_to_library`] so tests can exercise the streaming/cap logic
/// against a real mock server with a small `cap` and a scratch directory,
/// without transferring anywhere near the real 50MB limit.
async fn download_capped(base_url: &str, slug: &str, dest_dir: &Path, cap: u64) -> Result<DownloadOutcome, String> {
    let target = dest_dir.join(slug);
    if target.exists() {
        return Ok(DownloadOutcome::AlreadyDownloaded);
    }

    let url = format!("{}/{slug}", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    std::fs::create_dir_all(dest_dir).map_err(|e| format!("creating {}: {e}", dest_dir.display()))?;
    let mut file = std::fs::File::create(&target).map_err(|e| format!("creating {}: {e}", target.display()))?;

    let mut total: u64 = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        total += chunk.len() as u64;
        if total > cap {
            drop(file);
            let _ = std::fs::remove_file(&target);
            return Err(format!("{slug} exceeds the {} MB limit", cap / (1024 * 1024)));
        }
        file.write_all(&chunk).map_err(|e| format!("writing {}: {e}", target.display()))?;
    }
    Ok(DownloadOutcome::Downloaded(target))
}

/// Fetches `{base_url}/manifest.json` and parses it.
async fn list_entries(base_url: &str) -> Result<Vec<ManifestEntry>, String> {
    let url = format!("{}/manifest.json", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let body = response.text().await.map_err(|e| e.to_string())?;
    parse_manifest(&body)
}

/// Per-slug download progress, for the panel's list rows.
#[derive(Debug, Clone, PartialEq)]
pub enum SlugDownloadState {
    InProgress,
    Done,
    AlreadyDownloaded,
    Failed(String),
}

/// Continuous state published via `CloudVideoHandle::latest()`.
#[derive(Clone, Default)]
pub struct CloudVideoSnapshot {
    pub entries: Vec<ManifestEntry>,
    pub busy: bool,
    pub listing_error: Option<String>,
    pub downloads: HashMap<String, SlugDownloadState>,
}

/// Outward control messages sent to the CloudVideo thread. Fetch-only: no
/// Upload/Rename/Delete variant exists, by design.
pub enum CloudVideoControl {
    List { base_url: String },
    Download { base_url: String, slug: String },
}

/// Handle to the running CloudVideo thread. Mirrors `CloudPresetsHandle`'s
/// shape: `latest()` never blocks, `control_tx` sends never block.
pub struct CloudVideoHandle {
    state: Arc<ArcSwap<CloudVideoSnapshot>>,
    pub control_tx: Sender<CloudVideoControl>,
}

impl CloudVideoHandle {
    /// Never blocks: an atomic load of the current Arc.
    pub fn latest(&self) -> Arc<CloudVideoSnapshot> {
        self.state.load_full()
    }
}

/// Spawns the dedicated CloudVideo thread and returns immediately, starting
/// idle until the first control message arrives.
pub fn spawn() -> CloudVideoHandle {
    let state = Arc::new(ArcSwap::from_pointee(CloudVideoSnapshot::default()));
    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(state, control_rx)
    });
    CloudVideoHandle { state, control_tx }
}

fn update(state: &Arc<ArcSwap<CloudVideoSnapshot>>, f: impl FnOnce(&mut CloudVideoSnapshot)) {
    let mut next = (*state.load_full()).clone();
    f(&mut next);
    state.store(Arc::new(next));
}

fn run(state: Arc<ArcSwap<CloudVideoSnapshot>>, control_rx: Receiver<CloudVideoControl>) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("opendrop-io: cloud_video failed to build tokio runtime: {e}");
            return;
        }
    };
    rt.block_on(async_run(state, control_rx));
}

async fn async_run(state: Arc<ArcSwap<CloudVideoSnapshot>>, control_rx: Receiver<CloudVideoControl>) {
    let (async_tx, mut async_rx) = tokio::sync::mpsc::unbounded_channel::<CloudVideoControl>();
    tokio::task::spawn_blocking(move || {
        while let Ok(msg) = control_rx.recv() {
            if async_tx.send(msg).is_err() {
                break; // async side gone
            }
        }
    });

    while let Some(msg) = async_rx.recv().await {
        match msg {
            CloudVideoControl::List { base_url } => {
                update(&state, |s| s.busy = true);
                let result = list_entries(&base_url).await;
                update(&state, |s| {
                    s.busy = false;
                    match result {
                        Ok(entries) => {
                            s.entries = entries;
                            s.listing_error = None;
                        }
                        Err(e) => {
                            eprintln!("opendrop-io: cloud_video list failed: {e}");
                            s.listing_error = Some(e);
                        }
                    }
                });
            }
            CloudVideoControl::Download { base_url, slug } => {
                update(&state, |s| {
                    s.downloads.insert(slug.clone(), SlugDownloadState::InProgress);
                });
                let result = download_to_library(&base_url, &slug).await;
                update(&state, |s| {
                    let next_state = match result {
                        Ok(DownloadOutcome::Downloaded(_)) => SlugDownloadState::Done,
                        Ok(DownloadOutcome::AlreadyDownloaded) => SlugDownloadState::AlreadyDownloaded,
                        Err(e) => {
                            eprintln!("opendrop-io: cloud_video download failed: {e}");
                            SlugDownloadState::Failed(e)
                        }
                    };
                    s.downloads.insert(slug.clone(), next_state);
                });
            }
        }
    }
}

/// `ProjectDirs::data_dir()/video-clips`: duplicates `app::video_clips::
/// user_clips_dir` rather than depending on it (wrong direction, `io` can't
/// depend on `app`). `None` in an environment with no home directory at
/// all.
fn video_clips_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "opendrop-native").map(|dirs| dirs.data_dir().join("video-clips"))
}

async fn download_to_library(base_url: &str, slug: &str) -> Result<DownloadOutcome, String> {
    let dir = video_clips_dir().ok_or("no data directory available on this system")?;
    download_capped(base_url, slug, &dir, opendrop_core::video::MAX_CLIP_BYTES).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::{Path as AxumPath, State};
    use axum::http::StatusCode;
    use axum::response::Response;
    use axum::routing::get;
    use axum::Router;
    use std::sync::Mutex;
    use std::time::Duration;

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(fut)
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opendrop-io-test-cloud-video-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // --- manifest parsing (pure, no network) ---

    #[test]
    fn parses_the_documented_manifest_shape() {
        let body = r#"{"version":1,"count":1,"entries":[{"slug":"neon-city-night-01.webm","name":"Neon City Night 01"}]}"#;
        let entries = parse_manifest(body).unwrap();
        assert_eq!(entries, vec![ManifestEntry { slug: "neon-city-night-01.webm".into(), name: "Neon City Night 01".into() }]);
    }

    #[test]
    fn malformed_json_is_reported_as_an_error() {
        let err = parse_manifest("not json").unwrap_err();
        assert!(err.contains("invalid manifest"), "unexpected error: {err}");
    }

    #[test]
    fn an_empty_entries_array_parses_to_an_empty_list() {
        let entries = parse_manifest(r#"{"version":1,"count":0,"entries":[]}"#).unwrap();
        assert_eq!(entries, Vec::new());
    }

    #[test]
    fn unknown_extra_fields_are_ignored() {
        let body = r#"{"version":1,"count":1,"generatedAt":"2026-01-01","entries":[{"slug":"a.webm","name":"A","thumbnail":"a.png"}]}"#;
        let entries = parse_manifest(body).unwrap();
        assert_eq!(entries, vec![ManifestEntry { slug: "a.webm".into(), name: "A".into() }]);
    }

    // --- already-downloaded skip (no network call made) ---

    #[test]
    fn a_file_already_present_is_skipped_without_any_network_call() {
        block_on(async {
            let dir = scratch_dir("already-present");
            std::fs::write(dir.join("clip.webm"), b"existing").unwrap();

            // Deliberately unreachable: proves no request is attempted.
            let outcome = download_capped("http://127.0.0.1:1", "clip.webm", &dir, 1024).await.unwrap();

            assert_eq!(outcome, DownloadOutcome::AlreadyDownloaded);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    // --- size-cap abort and happy path, against a real mock server ---

    #[derive(Clone, Default)]
    struct MockBackend(Arc<Mutex<HashMap<String, Vec<u8>>>>);

    async fn mock_clip(State(backend): State<MockBackend>, AxumPath(slug): AxumPath<String>) -> Response {
        match backend.0.lock().unwrap().get(&slug).cloned() {
            Some(bytes) => Response::builder().status(StatusCode::OK).body(Body::from(bytes)).unwrap(),
            None => Response::builder().status(StatusCode::NOT_FOUND).body(Body::empty()).unwrap(),
        }
    }

    async fn mock_manifest(State(backend): State<MockBackend>) -> Response {
        let slugs = backend.0.lock().unwrap();
        let entries: Vec<String> = slugs.keys().map(|slug| format!(r#"{{"slug":"{slug}","name":"{slug}"}}"#)).collect();
        let body = format!(r#"{{"version":1,"count":{},"entries":[{}]}}"#, entries.len(), entries.join(","));
        Response::builder().status(StatusCode::OK).header("content-type", "application/json").body(Body::from(body)).unwrap()
    }

    fn mock_router(backend: MockBackend) -> Router {
        Router::new().route("/manifest.json", get(mock_manifest)).route("/{slug}", get(mock_clip)).with_state(backend)
    }

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
    fn a_body_over_the_cap_is_aborted_and_the_partial_file_is_removed() {
        block_on(async {
            let backend = MockBackend::default();
            backend.0.lock().unwrap().insert("big.webm".to_string(), vec![0u8; 20]);
            let base_url = start_mock_server(backend).await;
            let dir = scratch_dir("over-cap");

            let err = download_capped(&base_url, "big.webm", &dir, 10).await.unwrap_err();

            assert!(err.contains("exceeds"), "unexpected message: {err}");
            assert!(!dir.join("big.webm").exists(), "the partial file must be removed");
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    #[test]
    fn a_body_under_the_cap_is_written_in_full() {
        block_on(async {
            let backend = MockBackend::default();
            backend.0.lock().unwrap().insert("small.webm".to_string(), vec![7u8; 5]);
            let base_url = start_mock_server(backend).await;
            let dir = scratch_dir("under-cap");

            let outcome = download_capped(&base_url, "small.webm", &dir, 10).await.unwrap();

            assert_eq!(outcome, DownloadOutcome::Downloaded(dir.join("small.webm")));
            assert_eq!(std::fs::read(dir.join("small.webm")).unwrap(), vec![7u8; 5]);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    #[test]
    fn a_missing_clip_on_the_server_is_a_readable_error() {
        block_on(async {
            let base_url = start_mock_server(MockBackend::default()).await;
            let dir = scratch_dir("missing-clip");

            let err = download_capped(&base_url, "nope.webm", &dir, 1024).await.unwrap_err();

            assert!(err.contains("404"), "unexpected message: {err}");
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    #[test]
    fn list_entries_round_trips_against_a_real_mock_server() {
        block_on(async {
            let backend = MockBackend::default();
            backend.0.lock().unwrap().insert("a.webm".to_string(), vec![1, 2, 3]);
            let base_url = start_mock_server(backend).await;

            let entries = list_entries(&base_url).await.unwrap();

            assert_eq!(entries, vec![ManifestEntry { slug: "a.webm".into(), name: "a.webm".into() }]);
        });
    }

    #[test]
    fn listing_an_unreachable_server_is_a_readable_error() {
        block_on(async {
            let err = list_entries("http://127.0.0.1:1").await.unwrap_err();
            assert!(!err.is_empty());
        });
    }

    #[test]
    fn spawned_thread_lists_against_a_real_mock_server() {
        block_on(async {
            let backend = MockBackend::default();
            backend.0.lock().unwrap().insert("a.webm".to_string(), vec![1]);
            let base_url = start_mock_server(backend).await;

            let handle = spawn();
            handle.control_tx.send(CloudVideoControl::List { base_url }).unwrap();

            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                let snap = handle.latest();
                if !snap.entries.is_empty() {
                    assert_eq!(snap.entries[0].slug, "a.webm");
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "list never completed");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
    }
}
