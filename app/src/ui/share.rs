//! Share panel: port of `SidebarShare.svelte` (Step 13 of the Phase 8 VJ-
//! panels plan): a set-name field, a "copy link" button, and a count of
//! overlays the link can't carry (only text overlays survive a URL, see
//! `opendrop_core::share_set::filter_shareable_overlays`).
//!
//! The missing piece the web source didn't need (browser `Compression
//! Stream`/`Blob`/`Response` do the gzip+base64url for it) is
//! `opendrop_io::share_codec::encode_shared_set`: real I/O-shaped
//! encoding, out of `core`'s zero-dependency scope, added by this step.
//!
//! Read-only over `Show`: unlike the Recipe B panels (Snapshot's Recall,
//! for instance), building a link is a one-shot export, not a live-state
//! change that needs keyboard/MIDI/OSC/remote-ws parity: no new
//! `CommandId`, no `CommandRegistry::dispatch`.
//!
//! `SharedSet::preset_a`/`preset_b` have no direct `Show` equivalent (only
//! `Show::preset_catalog` + the private per-bus browsing cursor exist, and
//! those track catalog *browsing*, not what's actually loaded), resolved
//! here from `deck_preset_names` (the name actually loaded on each
//! physical slot, already threaded through `PerformCtx` for the Decks
//! panel) via `show.deck_bus`: the first slot routed to that bus, empty
//! string if none is. `SharedSet::transition_time` mirrors
//! `AppState::transition_seconds` (`PerformCtx`'s existing field, the
//! soft-cut duration `ui::decks`' transition rail already controls), the
//! same concept, TS-named differently.

use opendrop_core::share_set::{filter_shareable_overlays, SharedSet};
use opendrop_core::show::{DeckBus, Show};

use crate::ui::widgets;

pub fn show(ui: &mut egui::Ui, show: &Show, deck_preset_names: &[String; 4], transition_seconds: f64, share_set_name: &mut String) {
    ui.heading("Share");

    widgets::micro_label(ui, "Name");
    ui.text_edit_singleline(share_set_name);

    ui.separator();

    let shareable_overlays = filter_shareable_overlays(&show.overlay_store.overlays);
    let non_shareable_count = show.overlay_store.overlays.len() - shareable_overlays.len();
    if non_shareable_count > 0 {
        widgets::micro_label(
            ui,
            &format!("{non_shareable_count} overlay(s) can't be shared (only text overlays travel in a link)"),
        );
    }

    if ui.button("Copy Link").clicked() {
        let preset_a = preset_name_for_bus(show, deck_preset_names, DeckBus::A);
        let preset_b = preset_name_for_bus(show, deck_preset_names, DeckBus::B);

        let set = SharedSet {
            name: share_set_name.clone(),
            preset_a,
            preset_b,
            deck_bus: show.deck_bus,
            crossfader: show.crossfader,
            transition_time: transition_seconds,
            color_params_a: show.color_params_a,
            color_params_b: show.color_params_b,
            slot_composites: show.slot_composites,
            time_params: show.time_params,
            q_var_params: show.q_var_params,
            snapshots: show.snapshot_slots.clone(),
            snapshot_recall_duration: show.snapshot_recall_duration_sec,
            timeline_keyframes: show.timeline_keyframes.clone(),
            overlays: shareable_overlays.into_iter().cloned().collect(),
            beat_trigger_a: show.beat_trigger_a,
            beat_trigger_b: show.beat_trigger_b,
            beat_sync_a: show.beat_sync_a,
            beat_sync_b: show.beat_sync_b,
            overlay_queue_enabled: show.overlay_store.queue_enabled,
            overlay_queue_trigger: show.overlay_store.queue_trigger,
        };

        match opendrop_io::share_codec::encode_shared_set(&set) {
            Ok(link) => ui.ctx().copy_text(link),
            Err(e) => eprintln!("[app] share link encode failed: {e}"),
        }
    }
}

/// Name currently loaded on the first physical slot routed to `bus`, or
/// empty if no slot is; see this module's doc comment on why `Show` alone
/// can't answer this.
fn preset_name_for_bus(show: &Show, deck_preset_names: &[String; 4], bus: DeckBus) -> String {
    show.deck_bus.iter().position(|&b| b == bus).map(|i| deck_preset_names[i].clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use opendrop_core::overlay::{make_overlay, OverlayPatch};

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let state = Show::default();
            let names: [String; 4] = std::array::from_fn(|_| String::new());
            let mut name = String::new();
            show(ui, &state, &names, 0.0, &mut name);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let state = Show::default();
                let names: [String; 4] = std::array::from_fn(|_| String::new());
                let mut name = String::new();
                show(ui, &state, &names, 0.0, &mut name);
            });
        });
    }

    #[test]
    fn show_renders_the_non_shareable_overlay_count_without_panicking() {
        themed_test_ui(|ui| {
            let mut state = Show::default();
            state.overlay_store.overlays.push(make_overlay(
                "id-text".to_string(),
                "Text".to_string(),
                OverlayPatch { kind: Some(opendrop_core::overlay::OverlayKind::Text), ..Default::default() },
            ));
            state.overlay_store.overlays.push(make_overlay("id-media".to_string(), "img.png".to_string(), OverlayPatch::default()));
            let names: [String; 4] = std::array::from_fn(|i| format!("Preset {i}"));
            let mut name = "My set".to_string();
            show(ui, &state, &names, 1.5, &mut name);
        });
    }

    #[test]
    fn preset_name_for_bus_finds_the_first_matching_slot_and_defaults_to_empty() {
        let mut state = Show::default();
        state.deck_bus = [DeckBus::Off, DeckBus::A, DeckBus::A, DeckBus::Off];
        let names: [String; 4] = ["P0".to_string(), "P1".to_string(), "P2".to_string(), "P3".to_string()];

        assert_eq!(preset_name_for_bus(&state, &names, DeckBus::A), "P1");
        assert_eq!(preset_name_for_bus(&state, &names, DeckBus::B), "");
    }
}
