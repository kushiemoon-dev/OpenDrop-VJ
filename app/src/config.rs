//! UI-state persistence: `UiConfig` <-> JSON on disk at
//! `ProjectDirs::from("", "", "opendrop-native").config_dir()/ui.json`.
//! Structural copy of `io/src/midi/mapping.rs`'s mapping-persistence
//! pattern (Step 7 of the Phase 7 UI redesign plan), same best-effort,
//! never-panic philosophy: a missing/malformed/stale file degrades to
//! `UiConfig::default()` rather than failing bootstrap.
//!
//! Most of `UiConfig` is wired into `main.rs`'s bootstrap and exit paths:
//! `theme`, `active_panel`, `stage_mode`, `output_monitor`,
//! `audio_input_device`, `osc_port`, `obs_host`, `obs_port`,
//! `twitch_channel`, `kick_channel`, and `invisible_mode` are all restored
//! at bootstrap and saved in `App::exiting`. `ui_scale`/`target_fps` are
//! the two fields still unwired: no `AppState` counterpart exists yet to
//! receive them.
//!
//! Secrets (OBS/Twitch/Kick tokens) are never part of `UiConfig`, they
//! stay in the OS keyring via `opendrop_io::secrets`
//! (see `app/src/ui/streaming.rs`'s `save_secret_field`/
//! `clear_secret_button`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::theme::registry::ThemeId;
use crate::InvisibleMode;

/// Serde "remote type" shadow for `theme::registry::ThemeId`
/// (https://serde.rs/remote-derive.html): `ThemeId` is a Step 3 type we
/// import rather than redefine, and it doesn't derive `Serialize`/
/// `Deserialize` itself (Step 3 predates this module and has no reason to
/// carry a serde dependency for a persistence need that's local to this
/// step). This shadow's derived impl operates on the real `ThemeId`
/// directly via `#[serde(remote = "ThemeId")]`; `UiConfig::theme` opts in
/// via `#[serde(with = "ThemeIdWire")]` below. Variant names/count must
/// stay in sync with `ThemeId`'s real definition.
#[derive(Serialize, Deserialize)]
#[serde(remote = "ThemeId")]
enum ThemeIdWire {
    Kushie,
    OpenDropClassic,
    Cyan,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "InvisibleMode")]
enum InvisibleModeWire {
    Eco,
    Pause,
    Off,
}

/// Mirror of `main.rs`'s `Panel` enum (`main.rs:136-153`), kept as its own
/// serializable type rather than adding `Serialize`/`Deserialize` to
/// `Panel` itself, same reasoning `mapping.rs` gives for not deriving
/// those on `CommandId`: avoid a persistence-only impl on a type whose
/// primary job is UI navigation. Already reflects the post-Step-9 split of
/// `Panel::Ndi` into `NdiIn`/`NdiOut` (per this step's brief), even though
/// `Panel` itself hasn't split yet.
///
/// `#[serde(other)]` on `Decks` (mirroring `Panel`'s own `#[default]`
/// variant) makes any unrecognized wire name degrade to `Decks` instead of
/// failing deserialization: in particular, a `ui.json` written by a
/// `--features link` build (`active_panel: "Link"`) loads safely in a
/// default build, where the `Link` variant doesn't even exist. `Decks` is
/// declared last because serde requires the `#[serde(other)]` variant to
/// be the last one in the enum; `#[cfg]` stripping happens before serde's
/// derive macro sees the token stream, so `Decks` is still the trailing
/// variant in both a `--features link` build and a default build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PanelId {
    PresetBrowser,
    Playlists,
    Audio,
    Quality,
    Color,
    Composite,
    Keymap,
    Snapshot,
    Timeline,
    Time,
    Qvar,
    Strobe,
    Lfo,
    Output,
    Midi,
    NdiIn,
    NdiOut,
    Osc,
    Overlays,
    RemoteWs,
    Streaming,
    Share,
    #[cfg(feature = "link")]
    Link,
    V4l2,
    Video,
    CloudPresets,
    About,
    #[serde(other)]
    Decks,
}

/// Volatile UI state persisted best-effort to `ui.json`. Secrets excluded
/// on purpose (see module doc comment): everything here is safe to write
/// to disk in plain text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct UiConfig {
    #[serde(with = "ThemeIdWire")]
    pub(crate) theme: ThemeId,
    pub(crate) active_panel: PanelId,
    pub(crate) stage_mode: bool,
    pub(crate) ui_scale: f32,
    pub(crate) output_monitor: Option<String>,
    pub(crate) audio_input_device: Option<String>,
    pub(crate) osc_port: u16,
    pub(crate) obs_host: String,
    pub(crate) obs_port: u16,
    pub(crate) twitch_channel: String,
    pub(crate) kick_channel: String,
    #[serde(with = "InvisibleModeWire")]
    pub(crate) invisible_mode: InvisibleMode,
    pub(crate) target_fps: u32,
    pub(crate) favorite_presets: HashSet<String>,
    /// Base URL of the CloudPresets backend Worker (`workers/presets-
    /// cloud/` in the sibling `OpenDrop-VJ` repo). `None`/empty means the
    /// feature is disabled: same convention as the web app's
    /// `PUBLIC_CLOUD_PRESETS_API=` (empty in `.env.example`), see Step 6's
    /// Override 4: no production URL is committed anywhere yet, the user
    /// must supply one before this panel does anything.
    pub(crate) cloud_presets_api_url: Option<String>,
    /// On-disk shape of `AppState::keymap` (`HashMap<winit::keyboard::Key,
    /// CommandId>`): see `keymap.rs`'s module doc comment for why this is
    /// `HashMap<String, String>` (key-wire -> command-wire) rather than
    /// `HashMap<Key, CommandId>` or `HashMap<String, CommandId>` directly.
    /// Empty means "no persisted remapping yet": bootstrap falls back to
    /// `keymap::default_keymap()` in that case, same as a missing file
    /// falling back to the rest of `UiConfig::default()`.
    pub(crate) keymap: HashMap<String, String>,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: ThemeId::default(),
            active_panel: PanelId::Decks,
            stage_mode: false,
            ui_scale: 1.0,
            output_monitor: None,
            audio_input_device: None,
            osc_port: 7000,
            obs_host: "localhost".to_string(),
            obs_port: 4455,
            twitch_channel: String::new(),
            kick_channel: String::new(),
            invisible_mode: InvisibleMode::Eco,
            target_fps: 60,
            favorite_presets: HashSet::new(),
            cloud_presets_api_url: None,
            keymap: HashMap::new(),
        }
    }
}

/// `directories::ProjectDirs::from("", "", "opendrop-native").config_dir()
/// /ui.json`, or `None` if the OS gives us no home/config directory at all
/// (headless/CI environment), the caller treats that the same as
/// "nothing to load, nothing to save" (`mapping_file_path`'s own
/// contract).
pub(crate) fn config_file_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "opendrop-native").map(|dirs| dirs.config_dir().join("ui.json"))
}

/// `UiConfig` -> pretty-printed JSON string (human-editable on disk).
pub(crate) fn config_to_json(config: &UiConfig) -> String {
    serde_json::to_string_pretty(config).unwrap_or_else(|_| "{}".to_string())
}

/// JSON string -> `UiConfig`. Malformed JSON or a missing/unrecognized
/// field degrades to `UiConfig::default()` rather than erroring: a stale
/// or corrupt config file must never stop the app from starting.
pub(crate) fn config_from_json(json: &str) -> UiConfig {
    serde_json::from_str(json).unwrap_or_default()
}

/// Loads the config from `path`, or the default config if the file
/// doesn't exist yet or can't be read/parsed (logged once, never a
/// panic).
pub(crate) fn load_config(path: &Path) -> UiConfig {
    match std::fs::read_to_string(path) {
        Ok(json) => config_from_json(&json),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => UiConfig::default(),
        Err(e) => {
            eprintln!("[config] failed to read config file {}: {e}, using defaults", path.display());
            UiConfig::default()
        }
    }
}

/// Writes `config` to `path` as JSON, creating the parent directory if
/// needed. Best-effort: a write failure is logged, never a panic. Losing
/// the ability to persist UI state shouldn't take down the app.
pub(crate) fn save_config(path: Option<&Path>, config: &UiConfig) {
    let Some(path) = path else { return };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("[config] failed to create config dir {}: {e}, config not saved", parent.display());
            return;
        }
    }
    if let Err(e) = std::fs::write(path, config_to_json(config)) {
        eprintln!("[config] failed to write config file {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip_preserves_every_field() {
        let config = UiConfig {
            theme: ThemeId::Cyan,
            active_panel: PanelId::Streaming,
            stage_mode: true,
            ui_scale: 1.25,
            output_monitor: Some("DP-2".to_string()),
            audio_input_device: Some("Focusrite".to_string()),
            osc_port: 9000,
            obs_host: "192.168.1.50".to_string(),
            obs_port: 4444,
            twitch_channel: "kushiemoon".to_string(),
            kick_channel: "kushiemoon".to_string(),
            invisible_mode: InvisibleMode::Pause,
            target_fps: 144,
            favorite_presets: HashSet::from(["Alpha Swirl Refract".to_string(), "Beta Pulse Drift".to_string()]),
            cloud_presets_api_url: Some("https://presets-cloud.example.workers.dev".to_string()),
            keymap: HashMap::from([(r#"{"Named":"Tab"}"#.to_string(), "deck-switch".to_string())]),
        };

        let json = config_to_json(&config);
        let restored = config_from_json(&json);

        assert_eq!(restored, config);
    }

    #[test]
    fn default_config_round_trips_to_itself() {
        let config = UiConfig::default();
        assert_eq!(config_from_json(&config_to_json(&config)), config);
    }

    #[test]
    fn empty_json_degrades_to_default() {
        assert_eq!(config_from_json("{}"), UiConfig::default());
    }

    #[test]
    fn malformed_json_degrades_to_default() {
        assert_eq!(config_from_json("not json at all"), UiConfig::default());
    }

    #[test]
    fn load_config_missing_file_is_default_not_an_error() {
        let path = std::env::temp_dir().join("opendrop-native-test-config-does-not-exist.json");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load_config(&path), UiConfig::default());
    }

    #[test]
    fn save_then_load_round_trips_through_the_real_filesystem() {
        let dir = std::env::temp_dir().join(format!("opendrop-native-test-config-{}", std::process::id()));
        let path = dir.join("ui.json");
        let config = UiConfig { theme: ThemeId::OpenDropClassic, obs_port: 4455, ..UiConfig::default() };

        save_config(Some(&path), &config);
        let restored = load_config(&path);

        assert_eq!(restored, config);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_config_with_no_path_is_a_silent_no_op() {
        save_config(None, &UiConfig::default()); // must not panic
    }

    #[test]
    fn panel_id_round_trips_every_variant() {
        let variants = [
            PanelId::Decks,
            PanelId::PresetBrowser,
            PanelId::Playlists,
            PanelId::Audio,
            PanelId::Quality,
            PanelId::Keymap,
            PanelId::Snapshot,
            PanelId::Timeline,
            PanelId::Output,
            PanelId::Midi,
            PanelId::NdiIn,
            PanelId::NdiOut,
            PanelId::Osc,
            PanelId::RemoteWs,
            PanelId::Streaming,
            PanelId::Share,
            PanelId::V4l2,
            PanelId::Video,
            PanelId::CloudPresets,
            PanelId::About,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let restored: PanelId = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, variant);
        }
    }

    #[cfg(feature = "link")]
    #[test]
    fn panel_id_link_round_trips_under_the_link_feature() {
        let json = serde_json::to_string(&PanelId::Link).unwrap();
        let restored: PanelId = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, PanelId::Link);
    }

    #[cfg(not(feature = "link"))]
    #[test]
    fn panel_id_link_wire_value_degrades_to_decks_without_the_link_feature() {
        // Simulates loading a `ui.json` written by a `--features link`
        // build in a default build, where `PanelId::Link` doesn't exist.
        let restored: PanelId = serde_json::from_str("\"Link\"").unwrap();
        assert_eq!(restored, PanelId::Decks);
    }
}
