//! MIDI panel: connect toggle + input-port dropdown, and one row per
//! mappable command showing its current trigger, a Learn button, and a
//! Clear button.
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels (`ui::decks`, `ui::audio`, `ui::output`). Display only:
//! soft-takeover, LED flash/persistent state, clock sync, and hotplug LED
//! replay all live in `main.rs`'s `about_to_wait`;
//! this panel just reads `MidiHandle::latest()` and sends control messages.
//!
//! No "currently selected port" is tracked on `AppState`: `MidiSnapshot`
//! doesn't report it either, so the port dropdown can't highlight the
//! active selection, only list `device_names` and dispatch `SelectPort` on
//! click. A judgment call: acceptable since nothing else in this panel
//! needs that state.
//!
//! Reskinned: the connection row
//! swaps its `label(if snapshot.connected {...})` branch for `widgets::
//! connection_row`, which reports the same connected/offline state through
//! a `pill` (theme's `ok`/`dim` colors) instead of plain text. The MIDI
//! learn rows (the `for cmd in commands` loop inside the `ScrollArea`) are
//! wrapped in `widgets::dense`, one of 3 permanently-dense zones
//! (with the presets grid and Playlists). The connection row and port
//! dropdown above stay at the default airy spacing, since they aren't part
//! of that fixed-dense zone. The port `ComboBox` is untouched, already
//! re-themed automatically.
//!
//! Not unit-tested: `MidiHandle`'s only public constructor is `spawn()`
//! (`opendrop_io::midi::handle`), which starts a real background thread.
//! Its `state`/internals are private, so this panel's test module has no
//! way to build a stand-in `MidiHandle` with a chosen `MidiSnapshot`. Its
//! run loop also polls real system MIDI port enumeration on a timer
//! (`check_hotplug`), an external I/O dependency a unit test shouldn't
//! spin up. Same shape and same determination as `ui::audio`'s
//! `AudioHandle`: an unmockable external handle, not faked here.

use opendrop_core::commands::{CommandId, CommandRegistry};
use opendrop_io::midi::{MidiControl, MidiHandle, MidiTriggerKey, TriggerKind};

use crate::ui::widgets;

fn format_trigger(key: &MidiTriggerKey) -> String {
    match key.kind {
        TriggerKind::Cc => format!("CC{} ch{}", key.number, key.channel),
        TriggerKind::Note => format!("Note{} ch{}", key.number, key.channel),
        TriggerKind::Pitchbend => format!("PitchBend ch{}", key.channel),
    }
}

pub fn show(
    ui: &mut egui::Ui,
    midi: &MidiHandle,
    registry: &CommandRegistry,
    midi_learning: &mut Option<(CommandId, Option<MidiTriggerKey>)>,
) {
    let snapshot = midi.latest();

    ui.horizontal(|ui| {
        widgets::connection_row(ui, "MIDI", snapshot.connected);
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

    // `registry.all()` now comes back in the curated `DEFAULT_COMMANDS`
    // grouping (deck controls -> active-deck shortcuts -> ... -> q-vars),
    // matching the 3 reference UI panels; no longer needs the alphabetical
    // `.sort_by_key(|cmd| cmd.label)` workaround this panel used to carry
    // for the registry's old nondeterministic HashMap order.
    let commands = registry.all();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        widgets::dense(ui, |ui| {
            for cmd in commands {
                ui.horizontal(|ui| {
                    ui.label(cmd.label);
                    let trigger_text = snapshot.mapping.get(&cmd.id).map(format_trigger).unwrap_or_else(|| "not mapped".to_string());
                    ui.label(trigger_text);

                    let is_learning = matches!(midi_learning, Some((id, _)) if *id == cmd.id);
                    let learn_label = if is_learning { "waiting..." } else { "Learn" };
                    if ui.add_enabled(!is_learning, egui::Button::new(learn_label)).clicked() {
                        let _ = midi.control_tx.send(MidiControl::StartLearn(cmd.id));
                        // Snapshot the pre-existing mapping entry (if any) so
                        // `about_to_wait` can tell "learn completed" apart from
                        // "still the old entry": StartLearn doesn't clear it.
                        *midi_learning = Some((cmd.id, snapshot.mapping.get(&cmd.id).cloned()));
                    }
                    // Before this, the only
                    // way out of learn mode was a real MIDI message arriving;
                    // no way to back out of a Learn clicked by mistake.
                    if is_learning && ui.button("Cancel").clicked() {
                        let _ = midi.control_tx.send(MidiControl::StopLearn);
                        *midi_learning = None;
                    }
                    if ui.button("Clear").clicked() {
                        let _ = midi.control_tx.send(MidiControl::ClearMapping(cmd.id));
                    }
                });
            }
        });
    });
}
