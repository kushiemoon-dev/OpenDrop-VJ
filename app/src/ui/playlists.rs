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
//!
//! Reskinned (Step 15 of the Phase 7 UI redesign plan): the whole panel is
//! one of the plan's 3 permanently-dense zones (with the presets grid, Step
//! 14, and the MIDI learn rows, Step 17): `widgets::dense` wraps the
//! entire body below, no user-facing toggle. `🔒` is replaced by a plain
//! "LOCK" text label: no emoji in displayed text, even though Step 5's
//! widened fallback chain would still render it. `⇄` (auto-crossfade) is
//! NOT touched here: it already moved out of this file into the header's
//! mini-transport at Step 10 (alongside the BPM/tap block) and still
//! renders as the raw glyph in `ui::shell::header` today, out of this
//! step's scope. Every `ui.selectable_label` pair (mode, beat-sync, trigger
//! mode) is replaced with `widgets::pill`, colored `accent` when selected
//! and `dim` otherwise, clicked via `Response::interact(Sense::click())`:
//! the exact pattern `ui::decks::deck_card` already established for its own
//! 3-state bus badge: rather than `widgets::chip_row`: `chip_row` takes
//! `&[&str]` and returns one aggregate `Response` for the whole row, with
//! no way to learn which chip was clicked or to color one differently from
//! the rest, so it can't preserve the per-option click + selected-highlight
//! behavior these controls need. `chip`/`chip_row` stay reserved for
//! genuinely static tag rows, which this panel doesn't have.
//!
//! Item-row mini-thumbnails (found live post-Phase-7: the plain text list
//! read as less "finished" than Decks/Presets' thumbnail-driven look) reuse
//! `LibraryCtx`'s thumbnail pipeline exactly as `ui::preset_browser::tile`
//! does: same cache lookup, same `thumb_queue` enqueue-on-miss: so a
//! preset already thumbnailed anywhere in the app (browsed, or already on a
//! deck) shows up here too, and one not yet rendered gets queued rather
//! than staying a permanent placeholder. `Metrics::mini_thumb_size` (48x27)
//! is already the engine's own 16:9 cache aspect, so unlike the preset
//! browser's 4:3 tiles this needs no crop, just the plain default UV.

use opendrop_core::beat_trigger::{apply_beat_trigger_patch, BeatTriggerConfigPatch, BeatTriggerMode};
use opendrop_core::commands::Deck;
use opendrop_core::playlist::PlaylistMode;
use opendrop_core::show::Show;
use opendrop_core::thumb_queue::{enqueue_front, ThumbJob};
use std::collections::{HashMap, HashSet};

use crate::ui::ctx::LibraryCtx;
use crate::ui::widgets::{self, theme};

pub fn show(ui: &mut egui::Ui, show: &mut Show, library: &mut LibraryCtx) {
    widgets::dense(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::micro_label(ui, "Mode");
            let t = theme(ui);
            let sequential_color = if show.playlists.mode == PlaylistMode::Sequential { t.palette.accent } else { t.palette.dim };
            if widgets::pill(ui, "Sequential", sequential_color).interact(egui::Sense::click()).clicked() {
                show.playlists.mode = PlaylistMode::Sequential;
            }
            let shuffle_color = if show.playlists.mode == PlaylistMode::Shuffle { t.palette.accent } else { t.palette.dim };
            if widgets::pill(ui, "Shuffle", shuffle_color).interact(egui::Sense::click()).clicked() {
                show.playlists.mode = PlaylistMode::Shuffle;
            }
        });

        ui.horizontal(|ui| {
            widgets::micro_label(ui, "Interval (s)");
            ui.add(egui::Slider::new(&mut show.playlists.interval_sec, 2.0..=120.0));
        });

        ui.separator();

        ui.columns(2, |columns| {
            deck_panel(&mut columns[0], show, Deck::A, library.thumbnail_textures, library.failed_thumbnails, library.thumb_queue);
            deck_panel(&mut columns[1], show, Deck::B, library.thumbnail_textures, library.failed_thumbnails, library.thumb_queue);
        });
    });
}

/// One deck's (A/B) playlist transport/lock/items + beat-sync/trigger
/// controls.
#[allow(clippy::too_many_arguments)]
fn deck_panel(
    ui: &mut egui::Ui,
    show: &mut Show,
    deck: Deck,
    thumbnail_textures: &HashMap<String, egui::TextureHandle>,
    failed_thumbnails: &HashSet<String>,
    thumb_queue: &mut Vec<ThumbJob>,
) {
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
                Deck::A => ui.toggle_value(&mut show.lock_a, "LOCK"),
                Deck::B => ui.toggle_value(&mut show.lock_b, "LOCK"),
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
        let t = theme(ui);
        let mini_size = t.metrics.mini_thumb_size;
        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
            for name in &items {
                ui.horizontal(|ui| {
                    let (rect, _response) = ui.allocate_exact_size(mini_size, egui::Sense::hover());
                    if ui.is_rect_visible(rect) {
                        let corner_radius = egui::CornerRadius::from(t.metrics.radius_sm);
                        if let Some(tex) = thumbnail_textures.get(name) {
                            // `mini_thumb_size` is already 16:9, the same
                            // aspect ratio the engine caches at, so unlike
                            // `preset_browser::tile`'s 4:3 boxes this needs
                            // no crop, just the plain default UV. Rounded
                            // + bordered to match the card/tile look used
                            // everywhere else (found live: a plain
                            // sharp-cornered, borderless image read as
                            // unfinished next to Decks/Presets).
                            ui.put(rect, egui::Image::new((tex.id(), rect.size())).corner_radius(corner_radius));
                        } else if failed_thumbnails.contains(name) {
                            ui.painter().rect_filled(rect, corner_radius, t.palette.error.gamma_multiply(0.3));
                        } else {
                            ui.painter().rect_filled(rect, corner_radius, t.palette.dim);
                            *thumb_queue = enqueue_front(std::mem::take(thumb_queue), ThumbJob { slot_key: name.clone(), name: name.clone() });
                        }
                        ui.painter().rect_stroke(rect, corner_radius, egui::Stroke::new(t.metrics.border_width, t.palette.border), egui::StrokeKind::Outside);
                    }

                    // A bare `ui.label` here has no wrap width to truncate
                    // against inside a horizontal row (egui's `Extend` wrap
                    // mode for non-wrapping horizontal layouts), so a long
                    // preset name renders at its full natural width and
                    // overflows past this column, into Deck B's (found
                    // live). Reserve width for the mini-thumb and the `×`
                    // button first, then bound and truncate the label to
                    // what's left.
                    let button_w = ui.spacing().interact_size.x;
                    ui.set_width((ui.available_width() - button_w).max(0.0));
                    ui.add(egui::Label::new(name).truncate());
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
        let sync_color = if synced { t.palette.accent } else { t.palette.dim };
        // `widgets::pill` paints its label through a raw `ui.label` inside
        // `Frame::show`, which inherits ambient wrap mode (unlike egui's
        // built-in `selectable_label`, the widget this replaced, which
        // sizes to its content regardless of layout). Called bare here,
        // this pill sat directly in the column's vertical layout, so the
        // label's default `Wrap` mode stretched it to the full column
        // width once selected: the "BEAT SYNC" bar filling the whole
        // column found live. The other pills in this file already avoid
        // this because they're called inside an explicit `ui.horizontal`,
        // which resolves to `Extend` (natural width) instead.
        ui.horizontal(|ui| {
            if widgets::pill(ui, "Beat Sync", sync_color).interact(egui::Sense::click()).clicked() {
                show.toggle_beat_sync(deck);
            }
        });

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
        let t = theme(ui);
        let beat_color = if trigger.mode == BeatTriggerMode::Beat { t.palette.accent } else { t.palette.dim };
        if widgets::pill(ui, "Beat", beat_color).interact(egui::Sense::click()).clicked() {
            trigger = apply_beat_trigger_patch(trigger, BeatTriggerConfigPatch { mode: Some(BeatTriggerMode::Beat), ..Default::default() });
        }
        let volume_color = if trigger.mode == BeatTriggerMode::VolumePeak { t.palette.accent } else { t.palette.dim };
        if widgets::pill(ui, "Volume Peak", volume_color).interact(egui::Sense::click()).clicked() {
            trigger =
                apply_beat_trigger_patch(trigger, BeatTriggerConfigPatch { mode: Some(BeatTriggerMode::VolumePeak), ..Default::default() });
        }
    });

    ui.horizontal(|ui| {
        widgets::micro_label(ui, "Beats/change");
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
        widgets::micro_label(ui, "Offset");
        let mut offset = trigger.offset as i64;
        let max_offset = (trigger.beats_per_change as i64 - 1).max(0);
        if ui.add(egui::Slider::new(&mut offset, 0..=max_offset)).changed() {
            trigger = apply_beat_trigger_patch(trigger, BeatTriggerConfigPatch { offset: Some(offset), ..Default::default() });
        }
    });

    if trigger.mode == BeatTriggerMode::VolumePeak {
        ui.horizontal(|ui| {
            widgets::micro_label(ui, "Sensitivity");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use opendrop_core::beat_trigger::default_beat_trigger_config;

    fn sample_show() -> Show {
        Show::default()
    }

    // `show`/`deck_panel` now also need `LibraryCtx`'s thumbnail fields
    // (mini-thumb previews, found live post-Phase-7). Empty collections are
    // enough to exercise every branch below, same testability tier as
    // `ui::preset_browser`'s own tests: no real texture, so the code takes
    // the "not yet thumbnailed" branch and enqueues, never the happy path,
    // but that's still every line of the rendering logic running once.
    fn sample_library() -> (HashMap<String, egui::TextureHandle>, HashSet<String>, Vec<ThumbJob>) {
        (HashMap::new(), HashSet::new(), Vec::new())
    }

    // --- show(): the whole panel. Always internally dense (this file's
    // module doc comment), so the bare and `widgets::dense`-wrapped calls
    // below aren't exercising two different spacing modes, only mirroring
    // every other panel's test shape (`ui::decks`, `ui::preset_browser`)
    // for consistency and to guard against a future `dense` call being
    // accidentally removed from `show()` itself. ---------------------------

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let mut state = sample_show();
            let (thumbnail_textures, failed_thumbnails, mut thumb_queue) = sample_library();
            let mut search_query = String::new();
            let mut search_cache = crate::ui::preset_browser::SearchCache::default();
            let mut load_request = None;
            let mut favorite_presets = HashSet::new();
            let mut favorites_only = false;
            let mut library = LibraryCtx {
                preset_search_query: &mut search_query,
                search_cache: &mut search_cache,
                thumb_queue: &mut thumb_queue,
                thumbnail_textures: &thumbnail_textures,
                failed_thumbnails: &failed_thumbnails,
                load_request: &mut load_request,
                favorite_presets: &mut favorite_presets,
                favorites_only: &mut favorites_only,
            };
            show(ui, &mut state, &mut library);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let mut state = sample_show();
                let (thumbnail_textures, failed_thumbnails, mut thumb_queue) = sample_library();
                let mut search_query = String::new();
                let mut search_cache = crate::ui::preset_browser::SearchCache::default();
                let mut load_request = None;
                let mut favorite_presets = HashSet::new();
                let mut favorites_only = false;
                let mut library = LibraryCtx {
                    preset_search_query: &mut search_query,
                    search_cache: &mut search_cache,
                    thumb_queue: &mut thumb_queue,
                    thumbnail_textures: &thumbnail_textures,
                    failed_thumbnails: &failed_thumbnails,
                    load_request: &mut load_request,
                    favorite_presets: &mut favorite_presets,
                    favorites_only: &mut favorites_only,
                };
                show(ui, &mut state, &mut library);
            });
        });
    }

    // --- show(): non-default state (locked, synced, non-empty playlists,
    // shuffle mode) renders every branch without panicking -----------------

    #[test]
    fn show_renders_locked_synced_and_populated_decks() {
        themed_test_ui(|ui| {
            let mut state = sample_show();
            state.playlists.mode = PlaylistMode::Shuffle;
            state.playlists.a_items = vec!["Alpha".to_string(), "Beta".to_string()];
            state.playlists.b_items = vec!["Gamma".to_string()];
            state.lock_a = true;
            state.beat_sync_a = true;
            state.beat_sync_b = true;
            let (thumbnail_textures, failed_thumbnails, mut thumb_queue) = sample_library();
            let mut search_query = String::new();
            let mut search_cache = crate::ui::preset_browser::SearchCache::default();
            let mut load_request = None;
            let mut favorite_presets = HashSet::new();
            let mut favorites_only = false;
            let mut library = LibraryCtx {
                preset_search_query: &mut search_query,
                search_cache: &mut search_cache,
                thumb_queue: &mut thumb_queue,
                thumbnail_textures: &thumbnail_textures,
                failed_thumbnails: &failed_thumbnails,
                load_request: &mut load_request,
                favorite_presets: &mut favorite_presets,
                favorites_only: &mut favorites_only,
            };
            show(ui, &mut state, &mut library);
        });
    }

    // --- deck_panel(): both decks, A and B, don't panic --------------------

    #[test]
    fn deck_panel_does_not_panic_for_both_decks() {
        themed_test_ui(|ui| {
            let mut state = sample_show();
            let (thumbnail_textures, failed_thumbnails, mut thumb_queue) = sample_library();
            deck_panel(ui, &mut state, Deck::A, &thumbnail_textures, &failed_thumbnails, &mut thumb_queue);
            deck_panel(ui, &mut state, Deck::B, &thumbnail_textures, &failed_thumbnails, &mut thumb_queue);
        });
    }

    // --- trigger_config(): both trigger modes, since the Sensitivity row
    // only renders under VolumePeak ------------------------------------

    #[test]
    fn trigger_config_does_not_panic_in_beat_mode() {
        themed_test_ui(|ui| {
            let mut state = sample_show();
            state.beat_trigger_a = default_beat_trigger_config();
            trigger_config(ui, &mut state, Deck::A);
        });
    }

    #[test]
    fn trigger_config_does_not_panic_in_volume_peak_mode() {
        themed_test_ui(|ui| {
            let mut state = sample_show();
            let mut trigger = default_beat_trigger_config();
            trigger = apply_beat_trigger_patch(trigger, BeatTriggerConfigPatch { mode: Some(BeatTriggerMode::VolumePeak), ..Default::default() });
            state.beat_trigger_b = trigger;
            trigger_config(ui, &mut state, Deck::B);
        });
    }

    // --- deck_label(): stable, human-readable, keyed on the enum not an
    // index --------------------------------------------------------------

    #[test]
    fn deck_label_matches_deck() {
        assert_eq!(deck_label(Deck::A), "Deck A");
        assert_eq!(deck_label(Deck::B), "Deck B");
    }
}
