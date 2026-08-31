//! Shared widget library (Step 8 of the Phase 7 UI redesign plan): a set of
//! standalone helper functions built on the pure tokens (Step 3) and the
//! `Style`/`Visuals` translation (Step 4), already wired onto the live
//! `Context` at bootstrap (Step 6). No panel calls any of these yet: that
//! substitution happens panel by panel at Steps 13-21, at which point each
//! panel is retouched for the theme anyway. No animation lives here either
//! (Step 22 owns easing/transitions); `card`'s and `ghost_button`'s hover
//! response is a plain binary state switch, not a tween.
//!
//! Every color routed through a helper here comes from `theme(ui).palette`
//! (or a blend derived from two palette colors via `Color32::lerp_to_gamma`/
//! `gamma_multiply`, never a hand-picked literal) and every spacing/radius
//! from `theme(ui).metrics`, so a runtime theme switch (Step 12) repaints
//! every widget built from this file correctly with no separate wiring.

use crate::theme::fonts::FAMILY_MONO;
use crate::theme::registry::{self, ThemeId};
use crate::theme::tokens::Theme;
use crate::theme::THEME_ID_KEY;

/// Resolve the active theme's `&'static Theme` from the live `Context`.
///
/// Reads the `ThemeId` written into `ctx.data` at bootstrap (`main.rs`,
/// Step 6) under `egui::Id::new(THEME_ID_KEY)`, falling back to
/// `ThemeId::default()` (Kushie) when absent, which is always the case
/// under `egui::__run_test_ui` (`egui/src/lib.rs:683`), whose fresh
/// `Context::default()` never runs that bootstrap wiring. This is what
/// lets every helper below be tested with zero setup.
pub fn theme(ui: &egui::Ui) -> &'static Theme {
    let id = ui
        .ctx()
        .data(|d| d.get_temp::<ThemeId>(egui::Id::new(THEME_ID_KEY)))
        .unwrap_or_default();
    registry::get(id)
}

/// Density scope: for the duration of `add_contents`, `Ui::item_spacing`
/// switches from `Metrics::spacing_airy` to `Metrics::spacing_dense`. The
/// only aéré→dense switch mechanism in the app; no user-facing toggle
/// exists anywhere else.
pub fn dense<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = theme(ui);
    ui.scope(|ui| {
        ui.style_mut().spacing.item_spacing = t.metrics.spacing_dense;
        add_contents(ui)
    })
    .inner
}

/// Small mono-font, uppercase, muted-color label. For terse chrome text
/// (unit labels, field names) rather than prose.
///
/// Sizes via `RichText::font` with a `FontId` built directly from
/// `type_scale.micro` + `FAMILY_MONO`, not `RichText::text_style` +
/// `TextStyle::Name("Micro")`: the latter depends on `Style::text_styles`
/// already containing that entry, which requires composing in
/// `fonts::text_styles(theme)` wherever the live `Style` gets built,
/// something `main.rs`'s Step 6 bootstrap doesn't currently do. Building
/// the `FontId` straight from the token, like `fonts::text_styles` itself
/// does internally, keeps every helper below correct regardless of that.
pub fn micro_label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let t = theme(ui);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(egui::FontId::new(t.type_scale.micro, egui::FontFamily::Name(FAMILY_MONO.into())))
            .color(t.palette.muted),
    )
}

/// Uppercase section header (mono, `muted`), used above a group of
/// controls. Font via a direct `FontId` (`type_scale.section` +
/// `FAMILY_MONO`), same reasoning as `micro_label`.
pub fn section(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let t = theme(ui);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(egui::FontId::new(t.type_scale.section, egui::FontFamily::Name(FAMILY_MONO.into())))
            .color(t.palette.muted),
    )
}

/// Override `ui`'s interactive widget colors for the duration of `f`, then
/// run `f` (typically `|ui| ui.button(text)`) inside that scope. Used by
/// `ghost_button` below, which picks its own `fill`/`fg`/`border` colors,
/// all sourced from `theme(ui).palette`, and reuses whatever `bg_stroke`
/// width/`corner_radius` Step 4's `visuals()` already set on the ambient
/// style (only the colors are touched here).
fn styled_button<R>(ui: &mut egui::Ui, fill: egui::Color32, fg: egui::Color32, border: egui::Color32, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = theme(ui);
    // Hover/press feedback is a plain static color per state (not a tween,
    // per this step's "no animation" scope), derived from the variant's
    // own fill blended toward the theme's lightest (`text`) or darkest
    // (`ink`) token so it stays correct across every theme without a new
    // palette field.
    let hover_fill = fill.lerp_to_gamma(t.palette.text, 0.12);
    let active_fill = fill.lerp_to_gamma(t.palette.ink, 0.12);

    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        for (wv, bg) in [(&mut widgets.inactive, fill), (&mut widgets.hovered, hover_fill), (&mut widgets.active, active_fill)] {
            wv.weak_bg_fill = bg;
            wv.bg_stroke.color = border;
            wv.fg_stroke.color = fg;
        }
        f(ui)
    })
    .inner
}

/// Transparent (no fill), bordered, low-emphasis button.
pub fn ghost_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let t = theme(ui);
    // `gamma_multiply(0.0)` zeroes the alpha of a real palette color
    // (`surface`) rather than reaching for a bare `Color32::TRANSPARENT`
    // literal, keeping every color in this file traceable to a token.
    let transparent = t.palette.surface.gamma_multiply(0.0);
    styled_button(ui, transparent, t.palette.text, t.palette.border, |ui| ui.button(text))
}

/// A bordered, `surface`-filled container that lifts (lightens) and grows
/// an accent border on hover. No animation: a plain hovered/not-hovered
/// binary switch, computed each frame from `Response::hovered`, matching
/// the "Dynamic color" 2-step `Frame::begin`/`allocate_space`/`paint`
/// pattern documented at `egui/src/containers/frame.rs:78` (fill/stroke
/// aren't known until after `add_contents` has been laid out and the
/// frame's `Response` exists).
pub fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    let t = theme(ui);
    let corner_radius = egui::CornerRadius::from(t.metrics.radius_lg);
    let fill = t.palette.surface;
    let stroke = egui::Stroke::new(t.metrics.border_width, t.palette.border);

    let frame = egui::Frame::NONE.fill(fill).stroke(stroke).corner_radius(corner_radius).inner_margin(t.metrics.spacing_airy);

    let mut prepared = frame.begin(ui);
    let inner = add_contents(&mut prepared.content_ui);
    let response = prepared.allocate_space(ui);
    if response.hovered() {
        prepared.frame.fill = fill.lerp_to_gamma(t.palette.text, 0.03);
        prepared.frame.stroke = egui::Stroke::new(t.metrics.border_width, t.palette.accent);
    }
    prepared.paint(ui);

    egui::InnerResponse::new(inner, response)
}

/// A rounded, `color`-tinted badge: `color` at low opacity as the fill, full
/// `color` as both the border and the uppercase mono text. `color` is a
/// caller-supplied token (typically `theme(ui).palette.{ok,warn,error,
/// accent}`), so `pill` itself never picks a semantic meaning for it.
pub fn pill(ui: &mut egui::Ui, text: &str, color: egui::Color32) -> egui::Response {
    let t = theme(ui);
    egui::Frame::NONE
        .fill(color.gamma_multiply(0.18))
        .stroke(egui::Stroke::new(t.metrics.border_width, color))
        .corner_radius(egui::CornerRadius::from(t.metrics.radius_xl))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text.to_uppercase())
                    .font(egui::FontId::new(t.type_scale.micro, egui::FontFamily::Name(FAMILY_MONO.into())))
                    .color(color),
            )
        })
        .response
}

/// A `micro_label`ed row plus a `pill` reporting `connected` via the
/// theme's `ok` (connected) or `dim` (offline) color.
pub fn connection_row(ui: &mut egui::Ui, label: &str, connected: bool) -> egui::Response {
    let t = theme(ui);
    let (status, color) = if connected { ("Connected", t.palette.ok) } else { ("Offline", t.palette.dim) };
    ui.horizontal(|ui| {
        micro_label(ui, label);
        pill(ui, status, color)
    })
    .response
}

/// A horizontal level meter: an `ink`-filled track with a fill proportional
/// to `level` (clamped to `0.0..=1.0`), colored `ok`/`warn`/`error` past
/// fixed thresholds, bordered with `border`.
pub fn vu_meter(ui: &mut egui::Ui, level: f32) -> egui::Response {
    let t = theme(ui);
    let level = level.clamp(0.0, 1.0);
    let desired_size = egui::vec2(ui.available_width().max(t.metrics.tile_content_w), 8.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let corner_radius = egui::CornerRadius::from(t.metrics.radius_sm);
        ui.painter().rect_filled(rect, corner_radius, t.palette.ink);

        let mut fill_rect = rect;
        fill_rect.set_width(rect.width() * level);
        let fill_color = if level > 0.9 { t.palette.error } else if level > 0.7 { t.palette.warn } else { t.palette.ok };
        ui.painter().rect_filled(fill_rect, corner_radius, fill_color);

        ui.painter().rect_stroke(rect, corner_radius, egui::Stroke::new(t.metrics.border_width, t.palette.border), egui::StrokeKind::Outside);
    }

    response
}

/// A `color`-tinted banner: low-opacity `color` fill, full `color` border,
/// `text`-colored body text. Shared by `error_banner`/`warn_banner` below.
fn banner(ui: &mut egui::Ui, text: &str, color: egui::Color32) -> egui::Response {
    let t = theme(ui);
    egui::Frame::NONE
        .fill(color.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(t.metrics.border_width, color))
        .corner_radius(egui::CornerRadius::from(t.metrics.radius_md))
        .inner_margin(t.metrics.spacing_airy)
        .show(ui, |ui| ui.label(egui::RichText::new(text).color(t.palette.text)))
        .response
}

/// A `banner` tinted with the theme's `error` color.
pub fn error_banner(ui: &mut egui::Ui, text: &str) -> egui::Response {
    banner(ui, text, theme(ui).palette.error)
}

/// A `banner` tinted with the theme's `warn` color.
pub fn warn_banner(ui: &mut egui::Ui, text: &str) -> egui::Response {
    banner(ui, text, theme(ui).palette.warn)
}

/// Paint an accent-colored focus ring just outside `rect`, without
/// allocating layout space or interaction (a caller draws this over a
/// widget it already placed, using that widget's `Response::rect`).
///
/// `radius_lg` (whole-branch review fix wave, finding 6): the only caller
/// (`ui::decks`'s `active_glow`) rings a `card`, which is itself built with
/// `radius_lg`, not `radius_md`: a mismatch here left the ring's corners
/// visibly not following the card's.
pub fn focus_ring(ui: &egui::Ui, rect: egui::Rect) {
    let t = theme(ui);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::from(t.metrics.radius_lg),
        egui::Stroke::new(t.metrics.border_width * 2.0, t.palette.accent),
        egui::StrokeKind::Outside,
    );
}

/// `egui::__run_test_ui` (`egui/src/lib.rs:683`) hardcodes
/// `ctx.set_fonts(FontDefinitions::empty())` and a bare
/// `Context::default()` style, with no way to inject different ones.
/// That's fine for widgets built only from egui's own built-in
/// `TextStyle`s (which stay present, just with empty font lists, under
/// `FontDefinitions::empty()`), but every helper here that goes
/// through a family alias (`FAMILY_MONO`, mandated by this step's
/// "font weight only via family aliases" constraint) needs that
/// family actually registered, or egui panics trying to shape text in
/// a family it's never heard of (`FontFamily::Name("mono") is not
/// bound to any fonts`). `Context::set_fonts` only takes effect "at
/// the start of the next pass" (`context.rs:2124`), too late to
/// matter if called mid-`__run_test_ui`. So: prime a fresh `Context`
/// with the real `font_definitions()` (Step 5) *before* its first
/// pass, mirroring `main.rs`'s bootstrap (Step 6), then run one pass
/// with the same `run_ui`/`drop_without_applying_deltas` mechanics
/// `__run_test_ui` itself uses. Also installs the real `style()`
/// (Step 4), so the ambient `Visuals::widgets` grid (corner radii,
/// border colors) the button/`card` helpers scope over matches
/// production, not `Visuals::dark()`'s own defaults. Step 5's own
/// `fonts.rs` tests hit the fonts half of this identical constraint
/// and likewise never use `__run_test_ui` for anything
/// font-shaping-sensitive.
///
/// `pub(crate)` (Step 13 of the Phase 7 UI redesign plan): panel test
/// modules that call through a family alias too (`ui::decks`'s `card`/
/// `pill`/`micro_label` usage, and every panel Steps 14-21 theme after
/// it) hit this exact constraint, so this lives outside `mod tests`
/// rather than duplicated per panel file.
#[cfg(test)]
pub(crate) fn themed_test_ui(mut add_contents: impl FnMut(&mut egui::Ui)) {
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts::font_definitions());
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);

    let default_theme_id = ThemeId::default();
    let default_style = std::sync::Arc::new(crate::theme::visuals::style(registry::get(default_theme_id)));
    ctx.set_style_of(egui::Theme::Dark, default_style.clone());
    ctx.set_style_of(egui::Theme::Light, default_style);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(THEME_ID_KEY), default_theme_id));

    let output = ctx.run_ui(Default::default(), |ui| add_contents(ui));
    output.drop_without_applying_deltas();
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- theme(): default fallback, no bootstrap wiring required ---------

    #[test]
    fn theme_falls_back_to_default_theme_id_under_run_test_ui() {
        // Deliberately the bare, un-primed harness: this is the one test
        // whose entire point is the ctx.data-absent fallback path.
        egui::__run_test_ui(|ui| {
            assert_eq!(theme(ui).id, ThemeId::default());
        });
    }

    // --- Every helper: no panic, in both the airy (default) and `dense`
    // scopes ----------------------------------------------------------

    #[test]
    fn dense_does_not_panic() {
        themed_test_ui(|ui| {
            dense(ui, |ui| {
                ui.label("inside dense");
            });
        });
    }

    #[test]
    fn micro_label_does_not_panic() {
        themed_test_ui(|ui| {
            micro_label(ui, "gain");
            dense(ui, |ui| {
                micro_label(ui, "gain");
            });
        });
    }

    #[test]
    fn section_does_not_panic() {
        themed_test_ui(|ui| {
            section(ui, "output");
            dense(ui, |ui| {
                section(ui, "output");
            });
        });
    }

    #[test]
    fn ghost_button_does_not_panic() {
        themed_test_ui(|ui| {
            ghost_button(ui, "Cancel");
            dense(ui, |ui| {
                ghost_button(ui, "Cancel");
            });
        });
    }

    #[test]
    fn card_does_not_panic() {
        themed_test_ui(|ui| {
            card(ui, |ui| ui.label("inside card"));
            dense(ui, |ui| {
                card(ui, |ui| ui.label("inside card"));
            });
        });
    }

    #[test]
    fn pill_does_not_panic() {
        themed_test_ui(|ui| {
            let color = theme(ui).palette.ok;
            pill(ui, "live", color);
            dense(ui, |ui| {
                pill(ui, "live", color);
            });
        });
    }

    #[test]
    fn connection_row_does_not_panic() {
        themed_test_ui(|ui| {
            connection_row(ui, "midi", true);
            connection_row(ui, "osc", false);
            dense(ui, |ui| {
                connection_row(ui, "midi", true);
                connection_row(ui, "osc", false);
            });
        });
    }

    #[test]
    fn vu_meter_does_not_panic() {
        themed_test_ui(|ui| {
            vu_meter(ui, 0.5);
            vu_meter(ui, -1.0); // out-of-range: must clamp, not panic
            vu_meter(ui, 2.0);
            dense(ui, |ui| {
                vu_meter(ui, 0.5);
            });
        });
    }

    #[test]
    fn error_banner_does_not_panic() {
        themed_test_ui(|ui| {
            error_banner(ui, "Device disconnected");
            dense(ui, |ui| {
                error_banner(ui, "Device disconnected");
            });
        });
    }

    #[test]
    fn warn_banner_does_not_panic() {
        themed_test_ui(|ui| {
            warn_banner(ui, "Buffer underrun");
            dense(ui, |ui| {
                warn_banner(ui, "Buffer underrun");
            });
        });
    }

    #[test]
    fn focus_ring_does_not_panic() {
        themed_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(40.0, 20.0));
            focus_ring(ui, rect);
            dense(ui, |ui| {
                focus_ring(ui, rect);
            });
        });
    }
}
