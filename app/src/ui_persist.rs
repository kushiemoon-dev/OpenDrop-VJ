//! Writes every UI-controlled setting into `ui.json`.
//!
//! Called on exit and right after a theme change, so a crash after the
//! switch cannot lose it. It reads the file first and overwrites only the
//! fields the app owns, so anything it does not manage is kept.

use crate::{config, keymap, theme, AppState};

/// `theme` is the one just picked; `None` keeps the one already on disk.
pub(crate) fn save_ui_config(state: &AppState, theme: Option<theme::registry::ThemeId>) {
    let config_path = config::config_file_path();
    let mut ui_config = config_path.as_deref().map(config::load_config).unwrap_or_default();
    if let Some(id) = theme {
        ui_config.theme = id;
    }
    ui_config.active_panel = state.active_panel.into();
    ui_config.stage_mode = state.stage_mode;
    ui_config.output_monitor = state.selected_output_monitor.clone();
    ui_config.audio_input_device = state.selected_input_device.clone();
    ui_config.osc_port = state.osc_port;
    ui_config.osc_out_host = state.osc_out_host.clone();
    ui_config.osc_out_port = state.osc_out_port;
    ui_config.rkbx_link_port = state.rkbx_link_port;
    ui_config.obs_host = state.obs_host.clone();
    ui_config.obs_port = state.obs_port;
    ui_config.twitch_channel = state.twitch_channel.clone();
    ui_config.kick_channel = state.kick_channel.clone();
    ui_config.cloud_presets_api_url =
        if state.cloud_presets_api_url.trim().is_empty() { None } else { Some(state.cloud_presets_api_url.clone()) };
    ui_config.cloud_video_api_url =
        if state.cloud_video_api_url.trim().is_empty() { None } else { Some(state.cloud_video_api_url.clone()) };
    ui_config.invisible_mode = state.invisible_mode;
    ui_config.fps_override = state.fps_override;
    ui_config.hide_flagged_presets = state.hide_flagged;
    ui_config.video_links = state.video_links.clone();
    ui_config.preset_dir = non_empty(&state.preset_dir_setting);
    ui_config.texture_dir = non_empty(&state.texture_dir_setting);
    ui_config.check_for_updates = state.check_for_updates;
    ui_config.last_update_check = state.last_update_check;
    ui_config.known_release = state.known_release.clone();
    ui_config.track_presets = state.track_memory.presets_map().clone();
    ui_config.track_order = state.track_memory.order().cloned().collect();
    ui_config.track_auto_recall = state.track_auto_recall;
    ui_config.preset_tags = state.preset_annotations.tags_map().clone();
    ui_config.preset_notes = state.preset_annotations.notes_map().clone();
    ui_config.preset_history = state.preset_history.names().map(String::from).collect();
    ui_config.audio_gain_db = state.audio_gain_db;
    ui_config.audio_agc = state.audio_agc;
    ui_config.beat_sensitivity = state.beat_sensitivity;
    ui_config.ui_scale = state.ui_scale;
    ui_config.keymap = keymap::keymap_to_wire(&state.keymap);
    config::save_config(config_path.as_deref(), &ui_config);
}

/// A setting typed into a text box: blank means "not set".
pub(crate) fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_setting_is_not_set() {
        assert_eq!(non_empty(""), None);
        assert_eq!(non_empty("  \t"), None);
        assert_eq!(non_empty(" /music/presets "), Some("/music/presets".to_string()));
    }
}
