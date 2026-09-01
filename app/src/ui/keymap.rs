//! Keymap panel: one row per registered command showing its currently
//! assigned key (or "—"), a Learn button, and a Clear button, plus a
//! reset-to-defaults action: port of `SidebarKeymap.svelte` (Step 3 of the
//! Phase 8 VJ-panels plan).
//!
//! Same Learn/Clear row shape as `ui::midi`, but the capture side is
//! simpler: MIDI capture happens asynchronously on a separate IO thread, so
//! `ui::midi` only sets `midi_learning` here and a later `about_to_wait`
//! diff (`midi_learn_completed`) detects the commit. Keyboard events are
//! already synchronous on the main thread, so there's nothing to diff:
//! clicking Learn just sets `AppState::keymap_learning`; the very next
//! accepted key press commits the binding directly inside `main.rs`'s
//! `WindowEvent::KeyboardInput` handler (intercepted ahead of normal
//! dispatch) and clears `keymap_learning` there. This panel never inserts
//! into `keymap` itself.
//!
//! No `CommandRegistry::dispatch` for the buttons themselves: same
//! "direct field mutation" convention as `ui::quality`/`ui::color`/
//! `ui::composite`: Learn only sets `keymap_learning`, Clear/reset mutate
//! `keymap` directly.
//!
//! `keymap` is `Key`-keyed (one physical key -> one command, required for
//! `main.rs`'s per-frame dispatch lookup), not `CommandId`-keyed, so a
//! command *could* end up with more than one key bound to it (`n`/`N` both
//! map to `PlaylistNextActive` in `keymap::default_keymap`). The "assigned
//! key" column below shows only the first match found while walking the
//! map: an arbitrary but deterministic-per-run tie-break, same judgment
//! call `ui::midi` doesn't have to make (`MidiMapping` is `CommandId`-keyed,
//! so it's always exactly one trigger per command there).

use std::collections::HashMap;

use opendrop_core::commands::{CommandId, CommandRegistry};
use winit::keyboard::Key;

use crate::keymap::{default_keymap, format_key};
use crate::ui::widgets;

pub fn show(ui: &mut egui::Ui, keymap: &mut HashMap<Key, CommandId>, keymap_learning: &mut Option<CommandId>, registry: &CommandRegistry) {
    ui.horizontal(|ui| {
        ui.heading("Keymap");
        // One-shot action, not a selectable group: plain `ui.button`, same
        // convention as `ui::color`/`ui::composite`'s reset button.
        if ui.button("Reset to defaults").clicked() {
            *keymap = default_keymap();
            *keymap_learning = None;
        }
    });

    let commands = registry.all();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        widgets::dense(ui, |ui| {
            for cmd in commands {
                ui.horizontal(|ui| {
                    ui.label(cmd.label);

                    let mut assigned_key: Option<&Key> = None;
                    for (key, id) in keymap.iter() {
                        if *id == cmd.id {
                            assigned_key = Some(key);
                            break;
                        }
                    }
                    let assigned = assigned_key.map(format_key);
                    ui.label(assigned.as_deref().unwrap_or("—"));

                    let is_learning = *keymap_learning == Some(cmd.id);
                    let learn_label = if is_learning { "press a key…" } else { "Learn" };
                    if ui.add_enabled(!is_learning, egui::Button::new(learn_label)).clicked() {
                        *keymap_learning = Some(cmd.id);
                    }
                    if is_learning && ui.button("Cancel").clicked() {
                        *keymap_learning = None;
                    }
                    if ui.button("Clear").clicked() {
                        keymap.retain(|_, id| *id != cmd.id);
                    }
                });
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use opendrop_core::commands::create_default_registry;

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let mut keymap = default_keymap();
            let mut keymap_learning = None;
            let registry = create_default_registry();
            show(ui, &mut keymap, &mut keymap_learning, &registry);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let mut keymap = default_keymap();
                let mut keymap_learning = None;
                let registry = create_default_registry();
                show(ui, &mut keymap, &mut keymap_learning, &registry);
            });
        });
    }

    #[test]
    fn show_while_learning_and_with_an_empty_keymap_does_not_panic() {
        themed_test_ui(|ui| {
            let mut keymap: HashMap<Key, CommandId> = HashMap::new();
            let mut keymap_learning = Some(CommandId::DeckSwitch);
            let registry = create_default_registry();
            show(ui, &mut keymap, &mut keymap_learning, &registry);
        });
    }
}
