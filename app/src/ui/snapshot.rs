//! Snapshot panel: a recall-duration slider plus 8 named save/recall/clear
//! slots, backed by `Show::snapshot_slots`/`snapshot_recall_duration_sec`
//! (already ported+tested in `core::snapshot`: `Snapshot`, `smoothstep`,
//! `interpolate_snapshot`; and `core::show`: `capture_snapshot_values`,
//! `recall_snapshot`, `tick_recall`). Port of `SidebarSnapshot.svelte`
//! (Step 4 of the Phase 8 VJ-panels plan).
//!
//! Save/rename/Clear mutate `Show::snapshot_slots` directly, same "direct
//! field mutation" convention as `ui::quality`/`ui::color`/`ui::composite`.
//! Recall is the one exception in this panel: it must go through
//! `CommandRegistry::dispatch(CommandId::RecallSnapshotN, ...)` (Recipe B)
//! instead of a direct field write, so a recall fired from this button
//! takes the exact same path (`CommandContext::recall_snapshot` capturing
//! `start_values` and arming `Show::active_recall`) as one fired from
//! keyboard/MIDI/OSC/remote-ws (the plan's own cross-cutting parity
//! requirement). That's also why this panel takes `&mut Show` as a whole
//! (`perform.show`) rather than individual `&mut` fields like the other
//! Recipe A panels: dispatch needs `&mut dyn CommandContext`, which only a
//! whole `Show` can provide.

use opendrop_core::commands::{CommandId, CommandRegistry};
use opendrop_core::show::Show;
use opendrop_core::snapshot::Snapshot;

use crate::ui::widgets;

const RECALL_IDS: [CommandId; 8] = [
    CommandId::RecallSnapshot0,
    CommandId::RecallSnapshot1,
    CommandId::RecallSnapshot2,
    CommandId::RecallSnapshot3,
    CommandId::RecallSnapshot4,
    CommandId::RecallSnapshot5,
    CommandId::RecallSnapshot6,
    CommandId::RecallSnapshot7,
];

pub fn show(ui: &mut egui::Ui, show: &mut Show, registry: &CommandRegistry) {
    ui.heading("Snapshot");

    widgets::micro_label(ui, "Recall duration");
    ui.add(egui::Slider::new(&mut show.snapshot_recall_duration_sec, 0.1..=10.0).step_by(0.1).suffix("s"));

    ui.separator();

    widgets::dense(ui, |ui| {
        for (i, &recall_id) in RECALL_IDS.iter().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    match &mut show.snapshot_slots[i] {
                        Some(snapshot) => {
                            ui.text_edit_singleline(&mut snapshot.name);
                        }
                        None => {
                            ui.label(format!("Slot {}", i + 1));
                        }
                    }

                    if ui.button("Save").clicked() {
                        let values = show.capture_snapshot_values();
                        let name = match &show.snapshot_slots[i] {
                            Some(existing) => existing.name.clone(),
                            None => format!("Slot {}", i + 1),
                        };
                        show.snapshot_slots[i] = Some(Snapshot { name, values });
                    }

                    let has_snapshot = show.snapshot_slots[i].is_some();
                    if ui.add_enabled(has_snapshot, egui::Button::new("Recall")).clicked() {
                        registry.dispatch(recall_id, 1.0, show);
                    }
                    if ui.add_enabled(has_snapshot, egui::Button::new("Clear")).clicked() {
                        show.snapshot_slots[i] = None;
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
    use opendrop_core::commands::{create_default_registry, CommandContext};
    use std::collections::HashMap;

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let mut state = Show::default();
            let registry = create_default_registry();
            show(ui, &mut state, &registry);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let mut state = Show::default();
                let registry = create_default_registry();
                show(ui, &mut state, &registry);
            });
        });
    }

    #[test]
    fn show_renders_populated_slots_and_an_active_recall_without_panicking() {
        themed_test_ui(|ui| {
            let mut state = Show::default();
            state.snapshot_slots[0] =
                Some(Snapshot { name: "Intro".to_string(), values: HashMap::from([(CommandId::ColorHueA, 0.5)]) });
            let registry = create_default_registry();
            state.recall_snapshot(0);
            show(ui, &mut state, &registry);
        });
    }
}
