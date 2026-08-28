//! Decks panel: 4 deck cards (live GPU thumbnail, preset name/status,
//! active-slot highlight, bus-cycle button), plus the crossfader and
//! transition-seconds controls. Port of `MixerLayout.svelte` (Step 16 of
//! the plan).
//!
//! Takes individual `AppState` fields rather than `&mut AppState` as a
//! whole: the call site (`main.rs`'s `about_to_wait`) already holds
//! `state.egui_glow` mutably borrowed for the `run()` closure, so this
//! needs disjoint borrows of just the fields it touches.

use opendrop_core::show::{DeckBus, Show};
use std::collections::{HashMap, HashSet};

const THUMB_SIZE: egui::Vec2 = egui::vec2(160.0, 90.0);

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    show: &mut Show,
    deck_tex_ids: &[egui::TextureId; 4],
    deck_preset_names: &[String; 4],
    pending_validations: &HashSet<usize>,
    preset_errors: &HashMap<usize, String>,
    transition_seconds: &mut f64,
) {
    ui.horizontal(|ui| {
        for i in 0..4 {
            deck_card(ui, i, show, deck_tex_ids, deck_preset_names, pending_validations, preset_errors);
        }
    });

    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Crossfader");
        ui.add(egui::Slider::new(&mut show.crossfader, 0.0..=1.0));
        ui.label(format!("A {:.0}% / B {:.0}%", (1.0 - show.crossfader) * 100.0, show.crossfader * 100.0));
    });

    ui.horizontal(|ui| {
        ui.label("Transition (s)");
        ui.add(egui::Slider::new(transition_seconds, 0.0..=5.0));
        if ui.button("Hard Cut").clicked() {
            *transition_seconds = 0.0;
        }
    });
}

/// One deck card. The bus-cycle button is laid out in its own row below the
/// thumbnail/name block, so its rect never overlaps the block's: the
/// click-to-select `Sense` added to that block (below) and the button's own
/// click never compete for the same pointer event.
#[allow(clippy::too_many_arguments)]
fn deck_card(
    ui: &mut egui::Ui,
    i: usize,
    show: &mut Show,
    deck_tex_ids: &[egui::TextureId; 4],
    deck_preset_names: &[String; 4],
    pending_validations: &HashSet<usize>,
    preset_errors: &HashMap<usize, String>,
) {
    let is_active = i == show.selected_slot;
    let mut frame = egui::Frame::group(ui.style());
    if is_active {
        frame = frame.stroke(egui::Stroke::new(2.0, egui::Color32::YELLOW));
    }

    ui.push_id(i, |ui| {
        frame.show(ui, |ui| {
            let card = ui.vertical(|ui| {
                ui.image((deck_tex_ids[i], THUMB_SIZE));
                if pending_validations.contains(&i) {
                    ui.label("Validating…");
                } else if let Some(err) = preset_errors.get(&i) {
                    ui.colored_label(egui::Color32::RED, err);
                } else {
                    ui.label(&deck_preset_names[i]);
                }
            });
            if card.response.interact(egui::Sense::click()).clicked() {
                show.select_slot(i);
            }

            let bus_label = match show.deck_bus[i] {
                DeckBus::A => "Bus: A",
                DeckBus::B => "Bus: B",
                DeckBus::Off => "Bus: Off",
            };
            if ui.button(bus_label).clicked() {
                show.deck_bus[i] = show.deck_bus[i].next();
            }
        });
    });
}
