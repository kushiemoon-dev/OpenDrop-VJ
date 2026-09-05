//! Rekordbox Link panel (ticket #10 "Synchronised music video playback"):
//! the rkbx_link OSC bridge's port + Start/Stop + connection status,
//! mirroring `ui::osc` exactly for that half, plus the DJ-deck-to-visual-
//! deck mapping new to this ticket.

use opendrop_core::show::Show;
use opendrop_io::rkbx_link::{RkbxLinkControl, RkbxLinkHandle, MAX_DJ_DECKS};

use crate::ui::widgets;

pub fn show(
    ui: &mut egui::Ui,
    rkbx_link: &RkbxLinkHandle,
    rkbx_link_port: &mut u16,
    mapping_error: &mut Option<String>,
    show: &mut Show,
) {
    let snapshot = rkbx_link.latest();

    ui.horizontal(|ui| {
        ui.label("Port");
        ui.add_enabled(!snapshot.listening, egui::DragValue::new(rkbx_link_port).range(1..=65535));
    });
    ui.horizontal(|ui| {
        widgets::connection_row(ui, "rkbx_link", snapshot.listening);
        if snapshot.listening {
            ui.label(format!("on port {}", snapshot.port));
            if ui.button("Stop").clicked() {
                let _ = rkbx_link.control_tx.send(RkbxLinkControl::Stop);
            }
        } else if ui.button("Start").clicked() {
            let _ = rkbx_link.control_tx.send(RkbxLinkControl::Start(*rkbx_link_port));
        }
    });

    ui.separator();
    if let Some(err) = mapping_error.as_deref() {
        widgets::error_banner(ui, err);
    }
    ui.label("DJ deck mapping");
    for dj_deck in 0..MAX_DJ_DECKS {
        ui.horizontal(|ui| {
            ui.label(format!("DJ deck {dj_deck}"));
            let current = show.rkbx_deck_mapping[dj_deck];
            egui::ComboBox::from_id_salt(format!("od_rkbx_map_{dj_deck}"))
                .selected_text(match current {
                    None => "Unmapped".to_string(),
                    Some(slot) => format!("Deck {slot}"),
                })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(current.is_none(), "Unmapped").clicked() {
                        let _ = show.set_rkbx_deck_mapping(dj_deck, None);
                        *mapping_error = None;
                    }
                    for slot in 0..4 {
                        if ui.selectable_label(current == Some(slot), format!("Deck {slot}")).clicked() {
                            match show.set_rkbx_deck_mapping(dj_deck, Some(slot)) {
                                Ok(()) => *mapping_error = None,
                                Err(e) => *mapping_error = Some(e),
                            }
                        }
                    }
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    #[test]
    fn show_does_not_panic_with_every_dj_deck_unmapped() {
        themed_test_ui(|ui| {
            let rkbx_link = opendrop_io::rkbx_link::spawn();
            let mut port = 4460u16;
            let mut error = None;
            let mut state = Show::default();
            show(ui, &rkbx_link, &mut port, &mut error, &mut state);
        });
    }

    #[test]
    fn show_does_not_panic_with_one_dj_deck_mapped() {
        themed_test_ui(|ui| {
            let rkbx_link = opendrop_io::rkbx_link::spawn();
            let mut port = 4460u16;
            let mut error = None;
            let mut state = Show::default();
            state.set_rkbx_deck_mapping(0, Some(1)).unwrap();
            show(ui, &rkbx_link, &mut port, &mut error, &mut state);
        });
    }

    #[test]
    fn show_does_not_panic_with_a_mapping_conflict_error_set() {
        themed_test_ui(|ui| {
            let rkbx_link = opendrop_io::rkbx_link::spawn();
            let mut port = 4460u16;
            let mut state = Show::default();
            state.set_rkbx_deck_mapping(0, Some(1)).unwrap();
            let result = state.set_rkbx_deck_mapping(2, Some(1));
            assert!(result.is_err());
            let mut error = result.err();
            show(ui, &rkbx_link, &mut port, &mut error, &mut state);
        });
    }
}
