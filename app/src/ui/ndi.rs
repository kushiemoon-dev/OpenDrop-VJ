//! NDI output panel: a composite toggle + 4 per-deck toggles, plus the
//! mandatory NDI trademark attribution (Task 10 of the plan).
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels (`ui::decks`, `ui::midi`, `ui::output`). Stream names are
//! fixed (`"OpenDrop"` / `"OpenDrop Deck N"`), not user-configurable: the
//! brief says either is fine and there's no other panel field to hold a
//! per-stream name.
//!
//! `composite_active`/`deck_active` are the caller's own toggle state (not
//! `NdiSnapshot`, which reflects whether the sender actually started, e.g.
//! could be `false` after an SDK failure): this panel drives them and the
//! caller derives `AppState::ndi_active` from them each frame.

use opendrop_io::ndi::{NdiControl, NdiHandle};

const COMPOSITE_STREAM_NAME: &str = "OpenDrop";

fn deck_stream_name(slot: usize) -> String {
    format!("OpenDrop Deck {}", slot + 1)
}

pub fn show(ui: &mut egui::Ui, ndi: &NdiHandle, composite_active: &mut bool, deck_active: &mut [bool; 4]) {
    ui.label("NDI output");

    if ui.checkbox(composite_active, "Sortie NDI compositeur").changed() {
        let msg = if *composite_active {
            NdiControl::StartComposite(COMPOSITE_STREAM_NAME.to_string())
        } else {
            NdiControl::StopComposite
        };
        let _ = ndi.control_tx.send(msg);
    }

    ui.separator();
    ui.label("Decks");
    for (i, active) in deck_active.iter_mut().enumerate() {
        if ui.checkbox(active, format!("Deck {}", i + 1)).changed() {
            let msg =
                if *active { NdiControl::StartDeck(i, deck_stream_name(i)) } else { NdiControl::StopDeck(i) };
            let _ = ndi.control_tx.send(msg);
        }
    }

    ui.separator();
    ui.hyperlink_to("ndi.video", "https://ndi.video");
    ui.label("NDI® is a registered trademark of Vizrt NDI AB");
}
