//! Playlists panel: per-deck (A/B) playlist transport/lock/item list, mode/
//! interval controls, and per-deck beat-sync toggle + trigger config. Port
//! of `SidebarPlaylist.svelte` (Step 18 of the plan). The shared BPM
//! display, Tap Tempo/Clear, beats-per-change selector, and auto-crossfade
//! toggle that used to sit here moved into the header's hand-painted
//! mini-transport (Step 10 of the Phase 7 UI redesign plan, `ui::shell::
//! header`): this panel no longer renders them.
//!
//! Takes individual fields, not `&mut AppState`, same reasoning as
//! `ui::decks`/`ui::preset_browser`: the call site (`main.rs`'s
//! `about_to_wait`) already holds `state.egui_glow` mutably borrowed for the
//! `run()` closure, so this needs disjoint borrows of just the fields it
//! touches. Transport/lock/list/beat-sync/trigger controls call `Show`/
//! `PlaylistStore` methods (or fields) directly, bypassing `CommandContext`,
//! same legitimacy as the preset browser's direct `+A`/`+B` calls: `Show`
//! stays UI-manipulable directly for controls that have no keyboard binding.
//!
//! `mode` and `interval_sec` are single `PlaylistStore` fields, not one per
//! deck (`playlist.rs:170-171`, re-read by `toggle_playlist` on every call),
//! so this panel shows one shared mode/interval control above the two
//! per-deck sections, not two independent ones.

use opendrop_core::beat_trigger::{apply_beat_trigger_patch, BeatTriggerConfigPatch, BeatTriggerMode};
use opendrop_core::commands::Deck;
use opendrop_core::playlist::PlaylistMode;
use opendrop_core::show::Show;

pub fn show(ui: &mut egui::Ui, show: &mut Show) {
    ui.horizontal(|ui| {
        ui.label("Mode");
        if ui.selectable_label(show.playlists.mode == PlaylistMode::Sequential, "Sequential").clicked() {
            show.playlists.mode = PlaylistMode::Sequential;
        }
        if ui.selectable_label(show.playlists.mode == PlaylistMode::Shuffle, "Shuffle").clicked() {
            show.playlists.mode = PlaylistMode::Shuffle;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Interval (s)");
        ui.add(egui::Slider::new(&mut show.playlists.interval_sec, 2.0..=120.0));
    });

    ui.separator();

    ui.columns(2, |columns| {
        deck_panel(&mut columns[0], show, Deck::A);
        deck_panel(&mut columns[1], show, Deck::B);
    });
}

/// One deck's (A/B) playlist transport/lock/items + beat-sync/trigger
/// controls.
fn deck_panel(ui: &mut egui::Ui, show: &mut Show, deck: Deck) {
    ui.push_id(deck_label(deck), |ui| {
        ui.heading(deck_label(deck));

        ui.horizontal(|ui| {
            let playing = match deck {
                Deck::A => show.playlists.a_playing,
                Deck::B => show.playlists.b_playing,
            };
            if ui.button(if playing { "Pause" } else { "Play" }).clicked() {
                show.playlists.toggle_playlist(deck);
            }
            if ui.button("Prev").clicked() {
                show.playlists.playlist_prev(deck);
            }
            if ui.button("Next").clicked() {
                show.playlists.playlist_next(deck);
            }
            match deck {
                Deck::A => ui.toggle_value(&mut show.lock_a, "🔒"),
                Deck::B => ui.toggle_value(&mut show.lock_b, "🔒"),
            };
        });

        // Cloned out so the list below doesn't hold a borrow of
        // `show.playlists` across the `remove_from_playlist` call each ×
        // button needs, same pattern as the preset browser's search
        // results.
        let items: Vec<String> = match deck {
            Deck::A => show.playlists.a_items.clone(),
            Deck::B => show.playlists.b_items.clone(),
        };
        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
            for name in &items {
                ui.horizontal(|ui| {
                    ui.label(name);
                    if ui.button("×").clicked() {
                        show.playlists.remove_from_playlist(deck, name);
                    }
                });
            }
        });

        ui.separator();

        let synced = match deck {
            Deck::A => show.beat_sync_a,
            Deck::B => show.beat_sync_b,
        };
        if ui.selectable_label(synced, "Beat Sync").clicked() {
            show.toggle_beat_sync(deck);
        }

        trigger_config(ui, show, deck);
    });
}

/// Trigger-config controls (mode, beats/change with ÷2/×2, offset,
/// sensitivity) for one deck. Every mutation goes through
/// `apply_beat_trigger_patch` (already tested in `core`), which re-clamps
/// `beats_per_change`/`offset` together; never a raw field assignment.
fn trigger_config(ui: &mut egui::Ui, show: &mut Show, deck: Deck) {
    let mut trigger = match deck {
        Deck::A => show.beat_trigger_a,
        Deck::B => show.beat_trigger_b,
    };

    ui.horizontal(|ui| {
        if ui.selectable_label(trigger.mode == BeatTriggerMode::Beat, "Beat").clicked() {
            trigger = apply_beat_trigger_patch(trigger, BeatTriggerConfigPatch { mode: Some(BeatTriggerMode::Beat), ..Default::default() });
        }
        if ui.selectable_label(trigger.mode == BeatTriggerMode::VolumePeak, "Volume Peak").clicked() {
            trigger =
                apply_beat_trigger_patch(trigger, BeatTriggerConfigPatch { mode: Some(BeatTriggerMode::VolumePeak), ..Default::default() });
        }
    });

    ui.horizontal(|ui| {
        ui.label("Beats/change");
        let mut beats = trigger.beats_per_change as i64;
        if ui.add(egui::Slider::new(&mut beats, 1..=64)).changed() {
            trigger = apply_beat_trigger_patch(trigger, BeatTriggerConfigPatch { beats_per_change: Some(beats), ..Default::default() });
        }
        if ui.button("÷2").clicked() {
            trigger = apply_beat_trigger_patch(
                trigger,
                BeatTriggerConfigPatch { beats_per_change: Some(trigger.beats_per_change as i64 / 2), ..Default::default() },
            );
        }
        if ui.button("×2").clicked() {
            trigger = apply_beat_trigger_patch(
                trigger,
                BeatTriggerConfigPatch { beats_per_change: Some(trigger.beats_per_change as i64 * 2), ..Default::default() },
            );
        }
    });

    ui.horizontal(|ui| {
        ui.label("Offset");
        let mut offset = trigger.offset as i64;
        let max_offset = (trigger.beats_per_change as i64 - 1).max(0);
        if ui.add(egui::Slider::new(&mut offset, 0..=max_offset)).changed() {
            trigger = apply_beat_trigger_patch(trigger, BeatTriggerConfigPatch { offset: Some(offset), ..Default::default() });
        }
    });

    if trigger.mode == BeatTriggerMode::VolumePeak {
        ui.horizontal(|ui| {
            ui.label("Sensitivity");
            let mut sensitivity = trigger.sensitivity;
            if ui.add(egui::Slider::new(&mut sensitivity, 0.0..=1.0)).changed() {
                trigger = apply_beat_trigger_patch(trigger, BeatTriggerConfigPatch { sensitivity: Some(sensitivity), ..Default::default() });
            }
        });
    }

    match deck {
        Deck::A => show.beat_trigger_a = trigger,
        Deck::B => show.beat_trigger_b = trigger,
    }
}

fn deck_label(deck: Deck) -> &'static str {
    match deck {
        Deck::A => "Deck A",
        Deck::B => "Deck B",
    }
}
