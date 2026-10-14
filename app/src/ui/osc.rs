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
use opendrop_io::osc_out::OscOut;

use crate::ui::widgets;

/// The OSC output side: where to send the app's state, and the live sender.
pub struct OscOutSettings<'a> {
    pub sender: &'a mut Option<OscOut>,
    pub host: &'a mut String,
    pub port: &'a mut u16,
    pub error: &'a mut Option<String>,
}

pub fn show(ui: &mut egui::Ui, osc: &OscHandle, osc_port: &mut u16, out: OscOutSettings<'_>) {
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

    ui.separator();
    ui.label("Send state to a controller");
    let running = out.sender.is_some();
    ui.horizontal(|ui| {
        ui.label("Host");
        ui.add_enabled(!running, egui::TextEdit::singleline(out.host).desired_width(140.0));
        ui.label("Port");
        ui.add_enabled(!running, egui::DragValue::new(out.port).range(1..=65535));
    });
    ui.horizontal(|ui| {
        widgets::connection_row(ui, "OSC out", running);
        if running {
            if ui.button("Stop").clicked() {
                *out.sender = None;
            }
        } else if ui.button("Start").clicked() {
            match OscOut::connect(out.host, *out.port) {
                Ok(sender) => {
                    *out.sender = Some(sender);
                    *out.error = None;
                }
                Err(e) => *out.error = Some(e),
            }
        }
    });
    if let Some(error) = out.error.as_deref() {
        widgets::micro_label(ui, error);
    }
}
