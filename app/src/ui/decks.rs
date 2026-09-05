//! Decks panel: 4 deck cards (live GPU thumbnail, preset name/status,
//! active-slot highlight, bus-cycle badge), plus the transition-seconds
//! control. Port of `MixerLayout.svelte` (Step 16 of the plan). The
//! crossfader that used to sit here moved into the header's hand-painted
//! mini-transport (Step 10 of the Phase 7 UI redesign plan, `ui::shell::
//! header`), always visible now, not just while this panel is active.
//!
//! Reskinned per `decks-v2.html` (Step 13 of the plan): `widgets::card`
//! instead of `Frame::group`, `widgets::pill` bus badges, and a hand-painted
//! thumbnail texture/active-glow using the same layered-`rect_filled`
//! technique `ui::shell::crossfader`'s handle halo already established.
//! Step 22 wires up the card's hover lift+glow (`hover_glow`) and the
//! active card's breathing glow (`active_glow`'s continuous oscillation);
//! the live dot's pulse isn't one of that step's 3 mandated animated
//! locations (deck-card hover, preset-tile hover, nav-rail slide) and
//! stays resting/static, same as the diagonal hairline stripes.
//!
//! Takes individual `AppState` fields rather than `&mut AppState` as a
//! whole: the call site (`main.rs`'s `about_to_wait`) already holds
//! `state.egui_glow` mutably borrowed for the `run()` closure, so this
//! needs disjoint borrows of just the fields it touches.

use opendrop_core::show::{DeckBus, Show};
use std::collections::{HashMap, HashSet};

use crate::theme::easing::ease_out_kushie;
use crate::theme::fonts::FAMILY_MONO;
use crate::ui::widgets::{self, theme};
use crate::video_clips::VideoClip;

/// `Deck::texture` is filled by `glCopyTexSubImage2D` from the deck's own
/// FBO 0, so its texel row 0 is the framebuffer's *bottom* scanline: GL's
/// lower-left origin, the convention `engine::compositor` deliberately
/// keeps end to end (its vertex shader drops the source's `1.0 - vUV.y`).
/// egui's origin is top-left instead: `epaint::Mesh::add_rect_with_uv`
/// pairs `rect.left_top()` with `uv.left_top()`, and egui_glow's vertex
/// shader passes that straight into `texture()`. So the default
/// `(0,0)-(1,1)` rect would draw the live deck texture upside down; this
/// one flips V. Fixed here rather than in the GL pipeline on purpose: the
/// live output window is correct as-is, only egui's view of the texture
/// needs the compensation.
///
/// `pub(crate)` (Step 11 of the Phase 7 UI redesign plan): the Stage
/// bottom bar's own deck vignettes (`ui::shell::status_bar_stage`) draw
/// the same live textures at a smaller size and need this exact flip too
/// too, reused, not redefined, so the two never drift.
pub(crate) const FLIPPED_V_UV: egui::Rect = egui::Rect { min: egui::pos2(0.0, 1.0), max: egui::pos2(1.0, 0.0) };

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    show: &mut Show,
    deck_tex_ids: &[egui::TextureId; 4],
    deck_preset_names: &[String; 4],
    video_clips: &[VideoClip],
    deck_video_tex_ids: &[egui::TextureId; 4],
    deck_video_errors: &[Option<String>; 4],
    pending_validations: &HashSet<usize>,
    preset_errors: &HashMap<usize, String>,
    transition_seconds: &mut f64,
) {
    ui.horizontal(|ui| {
        for i in 0..4 {
            deck_card(
                ui,
                i,
                show,
                deck_tex_ids,
                deck_preset_names,
                video_clips,
                deck_video_tex_ids,
                deck_video_errors,
                pending_validations,
                preset_errors,
            );
        }
    });

    ui.separator();

    transition_row(ui, transition_seconds);
}

/// One deck card. The bus-cycle badge is laid out in its own row below the
/// thumbnail/name block, so its rect never overlaps the block's: the
/// click-to-select `Sense` added to that block (below) and the badge's own
/// click never compete for the same pointer event.
#[allow(clippy::too_many_arguments)]
fn deck_card(
    ui: &mut egui::Ui,
    i: usize,
    show: &mut Show,
    deck_tex_ids: &[egui::TextureId; 4],
    deck_preset_names: &[String; 4],
    video_clips: &[VideoClip],
    deck_video_tex_ids: &[egui::TextureId; 4],
    deck_video_errors: &[Option<String>; 4],
    pending_validations: &HashSet<usize>,
    preset_errors: &HashMap<usize, String>,
) {
    let t = theme(ui);
    let is_active = i == show.selected_slot;

    ui.push_id(i, |ui| {
        let card = widgets::card(ui, |ui| {
            // `card`'s content_ui inherits its layout from the caller,
            // here, the enclosing `ui.horizontal` deck row, so without an
            // explicit `ui.vertical` wrapper, `block` and `bus` below would
            // lay out side by side instead of stacked (also found live: the
            // bus badge rendered as an oversized vertical bar next to the
            // thumbnail, inheriting the row's full cross-axis height).
            ui.vertical(|ui| {
                let block = ui.vertical(|ui| {
                    // `ui.vertical` defaults to wrapping text to the FULL available
                    // width of the enclosing `ui.horizontal` row (egui's
                    // `TextWrapMode::Wrap` for vertical layouts), not to the
                    // thumbnail's intrinsic width. Without this, the preset-name
                    // label below stretches this card to consume the entire row,
                    // hiding decks 1-3 (found live, post-Phase-7-redesign).
                    ui.set_width(t.metrics.thumb_size.x);
                    let content_tex_id = if show.deck_video[i].enabled { deck_video_tex_ids[i] } else { deck_tex_ids[i] };
                    let image = ui.add(egui::Image::new((content_tex_id, t.metrics.thumb_size)).uv(FLIPPED_V_UV));
                    thumbnail_overlay(ui, image.rect, is_active);

                    if show.deck_video[i].enabled {
                        if let Some(err) = deck_video_errors[i].as_deref() {
                            widgets::error_banner(ui, err);
                        } else if video_clips.is_empty() {
                            ui.label("(no clip)");
                        } else {
                            let clip = &video_clips[show.deck_video[i].current_clip_index % video_clips.len()];
                            ui.label(&clip.name);
                        }
                    } else if pending_validations.contains(&i) {
                        ui.label("Validating…");
                    } else if let Some(err) = preset_errors.get(&i) {
                        widgets::error_banner(ui, err);
                    } else {
                        ui.label(&deck_preset_names[i]);
                    }
                    meta_line(ui);
                });
                if block.response.interact(egui::Sense::click()).clicked() {
                    show.select_slot(i);
                }

                // Bus A = `accent`, bus B = `ok`, off = `dim` (the mapping
                // `Metrics`' own doc comment establishes for this step, already
                // reused as-is by the header's own bus A/B pills, `ui::shell::
                // status_bar_stage`).
                let (bus_text, bus_color) = match show.deck_bus[i] {
                    DeckBus::A => ("● Bus A", t.palette.accent),
                    DeckBus::B => ("● Bus B", t.palette.ok),
                    DeckBus::Off => ("○ Off", t.palette.dim),
                };
                let bus = widgets::pill(ui, bus_text, bus_color);
                if bus.interact(egui::Sense::click()).clicked() {
                    show.deck_bus[i] = show.deck_bus[i].next();
                }
            });
        });

        // Hover lift + glow (Step 22 of the Phase 7 UI redesign plan): `i`
        // (the `push_id` above) is already a stable animation key: 4
        // fixed decks, never filtered or reordered, so no id change is
        // needed for this step's id-hygiene rule. Hit-test
        // (`card.response.hovered()`) always reads the card's real,
        // un-lifted `Response::rect` from the layout above: only
        // `hover_glow`'s decorative overlay rect is ever translated, so
        // the animated lift can never desync from the cursor mid-
        // transition, the "card dodges the cursor" bug this step's brief
        // explicitly warns about.
        let d = t.durations.fast.max(4.0 * ui.ctx().input(|input| input.stable_dt));
        let hover_t = ui.ctx().animate_bool_with_time_and_easing(ui.id().with("hover"), card.response.hovered(), d, ease_out_kushie);
        if hover_t > 0.0 {
            hover_glow(ui, card.response.rect, hover_t);
        }

        if is_active {
            active_glow(ui, card.response.rect);
        }
    });
}

/// Hand-painted texture over the live thumbnail (mockup: `.od-deck-thumb
/// ::after`'s repeating diagonal hairlines, `.od-livedot`): thin
/// `accent`-tinted diagonal lines, more opaque on the active deck (mirrors
/// the mockup's `.on` variant), clipped to `rect` so they never bleed onto
/// neighboring cards. The live dot only paints on the active deck: the
/// mockup's own markup only puts `.od-livedot` on its `.on` card (the other
/// 3, including one also assigned to a bus, have none), so this reads it as
/// bundled with the active/selected state rather than with bus routing.
/// Both elements stay resting-state only: the live dot's pulse isn't one
/// of Step 22's 3 mandated animated locations (see this file's module doc
/// comment), and the diagonal stripes have no animated counterpart in the
/// mockup either.
fn thumbnail_overlay(ui: &egui::Ui, rect: egui::Rect, is_active: bool) {
    let t = theme(ui);
    let painter = ui.painter().with_clip_rect(rect);

    let stripe = t.palette.accent.gamma_multiply(if is_active { 0.20 } else { 0.06 });
    let step = 6.0;
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment([egui::pos2(x, rect.bottom()), egui::pos2(x + rect.height(), rect.top())], egui::Stroke::new(1.0, stripe));
        x += step;
    }

    if is_active {
        let center = rect.left_top() + egui::vec2(10.0, 10.0);
        for (radius, alpha) in [(6.0, 40u8), (4.0, 90u8)] {
            painter.circle_filled(center, radius, t.palette.ok.gamma_multiply_u8(alpha));
        }
        painter.circle_filled(center, 3.0, t.palette.ok);
    }
}

/// Static mesh/fps meta row under the deck name (mockup: `.od-deck-meta`,
/// mono/micro/muted, justified left-right). No per-deck mesh-quality or fps
/// readback exists anywhere yet: `opendrop_engine::deck`'s `set_mesh_size`
/// is write-only (no getter), and the app's one fps figure (`ShellCtx::
/// last_wall_ms`) is a single global number, not per-deck. Rather than
/// fabricate numbers a live VJ performer could mistake for real telemetry,
/// this renders the row's layout/typography only, both sides a placeholder
/// dash, the same "no data" convention the mockup itself uses for an empty
/// slot's meta row. Wiring real per-deck figures is follow-up work, not
/// this step's (a static visual reskin).
fn meta_line(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        widgets::micro_label(ui, "—");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::micro_label(ui, "—");
        });
    });
}

/// Accent border + soft glow on the active deck's card, breathing (Step 22
/// of the Phase 7 UI redesign plan) between the mockup's `breathe`
/// keyframe's 0%/100% state (this function's original alphas below,
/// unchanged, are exactly that resting state: `0 0 0 1px accent, 0 4px
/// 16px accent@18%`) and its 50% peak: a continuous ~2.4s sine oscillation
/// of the halo layers' alpha, driven directly by `ui.ctx().input(|i|
/// i.time)`, deliberately NOT an `animate_*` call, since there is no
/// bool/value state transition here, just the elapsed clock. Never the
/// rect itself, only intensity oscillates. `focus_ring`'s crisp 1px ring
/// (the mockup's separate `0 0 0 1px accent` shadow layer) stays constant,
/// matching the mockup's own layering.
fn active_glow(ui: &egui::Ui, rect: egui::Rect) {
    let t = theme(ui);
    let corner_radius = egui::CornerRadius::from(t.metrics.radius_lg);

    // `sin(pi * phase)` is 0 at `phase` 0.0 and 1.0 (the keyframe's 0%/100%
    // resting endpoints) and 1.0 at `phase` 0.5 (the keyframe's 50% peak),
    // never negative across `phase`'s [0, 1) range, the "breathe out,
    // breathe in" shape of a CSS `breathe` keyframe.
    const PERIOD_SECS: f64 = 2.4;
    let phase = (ui.ctx().input(|i| i.time) / PERIOD_SECS).rem_euclid(1.0);
    let breathe = (std::f64::consts::PI * phase).sin() as f32;
    let intensity = 1.0 + 0.6 * breathe;

    for (expand, alpha) in [(14.0, 10u8), (8.0, 22u8), (3.0, 40u8)] {
        let alpha = ((alpha as f32) * intensity).min(255.0) as u8;
        ui.painter().rect_filled(rect.expand(expand), corner_radius, t.palette.accent.gamma_multiply_u8(alpha));
    }
    widgets::focus_ring(ui, rect);
}

/// Hover lift + glow overlay (Step 22): a stronger, upward-shifted version
/// of `active_glow`'s resting halo, faded in by `hover_t` (the id-keyed
/// `animate_bool_with_time_and_easing` progress computed by `deck_card`,
/// 0 = resting, 1 = fully hovered). Painted from the card's own
/// `Response::rect`, already fully laid out and hit-tested before this
/// runs (see `deck_card`'s own comment on this call site): only this
/// decorative overlay's rect is ever translated upward, which is what
/// visually "lifts" the card without moving anything the pointer is
/// actually tested against.
fn hover_glow(ui: &egui::Ui, rect: egui::Rect, hover_t: f32) {
    let t = theme(ui);
    let corner_radius = egui::CornerRadius::from(t.metrics.radius_lg);
    let lifted = rect.translate(egui::vec2(0.0, -4.0 * hover_t));
    for (expand, alpha) in [(16.0, 14u8), (9.0, 28u8)] {
        let alpha = (alpha as f32 * hover_t) as u8;
        ui.painter().rect_filled(lifted.expand(expand), corner_radius, t.palette.accent.gamma_multiply_u8(alpha));
    }
}

/// Fade/transition summary row: same ticks + rail visual grammar as the
/// header's crossfader (`ui::shell::crossfader`, Step 10): a `dim`-ticked,
/// `ink`-filled rail with an accent-tinted fill and border. Not the same
/// widget: this reads a duration (`0.0..=5.0` seconds), not an A/B mix
/// fraction, and its fill is one flat tint rather than the crossfader's own
/// 2-triangle gradient mesh, simpler, because this is a summary row, not
/// the header's always-visible live mini-transport. Still a real drag/click
/// control (dragging or clicking the rail sets `transition_seconds`
/// directly): "static" in this step's brief means no *animation* (Step 22's
/// job), not no interactivity; the plain `egui::Slider` it replaces was
/// draggable too.
fn transition_row(ui: &mut egui::Ui, transition_seconds: &mut f64) {
    let t = theme(ui);

    ui.horizontal(|ui| {
        widgets::micro_label(ui, "Fade");

        let rail_size = egui::vec2(220.0, 18.0);
        let rail_height = 8.0;
        let (rect, response) = ui.allocate_exact_size(rail_size, egui::Sense::click_and_drag());

        if (response.dragged() || response.clicked()) && rect.width() > 0.0 {
            if let Some(pos) = response.interact_pointer_pos() {
                let frac = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                *transition_seconds = frac as f64 * 5.0;
            }
        }

        if ui.is_rect_visible(rect) {
            let rail_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), rail_height));
            let corner_radius = egui::CornerRadius::from(t.metrics.radius_sm);

            ui.painter().rect_filled(rail_rect, corner_radius, t.palette.ink);

            const TICKS: usize = 8;
            for i in 0..TICKS {
                let x = rail_rect.left() + rail_rect.width() * (i as f32 / (TICKS - 1) as f32);
                ui.painter().line_segment(
                    [egui::pos2(x, rail_rect.top() + 1.0), egui::pos2(x, rail_rect.bottom() - 1.0)],
                    egui::Stroke::new(1.0, t.palette.dim),
                );
            }

            let frac = (*transition_seconds / 5.0).clamp(0.0, 1.0) as f32;
            let mut fill_rect = rail_rect;
            fill_rect.set_width(rail_rect.width() * frac);
            ui.painter().rect_filled(fill_rect, corner_radius, t.palette.accent.gamma_multiply(0.16));
            ui.painter()
                .line_segment([fill_rect.right_top(), fill_rect.right_bottom()], egui::Stroke::new(t.metrics.border_width, t.palette.accent));

            ui.painter().rect_stroke(
                rail_rect,
                corner_radius,
                egui::Stroke::new(t.metrics.border_width, t.palette.border),
                egui::StrokeKind::Outside,
            );
        }

        ui.label(
            egui::RichText::new(format!("{:.1}s", *transition_seconds))
                .font(egui::FontId::new(t.type_scale.numeric, egui::FontFamily::Name(FAMILY_MONO.into())))
                .color(t.palette.accent),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::ghost_button(ui, "Hard Cut").clicked() {
                *transition_seconds = 0.0;
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    // `state`, never `show` (that identifier is this module's own `show`
    // function, imported into scope by `use super::*` above; a same-named
    // local binding would shadow it and break every bare `show(...)` call
    // below).
    fn sample_state() -> Show {
        Show::default()
    }

    fn sample_tex_ids() -> [egui::TextureId; 4] {
        [egui::TextureId::default(); 4]
    }

    // --- show(): the whole panel, airy (default) and dense -----------------

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            let mut state = sample_state();
            let tex_ids = sample_tex_ids();
            let names: [String; 4] = Default::default();
            let video_errors: [Option<String>; 4] = Default::default();
            let pending = HashSet::new();
            let errors = HashMap::new();
            let mut transition_seconds = 1.2;
            show(ui, &mut state, &tex_ids, &names, &[], &tex_ids, &video_errors, &pending, &errors, &mut transition_seconds);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let mut state = sample_state();
                let tex_ids = sample_tex_ids();
                let names: [String; 4] = Default::default();
                let video_errors: [Option<String>; 4] = Default::default();
                let pending = HashSet::new();
                let errors = HashMap::new();
                let mut transition_seconds = 0.0;
                show(ui, &mut state, &tex_ids, &names, &[], &tex_ids, &video_errors, &pending, &errors, &mut transition_seconds);
            });
        });
    }

    // --- show(): every per-deck branch (validating / error / bus states) --

    #[test]
    fn show_renders_validating_and_error_decks() {
        themed_test_ui(|ui| {
            let mut state = sample_state();
            state.deck_bus = [DeckBus::A, DeckBus::B, DeckBus::Off, DeckBus::Off];
            let tex_ids = sample_tex_ids();
            let names: [String; 4] = Default::default();
            let video_errors: [Option<String>; 4] = Default::default();
            let mut pending = HashSet::new();
            pending.insert(1);
            let mut errors = HashMap::new();
            errors.insert(2, "mesh load failed".to_string());
            let mut transition_seconds = 5.0;
            show(ui, &mut state, &tex_ids, &names, &[], &tex_ids, &video_errors, &pending, &errors, &mut transition_seconds);
        });
    }

    #[test]
    fn show_renders_named_preset() {
        themed_test_ui(|ui| {
            let mut state = sample_state();
            let tex_ids = sample_tex_ids();
            let names = ["Neon Tunnel Refract".to_string(), String::new(), String::new(), String::new()];
            let video_errors: [Option<String>; 4] = Default::default();
            let pending = HashSet::new();
            let errors = HashMap::new();
            let mut transition_seconds = 2.5;
            show(ui, &mut state, &tex_ids, &names, &[], &tex_ids, &video_errors, &pending, &errors, &mut transition_seconds);
        });
    }

    #[test]
    fn show_renders_a_deck_in_video_mode() {
        themed_test_ui(|ui| {
            let mut state = sample_state();
            state.deck_video[0].enabled = true;
            let tex_ids = sample_tex_ids();
            let names: [String; 4] = Default::default();
            let video_errors: [Option<String>; 4] = Default::default();
            let clips = vec![VideoClip {
                key: "/clips/a.webm".into(),
                name: "a".into(),
                path: "/clips/a.webm".into(),
                builtin: false,
            }];
            let pending = HashSet::new();
            let errors = HashMap::new();
            let mut transition_seconds = 1.0;
            show(ui, &mut state, &tex_ids, &names, &clips, &tex_ids, &video_errors, &pending, &errors, &mut transition_seconds);
        });
    }

    // --- deck_card(): selected slot drives the active-glow branch ---------

    #[test]
    fn deck_card_active_and_inactive_do_not_panic() {
        themed_test_ui(|ui| {
            let mut state = sample_state();
            state.selected_slot = 0;
            let tex_ids = sample_tex_ids();
            let names: [String; 4] = Default::default();
            let video_errors: [Option<String>; 4] = Default::default();
            let pending = HashSet::new();
            let errors = HashMap::new();
            deck_card(ui, 0, &mut state, &tex_ids, &names, &[], &tex_ids, &video_errors, &pending, &errors);
            deck_card(ui, 1, &mut state, &tex_ids, &names, &[], &tex_ids, &video_errors, &pending, &errors);
        });
    }

    // --- transition_row(): drives Hard Cut and stays in range -------------

    #[test]
    fn transition_row_does_not_panic() {
        themed_test_ui(|ui| {
            let mut transition_seconds = 3.4;
            transition_row(ui, &mut transition_seconds);
            widgets::dense(ui, |ui| {
                transition_row(ui, &mut transition_seconds);
            });
        });
    }

    // --- thumbnail_overlay()/active_glow()/meta_line(): painter-only calls,
    // both active and inactive ----------------------------------------------

    #[test]
    fn thumbnail_overlay_does_not_panic() {
        themed_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(160.0, 90.0));
            thumbnail_overlay(ui, rect, false);
            thumbnail_overlay(ui, rect, true);
        });
    }

    #[test]
    fn active_glow_does_not_panic() {
        themed_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(160.0, 90.0));
            active_glow(ui, rect);
        });
    }

    #[test]
    fn meta_line_does_not_panic() {
        themed_test_ui(|ui| {
            meta_line(ui);
        });
    }
}
