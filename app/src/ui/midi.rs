//! MIDI panel: connect toggle + input-port dropdown, and one row per
//! mappable command showing its current trigger, a Learn button, and a
//! Clear button (Task 8 of the plan).
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels (`ui::decks`, `ui::audio`, `ui::output`). Display only:
//! soft-takeover, LED flash/persistent state, clock sync, and hotplug LED
//! replay all live in `main.rs`'s `about_to_wait` (Ruling A of the task):
//! this panel just reads `MidiHandle::latest()` and sends control messages.
//!
//! No "currently selected port" is tracked on `AppState`: `MidiSnapshot`
//! doesn't report it either, so the port dropdown can't highlight the
//! active selection, only list `device_names` and dispatch `SelectPort` on
//! click. A judgment call: acceptable since nothing else in this panel
//! needs that state.

use opendrop_core::commands::{CommandId, CommandRegistry};
use opendrop_io::midi::{MidiControl, MidiHandle, MidiTriggerKey, TriggerKind};

fn format_trigger(key: &MidiTriggerKey) -> String {
    match key.kind {
        TriggerKind::Cc => format!("CC{} ch{}", key.number, key.channel),
        TriggerKind::Note => format!("Note{} ch{}", key.number, key.channel),
        TriggerKind::Pitchbend => format!("PitchBend ch{}", key.channel),
    }
}

pub fn show(ui: &mut egui::Ui, midi: &MidiHandle, registry: &CommandRegistry, midi_learning: &mut Option<CommandId>) {
    let snapshot = midi.latest();

    ui.horizontal(|ui| {
        ui.label(if snapshot.connected { "MIDI: connected" } else { "MIDI: disconnected" });
        if snapshot.connected {
            if ui.button("Disconnect").clicked() {
                let _ = midi.control_tx.send(MidiControl::Disconnect);
            }
        } else if ui.button("Connect").clicked() {
            let _ = midi.control_tx.send(MidiControl::Connect);
        }
    });

    ui.label("Input port");
    if snapshot.device_names.is_empty() {
        ui.label("(no MIDI ports found)");
    } else {
        egui::ComboBox::from_id_salt("midi_input_port")
            .selected_text("select a port")
            .show_ui(ui, |ui| {
                for name in &snapshot.device_names {
                    if ui.selectable_label(false, name).clicked() {
                        let _ = midi.control_tx.send(MidiControl::SelectPort(name.clone()));
                    }
                }
            });
    }

    ui.separator();

    let mut commands = registry.all();
    commands.sort_by_key(|cmd| cmd.label);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for cmd in commands {
            ui.horizontal(|ui| {
                ui.label(cmd.label);
                let trigger_text = snapshot.mapping.get(&cmd.id).map(format_trigger).unwrap_or_else(|| "not mapped".to_string());
                ui.label(trigger_text);

                let is_learning = *midi_learning == Some(cmd.id);
                let learn_label = if is_learning { "waiting..." } else { "Learn" };
                if ui.add_enabled(!is_learning, egui::Button::new(learn_label)).clicked() {
                    let _ = midi.control_tx.send(MidiControl::StartLearn(cmd.id));
                    *midi_learning = Some(cmd.id);
                }
                if ui.button("Clear").clicked() {
                    let _ = midi.control_tx.send(MidiControl::ClearMapping(cmd.id));
                }
            });
        }
    });
}
