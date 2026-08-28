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
//!
//! Both of this panel's per-frame costs are bounded against a real
//! ~9800-preset library: the search result list is cached (`SearchCache`)
//! and the grid is laid out row by row through `ScrollArea::show_rows`, so
//! off-screen tiles cost nothing at all rather than a full widget layout
//! each.

use opendrop_core::commands::Deck;
use opendrop_core::preset_index::search;
use opendrop_core::show::Show;
use opendrop_core::thumb_queue::{enqueue_front, ThumbJob};
use std::collections::{HashMap, HashSet};

/// Matches `opendrop_engine::thumbnail::{THUMB_W, THUMB_H}`'s 192:108
/// aspect ratio, scaled down for a browser tile.
const TILE_SIZE: egui::Vec2 = egui::vec2(96.0, 54.0);

/// Width of a tile's content column: `TILE_SIZE.x` widened to fit the
/// "+A"/"+B" button row underneath it. Shared with `tile_stride` so the
/// tiles-per-row count and the tiles themselves can never disagree.
const TILE_CONTENT_W: f32 = 110.0;

/// Fixed row pitch handed to `ScrollArea::show_rows`, which needs one to
/// map a scroll offset to a row index without laying anything out. Picked
/// with headroom over a tile's natural height (thumbnail + name + button
/// row + the group frame's margins) and then enforced with `set_min_height`
/// on each row, so the pitch `show_rows` assumes always matches what is
/// actually laid out.
const ROW_HEIGHT: f32 = 132.0;

/// Cached result of `search(&show.preset_catalog, query)`, resolved to
/// owned names.
///
/// `search` walks the whole catalog, and the panel then has to clone every
/// match into an owned `String`: the borrow of `show.preset_catalog` must
/// end before the tiles below take `&mut show.playlists` for their +A/+B
/// buttons. Against a ~9800-preset library that is ~9800 allocations, and
/// it used to run on *every* frame, query untouched or not. Now it only
/// re-runs when the query text actually changed since the previous frame.
///
/// `show.preset_catalog` itself is scanned once at bootstrap and never
/// mutated afterwards, so the query is the only invalidation key needed.
#[derive(Default)]
pub struct SearchCache {
    /// `None` until the first resolve, which is distinct from `Some("")`:
    /// the empty query is a real, cacheable "show everything" result.
    query: Option<String>,
    results: Vec<String>,
}

impl SearchCache {
    fn resolve(&mut self, show: &Show, query: &str) -> &[String] {
        if self.query.as_deref() != Some(query) {
            self.results = search(&show.preset_catalog, query).into_iter().map(|p| p.name.clone()).collect();
            self.query = Some(query.to_string());
        }
        &self.results
    }
}

/// Horizontal space one tile occupies, content column plus the group
/// frame's margins/stroke plus the spacing before the next tile. Read off
/// the live style rather than hardcoded, so the tiles-per-row count stays
/// right if the theme's margins change.
fn tile_stride(ui: &egui::Ui) -> f32 {
    let frame = egui::Frame::group(ui.style());
    TILE_CONTENT_W + frame.total_margin().sum().x + ui.spacing().item_spacing.x
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    show: &mut Show,
    search_query: &mut String,
    search_cache: &mut SearchCache,
    thumb_queue: &mut Vec<ThumbJob>,
    thumbnail_textures: &HashMap<String, egui::TextureHandle>,
    failed_thumbnails: &HashSet<String>,
    load_request: &mut Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label("Search");
        ui.text_edit_singleline(search_query);
    });

    ui.separator();

    let per_row = ((ui.available_width() / tile_stride(ui)).floor() as usize).max(1);
    // Borrows `search_cache`, not `show`, so the tiles below can still take
    // `show` mutably.
    let results = search_cache.resolve(show, search_query.as_str());
    let total_rows = results.len().div_ceil(per_row);

    // `show_rows`, not `vertical() + horizontal_wrapped()`: the free-flow
    // version built every one of the ~9800 tiles as real widgets every
    // frame: `is_rect_visible` only skipped the *painting*, never the
    // layout. This one never even visits a row that isn't on screen.
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, ROW_HEIGHT, total_rows, |ui, rows| {
        for row in rows {
            let start = row * per_row;
            let end = (start + per_row).min(results.len());
            ui.horizontal(|ui| {
                ui.set_min_height(ROW_HEIGHT); // keeps the real pitch equal to ROW_HEIGHT
                for name in &results[start..end] {
                    tile(ui, show, name, thumb_queue, thumbnail_textures, failed_thumbnails, load_request);
                }
            });
        }
    });
}

/// One preset tile: thumbnail (or placeholder while it's still queued),
/// name, click-to-load, +A/+B playlist buttons.
///
/// The thumbnail request is enqueued at most once per tile per frame: and
/// only while the tile is both actually on-screen (`ui.is_rect_visible`,
/// which checks against the `ScrollArea`'s clip rect) and still missing its
/// texture. Once `pump_thumbnail_queue` (Step 15) fills `thumbnail_textures`
/// for this name, the `else` branch below stops firing on its own: no
/// separate "stop requesting" signal needed.
#[allow(clippy::too_many_arguments)]
fn tile(
    ui: &mut egui::Ui,
    show: &mut Show,
    name: &str,
    thumb_queue: &mut Vec<ThumbJob>,
    thumbnail_textures: &HashMap<String, egui::TextureHandle>,
    failed_thumbnails: &HashSet<String>,
    load_request: &mut Option<String>,
) {
    ui.push_id(name, |ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(TILE_CONTENT_W);
            let card = ui.vertical(|ui| {
                let (rect, _response) = ui.allocate_exact_size(TILE_SIZE, egui::Sense::hover());
                if ui.is_rect_visible(rect) {
                    if let Some(tex) = thumbnail_textures.get(name) {
                        // Identity UV, deliberately NOT the V-flipped rect
                        // `ui::decks` needs for the live deck texture:
                        // `render_thumbnail` already reverses glReadPixels'
                        // bottom-first rows at the source, so these pixels
                        // reach egui in its own top-first order.
                        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
                        ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
                    } else if failed_thumbnails.contains(name) {
                        // Rendering this one already failed. Re-queueing it
                        // would re-run a full preset load + 31 render frames
                        // + a blocking readback every tick, for as long as
                        // the tile stays on screen.
                        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(60, 30, 30));
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
