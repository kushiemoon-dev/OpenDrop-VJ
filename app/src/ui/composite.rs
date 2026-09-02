//! Composite panel: blend-mode dropdown + luma-key + color-key controls for
//! the currently selected deck slot, backed by `Show::slot_composites`
//! (`SlotComposite`, already ported+tested in `core::blend`, already
//! consumed by the GPU compositor every frame via `blend_state_for`/
//! `LayerInput.composite`: zero engine change here).
//! Port of `SidebarComposite.svelte` (Step 2 of the Phase 8 VJ-panels plan).
//!
//! Unlike Color (one group of controls per deck, both shown side by side),
//! the web reference only ever shows the one slot named by
//! `mixerSelectedSlot`: slot selection itself lives elsewhere in the UI
//! (the deck cards), so this panel takes the whole `[SlotComposite; 4]`
//! array plus the already-selected index (`Show::selected_slot`) and only
//! renders/mutates that one slot's group. `luma_black`/`luma_white`/
//! `color_hue`/`color_tol` are all stored (and displayed) 0..1 directly, no
//! unit conversion: matches the web reference's raw `min=0 max=1` sliders
//! (unlike Color's hue/percent conversion, which exists there to match the
//! web app's degree/percent-labeled sliders). Same "direct field mutation,
//! no `CommandRegistry::dispatch`" convention as `ui::quality`/`ui::color`:
//! real setters for keyboard/MIDI/OSC/LFO parity live on `CommandContext`
//! instead (see `core::commands`/`core::show`).

use crate::ui::widgets;
use opendrop_core::blend::{BlendMode, SlotComposite, DEFAULT_SLOT_COMPOSITE};

const BLEND_MODES: [BlendMode; 4] = [BlendMode::Normal, BlendMode::Additive, BlendMode::Screen, BlendMode::Multiply];

pub fn show(ui: &mut egui::Ui, slot_composites: &mut [SlotComposite; 4], selected_slot: usize) {
    let composite = &mut slot_composites[selected_slot];

    ui.horizontal(|ui| {
        ui.heading(format!("Composite (slot {selected_slot})"));
        // One-shot action, not a selectable group: plain `ui.button`, same
        // convention as `ui::color`'s reset button.
        if ui.button("Reset").clicked() {
            *composite = DEFAULT_SLOT_COMPOSITE;
        }
    });

    widgets::micro_label(ui, "Blend");
    egui::ComboBox::from_id_salt("od_composite_blend").selected_text(format!("{:?}", composite.blend)).show_ui(ui, |ui| {
        for mode in BLEND_MODES {
            if ui.selectable_label(composite.blend == mode, format!("{mode:?}")).clicked() {
                composite.blend = mode;
            }
        }
    });

    ui.checkbox(&mut composite.luma_key, "Luma Key");
    if composite.luma_key {
        widgets::micro_label(ui, "Black");
        ui.add(egui::Slider::new(&mut composite.luma_black, 0.0..=1.0));
        widgets::micro_label(ui, "White");
        ui.add(egui::Slider::new(&mut composite.luma_white, 0.0..=1.0));
    }

    ui.checkbox(&mut composite.color_key, "Color Key");
    if composite.color_key {
        widgets::micro_label(ui, "Hue");
        ui.add(egui::Slider::new(&mut composite.color_hue, 0.0..=1.0));
        widgets::micro_label(ui, "Tolerance");
        ui.add(egui::Slider::new(&mut composite.color_tol, 0.0..=1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let mut slots = [DEFAULT_SLOT_COMPOSITE; 4];
            show(ui, &mut slots, 0);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let mut slots = [DEFAULT_SLOT_COMPOSITE; 4];
                show(ui, &mut slots, 0);
            });
        });
    }

    #[test]
    fn show_renders_a_non_default_selected_slot_with_both_keys_open_without_panicking() {
        themed_test_ui(|ui| {
            let mut slots = [DEFAULT_SLOT_COMPOSITE; 4];
            slots[2] = SlotComposite {
                blend: BlendMode::Screen,
                luma_key: true,
                luma_black: 0.2,
                luma_white: 0.8,
                color_key: true,
                color_hue: 0.5,
                color_tol: 0.3,
            };
            show(ui, &mut slots, 2);
        });
    }
}
