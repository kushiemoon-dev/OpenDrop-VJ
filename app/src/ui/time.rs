//! Time panel: the 8 time/warp multipliers for the currently selected deck
//! slot, plus a reset, backed by `Show::time_params` (`DeckTimeParams`,
//! already ported+tested in `core::time_params`).
//! Port of `SidebarTime.svelte` (Step 8 of the Phase 8 VJ-panels plan).
//!
//! Takes the whole `[DeckTimeParams; 4]` plus the already-selected index
//! (`Show::selected_slot`) and only renders/mutates that one slot's group;
//! same shape as `ui::composite`, and for the same reason: the web reference
//! only ever shows the slot named by `mixerSelectedSlot`. All 8 sliders are
//! plain 0-2 in steps of 0.01 with 1 as neutral, exactly the web app's own
//! range, so there is no unit conversion here (unlike `ui::color`'s
//! degrees/percent). Same "direct field mutation, no
//! `CommandRegistry::dispatch`" convention as the sibling panels: the real
//! setters for keyboard/MIDI/OSC/LFO parity are the 32 `CommandId::Time*`
//! commands (see `core::commands`/`core::show`).
//!
//! Unlike Color and Composite, these values are not consumed by this app's
//! own GPU compositor: they are pushed into each deck's *running projectM
//! preset*, one changed value per deck per frame, by the per-deck block in
//! `main.rs`'s `about_to_wait` that calls `next_time_param_to_push` (see
//! `engine::time_patch` for the mechanism and `.planning/TIME-QVAR-SPIKE.md` for why it
//! works that way). Speed is the one
//! exception: it is stored and fully addressable but has no reachable
//! Milkdrop target; `engine::time_patch`'s module docs explain what was
//! measured and why.

use crate::ui::widgets;
use opendrop_core::time_params::{DeckTimeParams, TIME_MULT_MAX};

/// The web reference's slider step (`SidebarTime.svelte`).
const STEP: f64 = 0.01;

pub fn show(ui: &mut egui::Ui, time_params: &mut [DeckTimeParams; 4], selected_slot: usize) {
    let params = &mut time_params[selected_slot];

    ui.horizontal(|ui| {
        ui.heading(format!("Time (slot {selected_slot})"));
        // One-shot action, not a selectable group: plain `ui.button`, same
        // convention as `ui::color`/`ui::composite`'s reset buttons.
        if ui.button("Reset").clicked() {
            *params = DeckTimeParams::default();
        }
    });

    for (label, value) in [
        ("Speed", &mut params.speed_mult),
        ("Zoom", &mut params.zoom_mult),
        ("Rotation", &mut params.rot_mult),
        ("Warp", &mut params.warp_mult),
        ("Horizontal", &mut params.dx_mult),
        ("Vertical", &mut params.dy_mult),
        ("Stretch", &mut params.stretch_mult),
        ("Wave", &mut params.wave_mult),
    ] {
        widgets::micro_label(ui, label);
        ui.add(egui::Slider::new(value, 0.0..=TIME_MULT_MAX).step_by(STEP));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let mut params = [DeckTimeParams::default(); 4];
            show(ui, &mut params, 0);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let mut params = [DeckTimeParams::default(); 4];
                show(ui, &mut params, 3);
            });
        });
    }

    #[test]
    fn show_renders_non_default_params_without_panicking() {
        themed_test_ui(|ui| {
            let mut params = [DeckTimeParams::default(); 4];
            params[2] = DeckTimeParams {
                speed_mult: 0.0,
                zoom_mult: 2.0,
                rot_mult: 0.5,
                warp_mult: 1.75,
                dx_mult: 0.25,
                dy_mult: 1.33,
                stretch_mult: 0.0,
                wave_mult: 2.0,
            };
            show(ui, &mut params, 2);
        });
    }

    #[test]
    fn renders_only_the_selected_slot_and_leaves_the_others_alone() {
        let mut params = [DeckTimeParams::default(); 4];
        params[1].zoom_mult = 1.5;
        themed_test_ui(|ui| {
            show(ui, &mut params, 1);
        });
        assert_eq!(params[1].zoom_mult, 1.5);
        assert_eq!(params[0], DeckTimeParams::default());
        assert_eq!(params[2], DeckTimeParams::default());
        assert_eq!(params[3], DeckTimeParams::default());
    }
}
