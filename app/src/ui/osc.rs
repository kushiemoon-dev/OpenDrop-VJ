//! OSC panel: a port field + Start/Stop button, and a listening indicator.
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels (`ui::midi`, `ui::ndi`, `ui::output`). `osc_port` is the
//! panel's own editable field (`AppState::osc_port`), not
//! `OscSnapshot::port`; same reasoning as `ui::ndi`'s `selected_source`:
//! the snapshot's `port` reflects the port the thread actually bound
//! (only meaningful once `listening` is true), while this field is what
//! the user is currently typing, which Start reads at click time.
//!
//! No soft-takeover, no mapping/learn UI: unlike MIDI, OSC has no such
//! concept in the existing app.
//!
//! Reskinned: the listening row
//! swaps its `label(if snapshot.listening {...})` branch for `widgets::
//! connection_row`, same substitution as `ui::midi`/`ui::ndi` (Steps
//! 17-18). Unlike those, `snapshot.listening`'s label carried the bound
//! port too, so that detail is kept as a plain label right after the
//! `connection_row` pill rather than dropped. The port `DragValue` above
//! is untouched.

use opendrop_io::osc::{OscControl, OscHandle};

use crate::ui::widgets;

pub fn show(ui: &mut egui::Ui, osc: &OscHandle, osc_port: &mut u16) {
    let snapshot = osc.latest();

    ui.horizontal(|ui| {
        ui.label("Port");
        ui.add_enabled(!snapshot.listening, egui::DragValue::new(osc_port).range(1..=65535));
    });

    ui.horizontal(|ui| {
        widgets::connection_row(ui, "OSC", snapshot.listening);
        if snapshot.listening {
            ui.label(format!("on port {}", snapshot.port));
            if ui.button("Stop").clicked() {
                let _ = osc.control_tx.send(OscControl::Stop);
            }
        } else if ui.button("Start").clicked() {
            let _ = osc.control_tx.send(OscControl::Start(*osc_port));
        }
    });
}
