//! Asks GitHub for the latest release, once, on a background thread. The
//! answer is only ever shown to the user; nothing is downloaded or installed.
//!
//! It uses the "latest release" endpoint rather than the release list: the
//! repository also holds the releases of the earlier web app, whose version
//! numbers are higher, and GitHub's own "Latest" mark is what separates them.
//! Any failure (offline, rate limited, unexpected reply) is returned as an
//! error and the app carries on without a notice.

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use serde::Deserialize;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/kushiemoon-dev/OpenDrop-VJ/releases/latest";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// A real reply is a few kilobytes; anything beyond this is not one.
const MAX_BODY_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestRelease {
    pub tag: String,
    pub url: String,
}

#[derive(Deserialize)]
struct ReleaseJson {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Reads the fields we use from the API reply. A draft or pre-release is not
/// something to announce.
pub fn parse_latest_release(json: &str) -> Result<LatestRelease, String> {
    let release: ReleaseJson = serde_json::from_str(json).map_err(|e| format!("unexpected release reply: {e}"))?;
    if release.draft || release.prerelease {
        return Err("the latest release is a draft or a pre-release".to_string());
    }
    Ok(LatestRelease { tag: release.tag_name, url: release.html_url })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckState {
    Pending,
    Done(Result<LatestRelease, String>),
}

pub struct UpdateCheckHandle {
    state: Arc<ArcSwap<UpdateCheckState>>,
}

impl UpdateCheckHandle {
    pub fn latest(&self) -> Arc<UpdateCheckState> {
        self.state.load_full()
    }
}

/// Starts one check against GitHub. `app_version` goes into the User-Agent
/// header, which the GitHub API requires.
pub fn spawn(app_version: &str) -> UpdateCheckHandle {
    spawn_for(LATEST_RELEASE_URL.to_string(), app_version.to_string())
}

fn spawn_for(url: String, app_version: String) -> UpdateCheckHandle {
    let state = Arc::new(ArcSwap::from_pointee(UpdateCheckState::Pending));
    std::thread::spawn({
        let state = state.clone();
        move || {
            let outcome = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime.block_on(fetch_latest(&url, &app_version)),
                Err(e) => Err(format!("could not start the update check: {e}")),
            };
            state.store(Arc::new(UpdateCheckState::Done(outcome)));
        }
    });
    UpdateCheckHandle { state }
}

async fn fetch_latest(url: &str, app_version: &str) -> Result<LatestRelease, String> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(format!("OpenDrop-VJ/{app_version}"))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("update check failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("update check returned status {}", response.status()));
    }
    let body = response.bytes().await.map_err(|e| format!("update check failed: {e}"))?;
    if body.len() > MAX_BODY_BYTES {
        return Err("update check reply is too large".to_string());
    }
    parse_latest_release(&String::from_utf8_lossy(&body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn a_stable_release_is_read_and_drafts_and_prereleases_are_not() {
        let ok = r#"{"tag_name":"v0.3.0","html_url":"https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/tag/v0.3.0","draft":false,"prerelease":false,"body":"ignored"}"#;
        assert_eq!(
            parse_latest_release(ok),
            Ok(LatestRelease {
                tag: "v0.3.0".to_string(),
                url: "https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/tag/v0.3.0".to_string()
            })
        );
        assert!(parse_latest_release(r#"{"tag_name":"v0.3.0","prerelease":true}"#).is_err());
        assert!(parse_latest_release(r#"{"tag_name":"v0.3.0","draft":true}"#).is_err());
        assert!(parse_latest_release(r#"{"message":"Not Found"}"#).is_err());
        assert!(parse_latest_release("<html>").is_err());
    }

    /// Serves one canned HTTP response on a local port and returns its URL.
    fn serve_once(status: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        url
    }

    fn wait_done(handle: &UpdateCheckHandle) -> Result<LatestRelease, String> {
        for _ in 0..300 {
            if let UpdateCheckState::Done(result) = &*handle.latest() {
                return result.clone();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the check never finished");
    }

    #[test]
    fn a_successful_reply_ends_in_the_release() {
        let url = serve_once("200 OK", r#"{"tag_name":"v0.9.0","html_url":"https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/tag/v0.9.0"}"#);
        let handle = spawn_for(url, "0.2.0".to_string());
        assert_eq!(wait_done(&handle).unwrap().tag, "v0.9.0");
    }

    #[test]
    fn an_error_status_or_a_dead_server_ends_in_an_error_not_a_panic() {
        let handle = spawn_for(serve_once("403 Forbidden", r#"{"message":"rate limited"}"#), "0.2.0".to_string());
        assert!(wait_done(&handle).unwrap_err().contains("403"));

        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let dead_url = format!("http://{}/", closed.local_addr().unwrap());
        drop(closed);
        let handle = spawn_for(dead_url, "0.2.0".to_string());
        assert!(wait_done(&handle).is_err());
    }
}
