//! Ableton Link panel: toggle on/off, tempo/peers display and a tempo
//! field to push into the Link session (Task 18 of the plan).
//!
//! This entire file only exists in a build with the `link` Cargo
//! feature enabled: see `#[cfg(feature = "link")]` on this module's
//! `pub mod link;` declaration in `ui/mod.rs`. With the feature off
//! (the default), this file is never parsed, matching `opendrop_io::
//! link`'s own feature-gating (see that module's doc comment).
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels (`ui::osc`, `ui::midi`). `link_tempo_input` is the
//! panel's own editable field (`AppState::link_tempo_input`), not
//! `LinkSnapshot::tempo`; same reasoning as `ui::osc`'s `osc_port`: the
//! snapshot's `tempo` reflects the Link session's live tempo, while this
//! field is what the user is currently typing, sent via `SetTempo` only
//! when they click the button.
//!
//! Reskinned (Step 19 of the Phase 7 UI redesign plan): the
//! `label(if snapshot.enabled {...})` enabled/disabled row swaps for
//! `widgets::connection_row`, same substitution as `ui::midi`/`ui::ndi`
//! (Steps 17-18). Not compile-verified in this sandbox (missing cmake for
//! the `link` feature's native dependency): reviewed by careful reading
//! only, same determination as Steps 9/10/17. The Tempo/Peers row and the
//! Set-tempo row are untouched.

use opendrop_io::link::{LinkControl, LinkHandle};

use crate::ui::widgets;

pub fn show(ui: &mut egui::Ui, link: &LinkHandle, link_tempo_input: &mut f64) {
    let snapshot = link.latest();

    ui.horizontal(|ui| {
        widgets::connection_row(ui, "Ableton Link", snapshot.enabled);
        if snapshot.enabled {
            if ui.button("Stop").clicked() {
                let _ = link.control_tx.send(LinkControl::Stop);
            }
        } else if ui.button("Start").clicked() {
            let _ = link.control_tx.send(LinkControl::Start);
        }
    });

    if snapshot.enabled {
        ui.horizontal(|ui| {
            ui.label(format!("Tempo: {:.2} BPM", snapshot.tempo));
            ui.label(format!("Peers: {}", snapshot.peers));
        });
    }

    ui.horizontal(|ui| {
        ui.label("Set tempo");
        ui.add(egui::DragValue::new(link_tempo_input).range(20.0..=999.0).speed(1.0));
        if ui.button("Send").clicked() {
            let _ = link.control_tx.send(LinkControl::SetTempo(*link_tempo_input));
        }
    });
}
