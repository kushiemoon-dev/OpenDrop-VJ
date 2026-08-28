//! Preset Browser panel: search box + scrollable grid of preset tiles with
//! lazy, cached thumbnails, click-to-load through pre-flight validation, and
//! per-tile "+A"/"+B" playlist buttons. Port of `PresetBrowser.svelte` (Step
//! 17 of the plan).
//!
//! Takes individual `AppState`-derived fields, not `&mut AppState` as a
//! whole: same reasoning as `ui::decks`: the call site (`main.rs`'s
//! `about_to_wait`) already holds `state.egui_glow` mutably borrowed for the
//! `run()` closure, so this needs disjoint borrows of just the fields it
//! touches.
//!
//! Loading a preset can't be triggered directly from here:
//! `request_preset_load` (Step 14) needs the whole `AppState`: the
//! preflight channel sender, `path_by_name`, `pending_validations`: none of
//! which this panel owns, for the same reason `ui::decks` doesn't call it
//! either. A click instead writes the clicked name into `*load_request`; the
//! caller reads it back once `egui_glow.run()` returns (`show` is no longer
//! borrowed at that point) and performs the actual `request_preset_load`
//! call: the single validated entry point, never a direct `Deck::
//! load_preset`.

use opendrop_core::commands::Deck;
use opendrop_core::preset_index::search;
use opendrop_core::show::Show;
use opendrop_core::thumb_queue::{enqueue_front, ThumbJob};
use std::collections::HashMap;

/// Matches `opendrop_engine::thumbnail::{THUMB_W, THUMB_H}`'s 192:108
/// aspect ratio, scaled down for a browser tile.
const TILE_SIZE: egui::Vec2 = egui::vec2(96.0, 54.0);

pub fn show(
    ui: &mut egui::Ui,
    show: &mut Show,
    search_query: &mut String,
    thumb_queue: &mut Vec<ThumbJob>,
    thumbnail_textures: &HashMap<String, egui::TextureHandle>,
    load_request: &mut Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label("Search");
        ui.text_edit_singleline(search_query);
    });

    ui.separator();

    // Cloned out to owned Strings so the borrow of `show.preset_catalog`
    // ends here, before the tiles below need `&mut show.playlists` for the
    // +A/+B buttons.
    let results: Vec<String> = search(&show.preset_catalog, search_query.as_str()).into_iter().map(|p| p.name.clone()).collect();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for name in &results {
                tile(ui, show, name, thumb_queue, thumbnail_textures, load_request);
            }
        });
    });
}

/// One preset tile: thumbnail (or placeholder while it's still queued),
/// name, click-to-load, +A/+B playlist buttons.
///
/// The thumbnail request is enqueued at most once per tile per frame: and
/// only while the tile is both actually on-screen (`ui.is_rect_visible`,
/// which checks against the `ScrollArea`'s clip rect: an off-screen tile
/// never reaches this branch) and still missing its texture. Once
/// `pump_thumbnail_queue` (Step 15) fills `thumbnail_textures` for this
/// name, the `else` branch below stops firing on its own: no separate
/// "stop requesting" signal needed.
fn tile(
    ui: &mut egui::Ui,
    show: &mut Show,
    name: &str,
    thumb_queue: &mut Vec<ThumbJob>,
    thumbnail_textures: &HashMap<String, egui::TextureHandle>,
    load_request: &mut Option<String>,
) {
    ui.push_id(name, |ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(TILE_SIZE.x.max(110.0));
            let card = ui.vertical(|ui| {
                let (rect, _response) = ui.allocate_exact_size(TILE_SIZE, egui::Sense::hover());
                if ui.is_rect_visible(rect) {
                    if let Some(tex) = thumbnail_textures.get(name) {
                        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
                        ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
                    } else {
                        ui.painter().rect_filled(rect, 2.0, egui::Color32::DARK_GRAY);
                        *thumb_queue = enqueue_front(std::mem::take(thumb_queue), ThumbJob { slot_key: name.to_string(), name: name.to_string() });
                    }
                }
                ui.label(name);
            });
            // Own interaction on top of the card's contents, same pattern
            // as `ui::decks::deck_card`: click-to-load reads the card's
            // response rather than adding a competing Sense to each child
            // widget.
            if card.response.interact(egui::Sense::click()).clicked() {
                *load_request = Some(name.to_string());
            }

            // Laid out in its own row below the card, so its rect never
            // overlaps the card's: same reasoning as the bus-cycle button
            // in `ui::decks::deck_card`.
            ui.horizontal(|ui| {
                if ui.button("+A").clicked() {
                    show.playlists.add_to_playlist(Deck::A, name.to_string());
                }
                if ui.button("+B").clicked() {
                    show.playlists.add_to_playlist(Deck::B, name.to_string());
                }
            });
        });
    });
}
