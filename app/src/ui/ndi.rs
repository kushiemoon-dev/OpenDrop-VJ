//! NDI output panel: a composite toggle + 4 per-deck toggles, plus the
//! mandatory NDI trademark attribution (Task 10 of the plan). Extended with
//! an NDI-in source selector (Task 12): a dropdown of discovered sources
//! plus a Connect/Disconnect toggle, mirroring `ui::midi::show`'s
//! connect-toggle pattern.
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels (`ui::decks`, `ui::midi`, `ui::output`). Stream names are
//! fixed (`"OpenDrop"` / `"OpenDrop Deck N"`), not user-configurable: the
//! brief says either is fine and there's no other panel field to hold a
//! per-stream name.
//!
//! `composite_active`/`deck_active` are the caller's own toggle state, but
//! resynced from `NdiSnapshot::composite_active`/`deck_active` at the top of
//! every `show` call (whole-branch review Finding M5): a sender that failed
//! to start (SDK error) or died mid-session leaves the snapshot's flag
//! false on its own, and without this resync the checkbox would stay
//! checked forever for a stream that isn't actually running. The resync
//! runs before the checkbox widgets below are drawn, so a click made this
//! same frame still takes effect and is still sent to the thread.
//!
//! `selected_source` is likewise the caller's own state (`AppState::
//! ndi_in_selected_source`), not derived from `NdiSnapshot`, which has no
//! "currently selected" concept, only `sources` (discovered) and
//! `receive_active` (whether a receive session is actually running).
//! Discovery itself is started once at bootstrap (`main.rs`'s `bootstrap`,
//! Task 12), not from this panel: by the time this panel is ever shown,
//! `NdiSnapshot::sources` is already populated (or empty, if nothing is on
//! the network yet).

use opendrop_engine::deck;
use opendrop_io::ndi::{NdiControl, NdiHandle, NdiSource};

const COMPOSITE_STREAM_NAME: &str = "OpenDrop";

fn deck_stream_name(slot: usize) -> String {
    format!("OpenDrop Deck {}", slot + 1)
}

pub fn show(
    ui: &mut egui::Ui,
    ndi: &NdiHandle,
    composite_active: &mut bool,
    deck_active: &mut [bool; deck::DECK_COUNT],
    selected_source: &mut Option<NdiSource>,
) {
    let snapshot = ndi.latest();

    // See the module doc comment: resync before the checkboxes are drawn so
    // an external failure (or a start that never succeeded) is visible,
    // without discarding a click made this same frame.
    *composite_active = snapshot.composite_active;
    for (active, &snapshot_active) in deck_active.iter_mut().zip(snapshot.deck_active.iter()) {
        *active = snapshot_active;
    }

    ui.label("NDI output");

    if ui.checkbox(composite_active, "NDI compositor output").changed() {
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
    ui.label("NDI input");

    ui.horizontal(|ui| {
        ui.label(if snapshot.receive_active { "Receive: connected" } else { "Receive: disconnected" });
        if snapshot.receive_active {
            if ui.button("Disconnect").clicked() {
                let _ = ndi.control_tx.send(NdiControl::StopReceive);
            }
        } else if ui.add_enabled(selected_source.is_some(), egui::Button::new("Connect")).clicked() {
            if let Some(source) = selected_source.clone() {
                let _ = ndi.control_tx.send(NdiControl::StartReceive(source));
            }
        }
    });

    if snapshot.sources.is_empty() {
        ui.label("(no sources found)");
    } else {
        // Disabled while a receive is active: matches `ui::osc`/
        // `ui::streaming`'s convention of disabling fields while their
        // respective connection is live (whole-branch review Finding M9).
        ui.add_enabled_ui(!snapshot.receive_active, |ui| {
            egui::ComboBox::from_id_salt("ndi_in_source")
                .selected_text(selected_source.as_ref().map(|s| s.name.as_str()).unwrap_or("select a source"))
                .show_ui(ui, |ui| {
                    for source in &snapshot.sources {
                        let is_selected = selected_source.as_ref() == Some(source);
                        if ui.selectable_label(is_selected, &source.name).clicked() {
                            *selected_source = Some(source.clone());
                        }
                    }
                });
        });
    }

    ui.separator();
    ui.hyperlink_to("ndi.video", "https://ndi.video");
    ui.label("NDI® is a registered trademark of Vizrt NDI AB");
}
