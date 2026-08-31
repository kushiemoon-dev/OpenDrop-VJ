//! Quality panel: target-FPS buttons, invisible-deck power mode selector,
//! and per-deck mesh-size presets (Step 20 of the plan).
//!
//! Takes individual `AppState` fields, not `&mut AppState`: same reasoning
//! as the other panels (see `ui::decks`, `ui::audio`). Mesh-size clicks only
//! record the requested size in `pending_mesh_size`; the actual
//! `Deck::set_mesh_size` FFI call happens later, in `about_to_wait`'s
//! per-deck loop, at the point that deck's context is already current: this
//! panel never touches a `Deck` or a GL context directly.
//!
//! Reskinned (Step 17 of the Phase 7 UI redesign plan): the FPS and
//! invisible-mode rows use `widgets::pill` plus `Response::interact(Sense::
//! click())`, not `widgets::chip_row`, following `ui::playlists`' and
//! `ui::decks`' established precedent. `chip_row` takes `&[&str]` and
//! returns one aggregate `Response` for the whole row, with no way to color
//! one chip differently to show which target is currently active. Both
//! rows here are the same shape as playlists' Sequential/Shuffle mode
//! picker: a single mutually-exclusive group. This panel is also the one
//! Step 22's future 30fps/60fps manual verification reads to confirm the
//! active target, so losing the selected-state highlight would make that
//! check unreliable, and `chip_row` structurally can't express per-item
//! selected-state coloring even for a single group with no per-item
//! semantic color otherwise. The mesh-size buttons below stay untouched:
//! one-shot queue actions, not a selectable group, out of this step's
//! scope. Neither row lives inside `widgets::dense`, this panel stays airy
//! by default, not one of the plan's 3 fixed-dense zones.

use crate::ui::widgets::{self, theme};
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
        // Minor #17: selected-state indication, matching the invisible-
        // mode row's pill+interact pattern below, instead of plain buttons
        // that never showed which target was actually active. Rounded
        // rather than compared bit-exact against `refresh_interval` so it
        // still lights up correctly for the value bootstrap derived from
        // the monitor's real refresh rate, not just one set by a click
        // here.
        let current_fps = (1.0 / refresh_interval.as_secs_f64()).round() as u32;
        let t = theme(ui);
        for fps in [30u32, 45, 60] {
            let color = if current_fps == fps { t.palette.accent } else { t.palette.dim };
            if widgets::pill(ui, &fps.to_string(), color).interact(egui::Sense::click()).clicked() {
                *refresh_interval = Duration::from_secs_f64(1.0 / fps as f64);
            }
        }
    });

    ui.separator();

    ui.label("Invisible deck mode");
    ui.horizontal(|ui| {
        let t = theme(ui);
        for (mode, label) in [(InvisibleMode::Eco, "Eco"), (InvisibleMode::Pause, "Pause"), (InvisibleMode::Off, "Off")] {
            let color = if *invisible_mode == mode { t.palette.accent } else { t.palette.dim };
            if widgets::pill(ui, label, color).interact(egui::Sense::click()).clicked() {
                *invisible_mode = mode;
            }
        }
    });

    ui.separator();

    ui.label("Mesh size");
    ui.horizontal(|ui| {
        for (i, mesh_size) in pending_mesh_size.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.vertical(|ui| {
                    ui.label(format!("Deck {i}"));
                    if ui.button("Low").clicked() {
                        *mesh_size = Some(MESH_LOW);
                    }
                    if ui.button("Medium").clicked() {
                        *mesh_size = Some(MESH_MEDIUM);
                    }
                    if ui.button("High").clicked() {
                        *mesh_size = Some(MESH_HIGH);
                    }
                    // Minor #18: a mesh-size change only applies once
                    // `about_to_wait`'s per-deck loop actually renders this
                    // deck, which an invisible deck in `Pause` mode may not
                    // do for a while: previously silent, so a click could
                    // look like it did nothing.
                    if mesh_size.is_some() {
                        ui.label("(queued)");
                    }
                });
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    // `show` takes only plain fields (no external handle, unlike
    // `ui::audio`/`ui::output`/`ui::midi`), same testability tier as
    // `ui::decks`/`ui::playlists`, anticipated already at Step 8.

    // --- show(): the whole panel, airy (default) and dense -----------------

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let mut refresh_interval = Duration::from_secs_f64(1.0 / 60.0);
            let mut invisible_mode = InvisibleMode::Eco;
            let mut pending_mesh_size = [None; deck::DECK_COUNT];
            show(ui, &mut refresh_interval, &mut invisible_mode, &mut pending_mesh_size);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let mut refresh_interval = Duration::from_secs_f64(1.0 / 60.0);
                let mut invisible_mode = InvisibleMode::Eco;
                let mut pending_mesh_size = [None; deck::DECK_COUNT];
                show(ui, &mut refresh_interval, &mut invisible_mode, &mut pending_mesh_size);
            });
        });
    }

    // --- show(): every FPS target and invisible-mode value renders without
    // panicking, including a non-standard refresh rate where no FPS pill
    // matches `current_fps` and a queued mesh size on every deck -----------

    #[test]
    fn show_renders_every_fps_and_invisible_mode() {
        themed_test_ui(|ui| {
            for fps in [30u32, 45, 60] {
                for mode in [InvisibleMode::Eco, InvisibleMode::Pause, InvisibleMode::Off] {
                    let mut refresh_interval = Duration::from_secs_f64(1.0 / fps as f64);
                    let mut invisible_mode = mode;
                    let mut pending_mesh_size = [None; deck::DECK_COUNT];
                    show(ui, &mut refresh_interval, &mut invisible_mode, &mut pending_mesh_size);
                }
            }
        });
    }

    #[test]
    fn show_renders_non_standard_refresh_rate_and_queued_mesh_sizes() {
        themed_test_ui(|ui| {
            // 75fps: none of the 30/45/60 pills match `current_fps`.
            let mut refresh_interval = Duration::from_secs_f64(1.0 / 75.0);
            let mut invisible_mode = InvisibleMode::Pause;
            let mut pending_mesh_size = [Some((32, 24)); deck::DECK_COUNT];
            show(ui, &mut refresh_interval, &mut invisible_mode, &mut pending_mesh_size);
        });
    }
}
