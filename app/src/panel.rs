use crate::config;

/// Which top-level panel the control window is currently showing. Gates
/// per-tick work that only matters while its panel is visible (the
/// thumbnail pump only runs while `PresetBrowser` is on screen), besides
/// driving `ui_root`'s own tab row.
// `pub(crate)`: read by `ui::ctx::ShellCtx`, a different module from this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Panel {
    #[default]
    Decks,
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
    // Separate variants with their own nav buttons (SOURCES section,
    // `ui::shell::nav`): `NdiIn` renders `ui::ndi::show_in` and `NdiOut`
    // renders `ui::ndi::show_out`.
    NdiIn,
    NdiOut,
    SpoutOut,
    Osc,
    RkbxLink,
    Overlays,
    RemoteWs,
    Streaming,
    Share,
    #[cfg(feature = "link")]
    Link,
    V4l2,
    Video,
    CloudPresets,
    CloudVideo,
    About,
}

/// `config::PanelId` -> `Panel`, for restoring `AppState::active_panel`
/// from a loaded `ui.json` at bootstrap. Kept as an explicit conversion
/// rather than merging the two enums, per `config.rs`'s own module doc
/// comment (`Panel` must not derive `Serialize`/`Deserialize`).
impl From<config::PanelId> for Panel {
    fn from(id: config::PanelId) -> Self {
        match id {
            config::PanelId::Decks => Panel::Decks,
            config::PanelId::PresetBrowser => Panel::PresetBrowser,
            config::PanelId::Playlists => Panel::Playlists,
            config::PanelId::Audio => Panel::Audio,
            config::PanelId::Quality => Panel::Quality,
            config::PanelId::Color => Panel::Color,
            config::PanelId::Composite => Panel::Composite,
            config::PanelId::Keymap => Panel::Keymap,
            config::PanelId::Snapshot => Panel::Snapshot,
            config::PanelId::Timeline => Panel::Timeline,
            config::PanelId::Time => Panel::Time,
            config::PanelId::Qvar => Panel::Qvar,
            config::PanelId::Strobe => Panel::Strobe,
            config::PanelId::Lfo => Panel::Lfo,
            config::PanelId::Output => Panel::Output,
            config::PanelId::Midi => Panel::Midi,
            config::PanelId::NdiIn => Panel::NdiIn,
            config::PanelId::NdiOut => Panel::NdiOut,
            config::PanelId::SpoutOut => Panel::SpoutOut,
            config::PanelId::Osc => Panel::Osc,
            config::PanelId::RkbxLink => Panel::RkbxLink,
            config::PanelId::Overlays => Panel::Overlays,
            config::PanelId::RemoteWs => Panel::RemoteWs,
            config::PanelId::Streaming => Panel::Streaming,
            config::PanelId::Share => Panel::Share,
            #[cfg(feature = "link")]
            config::PanelId::Link => Panel::Link,
            config::PanelId::V4l2 => Panel::V4l2,
            config::PanelId::Video => Panel::Video,
            config::PanelId::CloudPresets => Panel::CloudPresets,
            config::PanelId::CloudVideo => Panel::CloudVideo,
            config::PanelId::About => Panel::About,
        }
    }
}

/// The reverse of the above, for saving `AppState::active_panel` into
/// `UiConfig` on exit.
impl From<Panel> for config::PanelId {
    fn from(panel: Panel) -> Self {
        match panel {
            Panel::Decks => config::PanelId::Decks,
            Panel::PresetBrowser => config::PanelId::PresetBrowser,
            Panel::Playlists => config::PanelId::Playlists,
            Panel::Audio => config::PanelId::Audio,
            Panel::Quality => config::PanelId::Quality,
            Panel::Color => config::PanelId::Color,
            Panel::Composite => config::PanelId::Composite,
            Panel::Keymap => config::PanelId::Keymap,
            Panel::Snapshot => config::PanelId::Snapshot,
            Panel::Timeline => config::PanelId::Timeline,
            Panel::Time => config::PanelId::Time,
            Panel::Qvar => config::PanelId::Qvar,
            Panel::Strobe => config::PanelId::Strobe,
            Panel::Lfo => config::PanelId::Lfo,
            Panel::Output => config::PanelId::Output,
            Panel::Midi => config::PanelId::Midi,
            Panel::NdiIn => config::PanelId::NdiIn,
            Panel::NdiOut => config::PanelId::NdiOut,
            Panel::SpoutOut => config::PanelId::SpoutOut,
            Panel::Osc => config::PanelId::Osc,
            Panel::RkbxLink => config::PanelId::RkbxLink,
            Panel::Overlays => config::PanelId::Overlays,
            Panel::RemoteWs => config::PanelId::RemoteWs,
            Panel::Streaming => config::PanelId::Streaming,
            Panel::Share => config::PanelId::Share,
            #[cfg(feature = "link")]
            Panel::Link => config::PanelId::Link,
            Panel::V4l2 => config::PanelId::V4l2,
            Panel::Video => config::PanelId::Video,
            Panel::CloudPresets => config::PanelId::CloudPresets,
            Panel::CloudVideo => config::PanelId::CloudVideo,
            Panel::About => config::PanelId::About,
        }
    }
}
