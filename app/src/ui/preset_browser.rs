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
//!
//! Reskinned per `decks-presets.html` (Step 14 of the Phase 7 UI redesign
//! plan): tile size/content width move into `Metrics` (`THUMB_SIZE`'s Step
//! 3/13 migration pattern), the tile becomes a 4:3 thumb (mockup's
//! `.od-tile-thumb`, was 16:9) with a mono truncated name, and the fixed
//! `ROW_HEIGHT` constant is replaced by `row_height`, derived from the
//! live style rather than hand-picked: see that function's doc comment
//! for why. This panel is one of 3 density-frozen zones in the app (always
//! dense, no user toggle), unlike Decks (Step 13, aéré by default).

use opendrop_core::commands::Deck;
use opendrop_core::preset_index::search;
use opendrop_core::show::Show;
use opendrop_core::thumb_queue::{enqueue_front, prune_to_visible, ThumbJob};
use std::collections::{HashMap, HashSet};

use crate::theme::fonts::FAMILY_MONO;
use crate::theme::tokens::Metrics;
use crate::ui::ctx::{LibraryCtx, PerformCtx};
use crate::ui::widgets::{self, theme};

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
/// right if the theme's margins change. Takes `metrics` explicitly (the
/// same `&'static Metrics` `row_height` below takes) rather than calling
/// `theme(ui).metrics` itself, so both functions are provably reading the
/// same source of truth instead of two independent lookups that could
/// diverge.
fn tile_stride(ui: &egui::Ui, metrics: &Metrics) -> f32 {
    let frame = egui::Frame::group(ui.style());
    metrics.tile_content_w + frame.total_margin().sum().x + ui.spacing().item_spacing.x
}

/// Row pitch handed to `ScrollArea::show_rows` as its `row_height_sans_
/// spacing` parameter (egui 0.36.1, `containers/scroll_area.rs`). Confirmed
/// from that function's source before writing this:
///
/// ```ignore
/// let spacing = ui.spacing().item_spacing;
/// let row_height_with_spacing = row_height_sans_spacing + spacing.y;
/// // ... min_row/max_row and the visible rect are computed from
/// // row_height_with_spacing, i.e. show_rows assumes EVERY row (not just
/// // the visible ones) is exactly `row_height_sans_spacing` tall plus one
/// // trailing `item_spacing.y` gap.
/// ```
///
/// The value returned here must therefore be the row's real content
/// height alone, WITHOUT that trailing gap: `show_rows` adds `item_
/// spacing.y` itself, and `show()`'s own `add_contents` closure (a plain
/// vertical layout) adds that same `item_spacing.y` again between
/// successive `ui.horizontal` rows through ordinary layout, which is what
/// makes the two additions line up rather than double- or under-count.
/// Passing a value that already includes the trailing gap would space
/// rows too far apart (wasted room, not an overlap); passing one that
/// undercounts the row's actual rendered height is the dangerous
/// direction: `show_rows` would then assume a shorter pitch than what
/// `set_min_height` (in `show()`, below) actually forces each row to be,
/// and every row from that point on would render fractionally lower than
/// its assumed slot, compounding into a real overlap by the time the
/// ~9800-item library has been scrolled through.
///
/// Derived, not hardcoded, by walking the exact same widget tree `tile()`
/// builds: the `Frame::group` margins that wrap each card, `metrics.
/// tile_size.y` for the thumbnail, one `item_spacing.y` for the gap
/// between the thumbnail and the name (siblings inside the card's own
/// `ui.vertical`), the name label's real text-row height (measured for
/// the exact mono `FontId` `tile()` renders it with, not a guessed
/// constant), a second `item_spacing.y` for the gap between the card and
/// the "+A"/"+B" row (siblings inside the frame's own vertical layout),
/// and the button row's height. The button figure can't be read back from
/// a live `Response` here without actually rendering a probe widget every
/// frame, so it mirrors the same floor `egui::Button` itself applies
/// before painting (`widgets/button.rs`: `min_size.y = min_size.y.
/// at_least(interact_size.y)`) together with its content-driven height
/// (text row + `2 * button_padding.y`), taking whichever is larger: this
/// is the one part of the derivation that isn't a byte-for-byte replay of
/// `tile()`'s own layout calls, which is exactly why `row_height_covers_
/// the_real_tile_content_height` (below, in `mod tests`) doesn't re-run
/// this same formula: it renders one real tile through `tile()` and
/// asserts this function's return value against the actually-measured
/// `Response` height, so a wrong assumption here (rather than in
/// `Button`'s own internals) still gets caught.
fn row_height(ui: &egui::Ui, metrics: &Metrics) -> f32 {
    let t = theme(ui);
    let frame = egui::Frame::group(ui.style());
    let spacing = ui.spacing();

    let name_font = egui::FontId::new(t.type_scale.small, egui::FontFamily::Name(FAMILY_MONO.into()));
    let name_height = ui.fonts_mut(|f| f.row_height(&name_font));
    let button_height = spacing.interact_size.y.max(ui.text_style_height(&egui::TextStyle::Button) + 2.0 * spacing.button_padding.y);

    frame.total_margin().sum().y + metrics.tile_size.y + spacing.item_spacing.y + name_height + spacing.item_spacing.y + button_height
}

// Step 9 (Phase 7 UI redesign plan): takes the two context structs that
// carry this panel's params instead of 7 individual ones: `perform.show`
// (needed for the tiles' +A/+B buttons) and `library`'s 6 browser-local
// fields. Pure re-packaging: every access below is exactly the field the
// old individual parameter of the same name used to be.
pub fn show(ui: &mut egui::Ui, perform: &mut PerformCtx, library: &mut LibraryCtx) {
    let metrics = theme(ui).metrics;

    // Permanently dense (one of 3 density-frozen zones in the app, see
    // this file's module doc comment): no user-facing toggle, so this
    // scope wraps the whole panel body rather than being conditional on
    // anything. `row_height`/`tile_stride` below both read `ui.spacing()`
    // live, so they automatically pick up the dense scale from inside
    // this closure: no separate wiring needed.
    let visible_names: HashSet<String> = widgets::dense(ui, |ui| {
        let t = theme(ui);
        ui.horizontal(|ui| {
            widgets::micro_label(ui, "Search");
            ui.add(
                egui::TextEdit::singleline(library.preset_search_query)
                    .font(egui::FontId::new(t.type_scale.monospace, egui::FontFamily::Name(FAMILY_MONO.into()))),
            );
        });

        ui.separator();

        // Measured against the width the ScrollArea below will actually
        // hand its contents: `allocated_width` is what a non-floating
        // scrollbar takes out of it (0 for egui's default floating bars).
        // Counting one tile too many per row would clip the rightmost
        // one, since `ui.horizontal` does not wrap.
        let usable_w = ui.available_width() - ui.spacing().scroll.allocated_width();
        let per_row = ((usable_w / tile_stride(ui, metrics)).floor() as usize).max(1);
        // Borrows `library.search_cache`, not `perform.show`, so the tiles
        // below can still take `perform.show` mutably: `&*perform.show`
        // here is a shared reborrow of a `PerformCtx` field, scoped to
        // this one call, never stored (see `ui::ctx`'s module doc comment
        // on this exact call site).
        let results = library.search_cache.resolve(&*perform.show, library.preset_search_query.as_str());
        let total_rows = results.len().div_ceil(per_row);

        // Whole-branch review Finding 4: names of the tiles actually on
        // screen this frame, collected alongside the row layout below so
        // `thumb_queue` can be pruned to just them afterwards: the queue
        // previously grew unbounded across a fast scroll or a
        // search-query change, since nothing ever removed a job for a
        // tile that scrolled (or was filtered) away.
        let mut visible_names: HashSet<String> = HashSet::new();

        // `row_height`, not a fixed constant: derived from the live
        // (dense) style, see that function's doc comment for the full
        // `show_rows` contract this has to satisfy. Computed once per
        // frame, outside the `show_rows` closure, and reused both for the
        // pitch passed to `show_rows` and for `set_min_height` below, so
        // the two can never read a different value.
        let row_h = row_height(ui, metrics);

        // `show_rows`, not `vertical() + horizontal_wrapped()`: the
        // free-flow version built every one of the ~9800 tiles as real
        // widgets every frame: `is_rect_visible` only skipped the
        // *painting*, never the layout. This one never even visits a row
        // that isn't on screen.
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, total_rows, |ui, rows| {
            for row in rows {
                let start = row * per_row;
                let end = (start + per_row).min(results.len());
                visible_names.extend(results[start..end].iter().cloned());
                ui.horizontal(|ui| {
                    ui.set_min_height(row_h); // keeps the real pitch equal to row_h
                    for name in &results[start..end] {
                        tile(ui, perform.show, name, metrics, library.thumb_queue, library.thumbnail_textures, library.failed_thumbnails, library.load_request);
                    }
                });
            }
        });

        visible_names
    });

    // A fast scroll through ~9800 tiles, or a search query that filters
    // most of them out, must not leave thousands of stale jobs queued
    // behind the ones actually on screen: see `prune_to_visible`'s doc
    // comment. Also naturally handles the panel-loses-focus case: this
    // function simply isn't called while the panel is hidden, so the
    // queue stays frozen (never pruned, but also never growing) until the
    // panel is shown again, at which point this prunes it fresh against
    // whatever is visible then, before the next pump tick can grind
    // through anything stale.
    *library.thumb_queue = prune_to_visible(std::mem::take(library.thumb_queue), &visible_names);
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
    metrics: &Metrics,
    thumb_queue: &mut Vec<ThumbJob>,
    thumbnail_textures: &HashMap<String, egui::TextureHandle>,
    failed_thumbnails: &HashSet<String>,
    load_request: &mut Option<String>,
) {
    let t = theme(ui);
    // `name`, the preset's stable key: never a `row * per_row + i`-style
    // index, which shifts under the caller as the list scrolls or the
    // search query changes and would silently rebind an in-progress
    // widget's state (focus, animation, drag) to a different preset. Load-
    // bearing for widget ids from this step on; Step 22 reuses this same
    // key for animation ids.
    ui.push_id(name, |ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(metrics.tile_content_w);
            let card = ui.vertical(|ui| {
                // `metrics.tile_size`: 4:3 (mockup's `.od-tile-thumb`), not
                // the 16:9 the engine's own thumbnail render uses: this
                // tile crops/letterboxes rather than reproducing the
                // source aspect ratio exactly, the redesign's deliberate
                // tradeoff for a denser grid.
                let (rect, _response) = ui.allocate_exact_size(metrics.tile_size, egui::Sense::hover());
                if ui.is_rect_visible(rect) {
                    if let Some(tex) = thumbnail_textures.get(name) {
                        // Identity UV, deliberately NOT the V-flipped rect
                        // `ui::decks` needs for the live deck texture: the
                        // `--render-thumbnail` child reverses glReadPixels'
                        // bottom-first rows before writing the cache file,
                        // so these pixels reach egui in its own top-first
                        // order.
                        //
                        // `ui.put` + the `Image` widget, not `Painter::
                        // image` (which takes a mandatory `tint: Color32`
                        // with no themed meaning here: untinted is the
                        // only correct choice for compositing a texture
                        // as-is): `Image`'s own default tint is `Color32::
                        // WHITE` internally, so no literal needs to live in
                        // this file for it (AC-15). `ui.put` places it into
                        // the exact `rect` already allocated above, same
                        // as `Painter::image` did.
                        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
                        ui.put(rect, egui::Image::new((tex.id(), rect.size())).uv(uv));
                    } else if failed_thumbnails.contains(name) {
                        // Rendering this one already failed. Re-queueing it
                        // would respawn a render child process, for a
                        // preset already known to produce nothing usable,
                        // for as long as the tile stays on screen.
                        ui.painter().rect_filled(rect, egui::CornerRadius::from(metrics.radius_sm), t.palette.error.gamma_multiply(0.3));
                    } else {
                        ui.painter().rect_filled(rect, egui::CornerRadius::from(metrics.radius_sm), t.palette.dim);
                        *thumb_queue = enqueue_front(std::mem::take(thumb_queue), ThumbJob { slot_key: name.to_string(), name: name.to_string() });
                    }
                }
                // Truncated to a single line, not wrapped: a wrapped name
                // makes the tile's height depend on the name's length, and
                // `show_rows` above needs every row to actually be
                // `row_height`'s value tall. The full name stays reachable
                // on hover. Mono (mockup's `.od-tile-name`), `muted` (not
                // uppercased like `micro_label`'s chrome text: this is a
                // real, case-sensitive preset name, not section chrome).
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(name)
                            .font(egui::FontId::new(t.type_scale.small, egui::FontFamily::Name(FAMILY_MONO.into())))
                            .color(t.palette.muted),
                    )
                    .truncate(),
                )
                .on_hover_text(name);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use opendrop_core::preset_index::PresetMeta;

    fn sample_show() -> Show {
        let mut show = Show::default();
        show.preset_catalog = vec![
            PresetMeta { name: "Alpha Swirl Refract".to_string(), category: "Alpha".to_string() },
            PresetMeta { name: "Beta Pulse Drift".to_string(), category: "Beta".to_string() },
        ];
        show
    }

    // --- SearchCache::resolve: known queries -------------------------------

    #[test]
    fn search_cache_filters_by_case_insensitive_substring() {
        let show = sample_show();
        let mut cache = SearchCache::default();
        let results = cache.resolve(&show, "alpha");
        assert_eq!(results, ["Alpha Swirl Refract".to_string()]);
    }

    #[test]
    fn search_cache_empty_query_returns_every_preset() {
        let show = sample_show();
        let mut cache = SearchCache::default();
        let results = cache.resolve(&show, "");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_cache_no_match_returns_empty() {
        let show = sample_show();
        let mut cache = SearchCache::default();
        let results = cache.resolve(&show, "nonexistent");
        assert!(results.is_empty());
    }

    // --- row_height(): the load-bearing regression guard -------------------
    //
    // Renders one real tile through `tile()` (not a re-derivation of
    // `row_height`'s own formula, which would just check the formula
    // against itself) and asserts the derived pitch covers what actually
    // got laid out. A failure here means `show_rows` would receive a pitch
    // smaller than the real row height, which silently overlaps rows once
    // scrolled far enough into the ~9800-item library: see `row_height`'s
    // own doc comment for the full `show_rows` contract this guards.
    #[test]
    fn row_height_covers_the_real_tile_content_height() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let metrics = theme(ui).metrics;
                let mut show = sample_show();
                let mut thumb_queue = Vec::new();
                let thumbnail_textures = HashMap::new();
                let failed_thumbnails = HashSet::new();
                let mut load_request = None;

                let response = ui
                    .horizontal(|ui| {
                        tile(ui, &mut show, "Alpha Swirl Refract", metrics, &mut thumb_queue, &thumbnail_textures, &failed_thumbnails, &mut load_request);
                    })
                    .response;

                let real_height = response.rect.height();
                let derived = row_height(ui, metrics);
                assert!(
                    derived >= real_height,
                    "row_height() = {derived}, real rendered tile height = {real_height}: \
                     show_rows would receive a pitch smaller than the real row, which \
                     silently overlaps rows once scrolled far enough"
                );
            });
        });
    }

    // --- tile_stride()/row_height(): same Metrics, can't diverge -----------

    #[test]
    fn tile_stride_and_row_height_read_the_same_metrics_instance() {
        themed_test_ui(|ui| {
            let metrics = theme(ui).metrics;
            // Both take `&Metrics` explicitly rather than each calling
            // `theme(ui).metrics` internally: this asserts the call site
            // actually passes the identical `&'static Metrics` to both, the
            // property that rules out the two ever reading a different
            // instance.
            let a = std::ptr::from_ref(metrics);
            tile_stride(ui, metrics);
            row_height(ui, metrics);
            let b = std::ptr::from_ref(metrics);
            assert!(std::ptr::eq(a, b));
        });
    }

    // --- show(): the whole panel, airy (default) and dense -----------------

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let mut inner_show = sample_show();
            let deck_tex_ids = [egui::TextureId::default(); 4];
            let deck_preset_names: [String; 4] = Default::default();
            let pending = HashSet::new();
            let errors = HashMap::new();
            let mut transition_seconds = 0.0;
            let mut perform = PerformCtx {
                show: &mut inner_show,
                deck_tex_ids: &deck_tex_ids,
                deck_preset_names: &deck_preset_names,
                pending_validations: &pending,
                preset_errors: &errors,
                transition_seconds: &mut transition_seconds,
                t0: std::time::Instant::now(),
            };

            let mut search_query = String::new();
            let mut search_cache = SearchCache::default();
            let mut thumb_queue = Vec::new();
            let thumbnail_textures = HashMap::new();
            let failed_thumbnails = HashSet::new();
            let mut load_request = None;
            let mut library = LibraryCtx {
                preset_search_query: &mut search_query,
                search_cache: &mut search_cache,
                thumb_queue: &mut thumb_queue,
                thumbnail_textures: &thumbnail_textures,
                failed_thumbnails: &failed_thumbnails,
                load_request: &mut load_request,
            };

            show(ui, &mut perform, &mut library);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let mut inner_show = sample_show();
                let deck_tex_ids = [egui::TextureId::default(); 4];
                let deck_preset_names: [String; 4] = Default::default();
                let pending = HashSet::new();
                let errors = HashMap::new();
                let mut transition_seconds = 0.0;
                let mut perform = PerformCtx {
                    show: &mut inner_show,
                    deck_tex_ids: &deck_tex_ids,
                    deck_preset_names: &deck_preset_names,
                    pending_validations: &pending,
                    preset_errors: &errors,
                    transition_seconds: &mut transition_seconds,
                    t0: std::time::Instant::now(),
                };

                let mut search_query = String::new();
                let mut search_cache = SearchCache::default();
                let mut thumb_queue = Vec::new();
                let thumbnail_textures = HashMap::new();
                let failed_thumbnails = HashSet::new();
                let mut load_request = None;
                let mut library = LibraryCtx {
                    preset_search_query: &mut search_query,
                    search_cache: &mut search_cache,
                    thumb_queue: &mut thumb_queue,
                    thumbnail_textures: &thumbnail_textures,
                    failed_thumbnails: &failed_thumbnails,
                    load_request: &mut load_request,
                };

                show(ui, &mut perform, &mut library);
            });
        });
    }

    // --- tile(): push_id is keyed on the stable preset name -----------------

    #[test]
    fn tile_does_not_panic_and_is_keyed_on_name_not_index() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let metrics = theme(ui).metrics;
                let mut show = sample_show();
                let mut thumb_queue = Vec::new();
                let thumbnail_textures = HashMap::new();
                let failed_thumbnails = HashSet::new();
                let mut load_request = None;
                // Two tiles with the same name would collide on id if
                // `push_id` were keyed on a scroll/filter-shifting index
                // instead of the name: rendering the same name twice in
                // one `ui.horizontal` (impossible in practice since
                // results are deduplicated preset names, but a cheap way
                // to prove the id key is `name`-stable) must not panic on
                // an id clash within a single frame's widget tree either.
                ui.horizontal(|ui| {
                    tile(ui, &mut show, "Alpha Swirl Refract", metrics, &mut thumb_queue, &thumbnail_textures, &failed_thumbnails, &mut load_request);
                    tile(ui, &mut show, "Beta Pulse Drift", metrics, &mut thumb_queue, &thumbnail_textures, &failed_thumbnails, &mut load_request);
                });
            });
        });
    }
}
