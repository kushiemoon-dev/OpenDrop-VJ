//! Spout output panel (Windows): a composite toggle and one per deck, each
//! sharing its frames with other applications under a fixed sender name.
//!
//! Like `ui::ndi`, the toggles are resynced from the sender thread's snapshot
//! before they are drawn, so a stream that failed to start (no GPU, Spout
//! unavailable) shows as off together with the reason.

use opendrop_engine::deck;
use opendrop_io::spout::{SpoutHandle, SpoutSlot};

use crate::ui::widgets;

const COMPOSITE_SENDER_NAME: &str = "OpenDrop";

fn deck_sender_name(slot: usize) -> String {
    format!("OpenDrop Deck {}", slot + 1)
}

pub fn show_out(
    ui: &mut egui::Ui,
    spout: &SpoutHandle,
    composite_active: &mut bool,
    deck_active: &mut [bool; deck::DECK_COUNT],
) {
    let snapshot = spout.latest();
    *composite_active = snapshot.composite_active;
    for (active, &snapshot_active) in deck_active.iter_mut().zip(snapshot.deck_active.iter()) {
        *active = snapshot_active;
    }

    ui.label("Spout output");
    if !cfg!(windows) {
        widgets::micro_label(ui, "Spout is available on Windows only.");
    }

    ui.add_enabled_ui(cfg!(windows), |ui| {
        if ui.checkbox(composite_active, "Spout compositor output").changed() {
            if *composite_active {
                spout.start(SpoutSlot::Composite, COMPOSITE_SENDER_NAME.to_string());
            } else {
                spout.stop(SpoutSlot::Composite);
            }
        }

        ui.separator();
        ui.label("Decks");
        for (i, active) in deck_active.iter_mut().enumerate() {
            if ui.checkbox(active, format!("Deck {}", i + 1)).changed() {
                if *active {
                    spout.start(SpoutSlot::Deck(i), deck_sender_name(i));
                } else {
                    spout.stop(SpoutSlot::Deck(i));
                }
            }
        }
    });

    if let Some(error) = &snapshot.last_error {
        ui.separator();
        widgets::micro_label(ui, &format!("Last error: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    #[test]
    fn deck_sender_names_are_one_based() {
        assert_eq!(deck_sender_name(0), "OpenDrop Deck 1");
        assert_eq!(deck_sender_name(3), "OpenDrop Deck 4");
    }

    #[test]
    fn show_out_does_not_panic() {
        themed_test_ui(|ui| {
            let spout = opendrop_io::spout::spawn();
            let mut composite = false;
            let mut decks = [false; deck::DECK_COUNT];
            show_out(ui, &spout, &mut composite, &mut decks);
        });
    }
}
