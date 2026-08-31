//! Audio panel: input-device dropdown with hot-swap + VU meter (Step 19 of
//! the plan).
//!
//! Takes individual fields, not `&mut AppState`, same reasoning as the
//! other panels (`ui::decks`, `ui::playlists`, `ui::preset_browser`): the
//! call site (`main.rs`'s `about_to_wait`) already holds `state.egui_glow`
//! mutably borrowed for the `run()` closure, so this needs disjoint borrows
//! of just the fields it touches.
//!
//! `input_devices` is enumerated once at bootstrap via `opendrop_audio::
//! list_input_devices()` (Step 8) and cached on `AppState`. This panel
//! never calls it itself, so picking a device never re-scans the device
//! list. `last_vu_level` is `opendrop_audio::analysis::vu_level`, computed
//! once per tick by `about_to_wait` (Step 18); this panel only reads it,
//! never runs a second `vu_level` pass over the same PCM.

use opendrop_audio::AudioHandle;

use crate::ui::widgets;

pub fn show(
    ui: &mut egui::Ui,
    audio: &AudioHandle,
    input_devices: &Vec<String>,
    selected_input_device: &mut Option<String>,
    last_vu_level: f64,
) {
    ui.horizontal(|ui| {
        ui.label("Input device");
        if input_devices.is_empty() {
            ui.label("(no input devices found)");
        } else {
            egui::ComboBox::from_id_salt("audio_input_device")
                .selected_text(selected_input_device.as_deref().unwrap_or("(default)"))
                .show_ui(ui, |ui| {
                    for name in input_devices {
                        let is_selected = selected_input_device.as_deref() == Some(name.as_str());
                        if ui.selectable_label(is_selected, name).clicked() {
                            *selected_input_device = Some(name.clone());
                            audio.set_device(name.clone());
                        }
                    }
                });
        }
    });

    ui.separator();

    ui.horizontal(|ui| {
        ui.label("VU");
        // `vu_meter` (Step 8, `widgets.rs`) clamps to [0, 1] and colors
        // itself internally, so no clamp is needed here. `last_vu_level`
        // is read as-is, not recomputed. It sizes itself to
        // `ui.available_width()`, which is the full-width usage it was
        // built for (see the narrower `allocate_ui`-wrapped call in
        // `shell.rs`'s dense header for the other case).
        widgets::vu_meter(ui, last_vu_level as f32);
    });
}
