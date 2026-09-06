//! Color panel: 5 per-deck (A/B) sliders (hue, saturation, brightness,
//! contrast, invert) plus a reset button, backed by `Show::color_params_a/
//! b` (`ColorParams`, already ported+tested in `core::blend`, already
//! consumed by the GPU compositor every frame, zero engine change here).
//! Port of `SidebarColor.svelte`.
//!
//! `ColorParams`'s fields are all stored 0..1 (see that struct's own field
//! comments for what each maps to: hue 0..1->0..360deg, saturate/
//! brightness/contrast 0..1->0..200%, invert 0..1->0..100%). This panel
//! converts to/from those human-facing ranges around each `egui::Slider`
//! so the displayed units (degrees, percent) match the web app, then
//! writes the normalized 0..1 value into the field. Same "direct field
//! mutation, no `CommandRegistry::dispatch`" convention as `ui::quality`;
//! real setters for keyboard/MIDI/OSC/LFO parity live on `CommandContext`
//! instead (see `core::commands`/`core::show`).

use crate::ui::widgets;
use opendrop_core::blend::{ColorParams, DEFAULT_COLOR_PARAMS};

pub fn show(ui: &mut egui::Ui, params_a: &mut ColorParams, params_b: &mut ColorParams) {
    ui.columns(2, |columns| {
        deck_sliders(&mut columns[0], "Deck A", params_a);
        deck_sliders(&mut columns[1], "Deck B", params_b);
    });
}

/// One deck's 5 sliders + reset, `push_id`-scoped on `label` so Deck A and
/// Deck B's identically-labeled sliders don't collide (same idiom as
/// `ui::playlists::deck_panel`).
fn deck_sliders(ui: &mut egui::Ui, label: &str, params: &mut ColorParams) {
    ui.push_id(label, |ui| {
        ui.heading(label);

        let mut hue_deg = params.hue_rotate * 360.0;
        widgets::micro_label(ui, "Hue");
        if ui.add(egui::Slider::new(&mut hue_deg, 0.0..=360.0).suffix("°")).changed() {
            params.hue_rotate = (hue_deg / 360.0).clamp(0.0, 1.0);
        }

        let mut saturate_pct = params.saturate * 200.0;
        widgets::micro_label(ui, "Saturation");
        if ui.add(egui::Slider::new(&mut saturate_pct, 0.0..=200.0).suffix("%")).changed() {
            params.saturate = (saturate_pct / 200.0).clamp(0.0, 1.0);
        }

        let mut brightness_pct = params.brightness * 200.0;
        widgets::micro_label(ui, "Brightness");
        if ui.add(egui::Slider::new(&mut brightness_pct, 0.0..=200.0).suffix("%")).changed() {
            params.brightness = (brightness_pct / 200.0).clamp(0.0, 1.0);
        }

        let mut contrast_pct = params.contrast * 200.0;
        widgets::micro_label(ui, "Contrast");
        if ui.add(egui::Slider::new(&mut contrast_pct, 0.0..=200.0).suffix("%")).changed() {
            params.contrast = (contrast_pct / 200.0).clamp(0.0, 1.0);
        }

        let mut invert_pct = params.invert * 100.0;
        widgets::micro_label(ui, "Invert");
        if ui.add(egui::Slider::new(&mut invert_pct, 0.0..=100.0).suffix("%")).changed() {
            params.invert = (invert_pct / 100.0).clamp(0.0, 1.0);
        }

        // One-shot action, not a selectable group: plain `ui.button`, same
        // convention as `ui::quality`'s mesh-size buttons.
        if ui.button("Reset").clicked() {
            *params = DEFAULT_COLOR_PARAMS;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let mut params_a = DEFAULT_COLOR_PARAMS;
            let mut params_b = DEFAULT_COLOR_PARAMS;
            show(ui, &mut params_a, &mut params_b);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let mut params_a = DEFAULT_COLOR_PARAMS;
                let mut params_b = DEFAULT_COLOR_PARAMS;
                show(ui, &mut params_a, &mut params_b);
            });
        });
    }

    #[test]
    fn show_renders_non_default_params_without_panicking() {
        themed_test_ui(|ui| {
            let mut params_a =
                ColorParams { hue_rotate: 0.75, saturate: 1.0, brightness: 0.1, contrast: 0.9, invert: 1.0 };
            let mut params_b =
                ColorParams { hue_rotate: 0.25, saturate: 0.0, brightness: 1.0, contrast: 0.0, invert: 0.5 };
            show(ui, &mut params_a, &mut params_b);
        });
    }
}
