//! Quality panel: target-FPS buttons, invisible-deck power mode selector,
//! and per-deck mesh-size presets (Step 20 of the plan).
//!
//! Takes individual `AppState` fields, not `&mut AppState`: same reasoning
//! as the other panels (see `ui::decks`, `ui::audio`). Mesh-size clicks only
//! record the requested size in `pending_mesh_size`; the actual
//! `Deck::set_mesh_size` FFI call happens later, in `about_to_wait`'s
//! per-deck loop, at the point that deck's context is already current: this
//! panel never touches a `Deck` or a GL context directly.

use crate::InvisibleMode;
use opendrop_engine::deck;
use std::time::Duration;

/// Mesh-size presets, ported from `quality.ts:17,20,25`
/// (`meshWidth`/`meshHeight` only).
const MESH_LOW: (usize, usize) = (32, 24);
const MESH_MEDIUM: (usize, usize) = (48, 36);
const MESH_HIGH: (usize, usize) = (64, 48);

pub fn show(
    ui: &mut egui::Ui,
    refresh_interval: &mut Duration,
    invisible_mode: &mut InvisibleMode,
    pending_mesh_size: &mut [Option<(usize, usize)>; deck::DECK_COUNT],
) {
    ui.label("Target FPS");
    ui.horizontal(|ui| {
        for fps in [30u32, 45, 60] {
            if ui.button(fps.to_string()).clicked() {
                *refresh_interval = Duration::from_secs_f64(1.0 / fps as f64);
            }
        }
    });

    ui.separator();

    ui.label("Invisible deck mode");
    ui.horizontal(|ui| {
        ui.selectable_value(invisible_mode, InvisibleMode::Eco, "Eco");
        ui.selectable_value(invisible_mode, InvisibleMode::Pause, "Pause");
        ui.selectable_value(invisible_mode, InvisibleMode::Off, "Off");
    });

    ui.separator();

    ui.label("Mesh size");
    ui.horizontal(|ui| {
        for i in 0..deck::DECK_COUNT {
            ui.push_id(i, |ui| {
                ui.vertical(|ui| {
                    ui.label(format!("Deck {i}"));
                    if ui.button("Low").clicked() {
                        pending_mesh_size[i] = Some(MESH_LOW);
                    }
                    if ui.button("Medium").clicked() {
                        pending_mesh_size[i] = Some(MESH_MEDIUM);
                    }
                    if ui.button("High").clicked() {
                        pending_mesh_size[i] = Some(MESH_HIGH);
                    }
                });
            });
        }
    });
}
