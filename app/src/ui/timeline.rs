//! Timeline panel: up to 8 keyframes (snapshot slot + time) sequenced
//! across a wall-clock-free playback loop, plus play/pause. Backed by
//! `Show::timeline_keyframes`/`timeline_playing` and the pure
//! `core::timeline::{timeline_loop_duration, timeline_values_at}`
//! (already ported+tested, zero prior usage before this panel, Step 5 of
//! the Phase 8 VJ-panels plan). Port of `SidebarTimeline.svelte` +
//! `timeline-store.svelte.ts`.
//!
//! Keyframe slot/time edits and remove mutate `Show::timeline_keyframes`
//! directly (same "direct field mutation" convention as
//! `ui::quality`/`ui::color`/`ui::composite`), re-sorting by `time_sec`
//! after a time edit settles (`timeline_values_at`'s own doc comment: "callers
//! are responsible for keeping keyframes sorted by time_sec"; sorting on
//! `drag_stopped`/`lost_focus` rather than on every `changed()` frame avoids
//! reordering the list out from under an in-progress drag). Play/pause is
//! the one exception: it goes through
//! `CommandRegistry::dispatch(CommandId::TimelineToggle, ...)` (Recipe B)
//! so a toggle fired from this button takes the same path as
//! keyboard/MIDI/OSC/remote-ws: `CommandContext::toggle_timeline`,
//! mirroring `CommandContext::recall_snapshot`'s precedent (Step 4/
//! `ui::snapshot`).
//!
//! "+ Point" appends a new keyframe defaulting to the first non-empty
//! snapshot slot (or slot 0 if none are filled) at `last_time + 5` seconds
//! (`0` if the list is empty), a direct port of `addTimelineKeyframe` in
//! `timeline-store.svelte.ts`, not a "current playback time" capture:
//! `Show` has no wall clock of its own (see `Show::tick_recall`'s doc
//! comment), so there is no such value to capture, playing or not. The new
//! keyframe's slot and time are then editable inline via the same row
//! controls as every other keyframe.

use opendrop_core::commands::{CommandId, CommandRegistry};
use opendrop_core::show::Show;
use opendrop_core::timeline::{timeline_loop_duration, TimelineKeyframe};

use crate::ui::widgets;

const MAX_KEYFRAMES: usize = 8;

pub fn show(ui: &mut egui::Ui, show: &mut Show, registry: &CommandRegistry) {
    ui.horizontal(|ui| {
        ui.heading("Timeline");
        let can_play = timeline_loop_duration(&show.timeline_keyframes) > 0.0;
        let label = if show.timeline_playing { "⏸ Pause" } else { "▶ Play" };
        if ui.add_enabled(can_play, egui::Button::new(label)).clicked() {
            registry.dispatch(CommandId::TimelineToggle, 1.0, show);
        }
    });

    ui.separator();

    let slot_labels: [String; 8] = std::array::from_fn(|i| match &show.snapshot_slots[i] {
        Some(snapshot) => snapshot.name.clone(),
        None => format!("Slot {} (empty)", i + 1),
    });
    let slot_filled: [bool; 8] = std::array::from_fn(|i| show.snapshot_slots[i].is_some());

    let mut remove_index = None;
    let mut needs_sort = false;

    widgets::dense(ui, |ui| {
        for (i, kf) in show.timeline_keyframes.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("od_timeline_kf_slot").selected_text(slot_labels[kf.slot].clone()).show_ui(
                        ui,
                        |ui| {
                            for slot in 0..8 {
                                ui.add_enabled_ui(slot_filled[slot], |ui| {
                                    if ui.selectable_label(kf.slot == slot, &slot_labels[slot]).clicked() {
                                        kf.slot = slot;
                                    }
                                });
                            }
                        },
                    );

                    let time_response =
                        ui.add(egui::DragValue::new(&mut kf.time_sec).range(0.0..=f64::INFINITY).speed(0.5).suffix("s"));
                    if time_response.drag_stopped() || time_response.lost_focus() {
                        needs_sort = true;
                    }

                    if ui.button("×").clicked() {
                        remove_index = Some(i);
                    }
                });
            });
        }
    });

    if let Some(i) = remove_index {
        show.timeline_keyframes.remove(i);
    }
    if needs_sort {
        show.timeline_keyframes.sort_by(|a, b| a.time_sec.total_cmp(&b.time_sec));
    }

    let can_add = show.timeline_keyframes.len() < MAX_KEYFRAMES;
    if ui.add_enabled(can_add, egui::Button::new("+ Point")).clicked() {
        let last_time = show.timeline_keyframes.last().map_or(-5.0, |kf| kf.time_sec);
        let default_slot = (0..8).find(|&i| slot_filled[i]).unwrap_or(0);
        show.timeline_keyframes.push(TimelineKeyframe { slot: default_slot, time_sec: last_time + 5.0 });
        show.timeline_keyframes.sort_by(|a, b| a.time_sec.total_cmp(&b.time_sec));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use opendrop_core::commands::{create_default_registry, CommandContext};
    use opendrop_core::snapshot::Snapshot;
    use std::collections::HashMap;

    #[test]
    fn show_does_not_panic_with_no_keyframes() {
        themed_test_ui(|ui| {
            let mut state = Show::default();
            let registry = create_default_registry();
            show(ui, &mut state, &registry);
        });
    }

    #[test]
    fn show_does_not_panic_with_populated_keyframes_and_slots() {
        themed_test_ui(|ui| {
            let mut state = Show::default();
            state.snapshot_slots[0] = Some(Snapshot { name: "Intro".to_string(), values: HashMap::from([(CommandId::ColorHueA, 0.0)]) });
            state.snapshot_slots[1] = Some(Snapshot { name: "Drop".to_string(), values: HashMap::from([(CommandId::ColorHueA, 1.0)]) });
            state.timeline_keyframes =
                vec![TimelineKeyframe { slot: 0, time_sec: 0.0 }, TimelineKeyframe { slot: 1, time_sec: 10.0 }];
            let registry = create_default_registry();
            show(ui, &mut state, &registry);
        });
    }

    #[test]
    fn show_does_not_panic_while_playing() {
        themed_test_ui(|ui| {
            let mut state = Show::default();
            state.snapshot_slots[0] = Some(Snapshot { name: "Intro".to_string(), values: HashMap::from([(CommandId::ColorHueA, 0.0)]) });
            state.snapshot_slots[1] = Some(Snapshot { name: "Drop".to_string(), values: HashMap::from([(CommandId::ColorHueA, 1.0)]) });
            state.timeline_keyframes =
                vec![TimelineKeyframe { slot: 0, time_sec: 0.0 }, TimelineKeyframe { slot: 1, time_sec: 10.0 }];
            state.toggle_timeline();
            let registry = create_default_registry();
            show(ui, &mut state, &registry);
        });
    }

    #[test]
    fn show_does_not_panic_at_the_8_keyframe_cap() {
        themed_test_ui(|ui| {
            let mut state = Show::default();
            state.timeline_keyframes = (0..8).map(|i| TimelineKeyframe { slot: 0, time_sec: i as f64 }).collect();
            let registry = create_default_registry();
            show(ui, &mut state, &registry);
        });
    }
}
