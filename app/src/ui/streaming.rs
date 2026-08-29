//! Streaming panel: OBS WebSocket connect/disconnect + scene control (Task
//! 16 of the plan). Twitch and Kick sections are added by Task 17, in this
//! same file: see PHASE5-IO.PLAN's découpage note for why the three share
//! one file rather than three separate ones (comparable size to the other
//! single-file panels once all three sections exist, not yet the case with
//! only OBS built).
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels (`ui::osc`, `ui::remote`). `obs_host`/`obs_port` are the
//! panel's own editable fields (`AppState::obs_host`/`obs_port`), read by
//! `Connect` at click time: same reasoning as `ui::osc`'s `osc_port`:
//! `ObsSnapshot` has no "what the user is currently typing" concept, only
//! `connected`/`scenes`.
//!
//! Scene switching is one button per scene name (not a dropdown+Go): each
//! click dispatches `ObsControl::SetScene` immediately, and `ObsSnapshot`
//! carries no "current scene" field to highlight a selection against
//! (Override: `CurrentProgramSceneChanged`, OBS->app, isn't ported: see
//! `opendrop_io::obs`'s module doc comment), so there's nothing for a
//! dropdown's "selected" state to reflect anyway.

use opendrop_io::obs::{ObsControl, ObsHandle};

pub fn show(ui: &mut egui::Ui, obs: &ObsHandle, obs_host: &mut String, obs_port: &mut u16) {
    let snapshot = obs.latest();

    ui.label("OBS");

    ui.horizontal(|ui| {
        ui.label("Host");
        ui.add_enabled(!snapshot.connected, egui::TextEdit::singleline(obs_host).desired_width(140.0));
        ui.label("Port");
        ui.add_enabled(!snapshot.connected, egui::DragValue::new(obs_port).range(1..=65535));
    });

    ui.horizontal(|ui| {
        ui.label(if snapshot.connected { "OBS: connected" } else { "OBS: not connected" });
        if snapshot.connected {
            if ui.button("Disconnect").clicked() {
                let _ = obs.control_tx.send(ObsControl::Disconnect);
            }
        } else if ui.button("Connect").clicked() {
            let _ = obs.control_tx.send(ObsControl::Connect(obs_host.clone(), *obs_port));
        }
    });

    if snapshot.connected {
        ui.separator();
        ui.label("Scenes");
        if snapshot.scenes.is_empty() {
            ui.label("(no scenes found)");
        } else {
            for scene in &snapshot.scenes {
                if ui.button(scene).clicked() {
                    let _ = obs.control_tx.send(ObsControl::SetScene(scene.clone()));
                }
            }
        }
    }
}
