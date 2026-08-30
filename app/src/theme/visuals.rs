//! Pure functions translating a `Theme` (Step 3) into egui's real
//! `Visuals`/`Style` types, testable without a `Context` or GL. Nothing
//! here is applied to a `Context` yet: that wiring starts at Step 6.

use egui::style::{HandleShape, Selection, WidgetVisuals, Widgets};
use egui::{CornerRadius, Shadow, Stroke, Style, Visuals};

use super::tokens::Theme;

/// Fixed blend factor used to precompute `faint_bg_color` as an opaque
/// mix of `surface` toward `ink` (`Color32::lerp_to_gamma`, not
/// `Color32::from_additive_luminance`, which is semi-transparent and
/// breaks stacking of opaque backgrounds).
const FAINT_BG_BLEND_TOWARD_INK: f32 = 0.08;

/// Alpha of `window_shadow`'s theme-tinted color (same magnitude as
/// egui's own default `Color32::from_black_alpha(96)`, just tinted with
/// the theme's `ink` instead of pure black).
const WINDOW_SHADOW_ALPHA: u8 = 96;

/// Slider/scrollbar handle aspect ratio, narrower than egui's default
/// `0.75` per the Step 4 brief.
const HANDLE_ASPECT_RATIO: f32 = 0.4;

/// The 5-state `Widgets` grid: starts from egui's own dark-theme
/// defaults (button/checkbox fills, corner radii, text strokes) and
/// overrides only what the brief calls out: every state's `expansion`
/// (egui's default 1px hover growth causes a visible jitter against the
/// violet accent) and `noninteractive.bg_stroke` (1px borders on every
/// existing `Frame::group(ui.style())` call site, without editing those
/// sites).
fn widgets(t: &Theme) -> Widgets {
    let base = Visuals::dark().widgets;
    Widgets {
        noninteractive: WidgetVisuals { bg_stroke: Stroke::new(1.0, t.palette.border), expansion: 0.0, ..base.noninteractive },
        inactive: WidgetVisuals { expansion: 0.0, ..base.inactive },
        hovered: WidgetVisuals { expansion: 0.0, ..base.hovered },
        active: WidgetVisuals { expansion: 0.0, ..base.active },
        open: WidgetVisuals { expansion: 0.0, ..base.open },
    }
}

/// Translate `t` into egui's `Visuals`. Pure: produces a value, applies
/// nothing to a `Context` (that wiring is Step 6).
pub fn visuals(t: &Theme) -> Visuals {
    // `window_corner_radius`/`menu_corner_radius` are the only other
    // `CornerRadius`-typed fields on `Visuals` outside the widgets grid;
    // both keep egui's own default relationship (same radius for both)
    // while now being driven by `metrics.radius_lg` instead of a
    // hardcoded literal.
    let corner_radius = CornerRadius::from(t.metrics.radius_lg);

    Visuals {
        override_text_color: None,
        widgets: widgets(t),
        selection: Selection { bg_fill: t.palette.accent, stroke: Stroke::new(1.0, t.palette.text) },
        faint_bg_color: t.palette.surface.lerp_to_gamma(t.palette.ink, FAINT_BG_BLEND_TOWARD_INK),
        extreme_bg_color: t.palette.ink,
        warn_fg_color: t.palette.warn,
        error_fg_color: t.palette.error,
        window_shadow: Shadow { offset: [10, 20], blur: 15, spread: 0, color: t.palette.ink.gamma_multiply_u8(WINDOW_SHADOW_ALPHA) },
        window_corner_radius: corner_radius,
        menu_corner_radius: corner_radius,
        panel_fill: t.palette.surface,
        slider_trailing_fill: true,
        handle_shape: HandleShape::Rect { aspect_ratio: HANDLE_ASPECT_RATIO },
        ..Visuals::dark()
    }
}

/// Translate `t` into egui's `Style`. Uses the airy (default) spacing
/// scale; `dense` is an explicit scope wired later (Step 8
/// `widgets::dense`).
pub fn style(t: &Theme) -> Style {
    Style { visuals: visuals(t), spacing: egui::style::Spacing { item_spacing: t.metrics.spacing_airy, ..Default::default() }, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::registry::{self, ThemeId};

    const ALL_THEME_IDS: [ThemeId; 3] = [ThemeId::Kushie, ThemeId::OpenDropClassic, ThemeId::Cyan];

    // --- Direct token ports (one assert per field, per brief's "un test
    // par token" requirement) ------------------------------------------

    #[test]
    fn panel_fill_matches_surface_token() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            assert_eq!(visuals(t).panel_fill, t.palette.surface, "{id:?}");
        }
    }

    #[test]
    fn extreme_bg_color_matches_ink_token() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            assert_eq!(visuals(t).extreme_bg_color, t.palette.ink, "{id:?}");
        }
    }

    #[test]
    fn warn_fg_color_matches_warn_token() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            assert_eq!(visuals(t).warn_fg_color, t.palette.warn, "{id:?}");
        }
    }

    #[test]
    fn error_fg_color_matches_error_token() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            assert_eq!(visuals(t).error_fg_color, t.palette.error, "{id:?}");
        }
    }

    #[test]
    fn selection_bg_fill_matches_accent_token() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            assert_eq!(visuals(t).selection.bg_fill, t.palette.accent, "{id:?}");
        }
    }

    #[test]
    fn selection_stroke_color_matches_text_token() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            assert_eq!(visuals(t).selection.stroke.color, t.palette.text, "{id:?}");
        }
    }

    #[test]
    fn slider_trailing_fill_is_always_true() {
        for id in ALL_THEME_IDS {
            assert!(visuals(registry::get(id)).slider_trailing_fill, "{id:?}");
        }
    }

    #[test]
    fn handle_shape_is_rect_with_narrow_aspect_ratio() {
        for id in ALL_THEME_IDS {
            assert_eq!(visuals(registry::get(id)).handle_shape, HandleShape::Rect { aspect_ratio: 0.4 }, "{id:?}");
        }
    }

    #[test]
    fn override_text_color_is_always_none() {
        // Setting this breaks `RichText::weak()`/`strong()`, which depend
        // on `Visuals`'s default text color.
        for id in ALL_THEME_IDS {
            assert_eq!(visuals(registry::get(id)).override_text_color, None, "{id:?}");
        }
    }

    // --- faint_bg_color: opaque precomputed blend, never additive ------

    #[test]
    fn faint_bg_color_is_fully_opaque_not_additive() {
        // `Color32::from_additive_luminance` always produces alpha 0
        // (semi-transparent, breaks stacking of opaque backgrounds).
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            assert_eq!(visuals(t).faint_bg_color.a(), 255, "{id:?}");
        }
    }

    #[test]
    fn faint_bg_color_equals_precomputed_surface_ink_blend() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            let expected = t.palette.surface.lerp_to_gamma(t.palette.ink, FAINT_BG_BLEND_TOWARD_INK);
            assert_eq!(visuals(t).faint_bg_color, expected, "{id:?}");
        }
    }

    #[test]
    fn faint_bg_color_sits_between_ink_and_surface_in_luminance() {
        use crate::theme::tokens::relative_luminance as lum;
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            let faint = visuals(t).faint_bg_color;
            let (li, lf, ls) = (lum(t.palette.ink), lum(faint), lum(t.palette.surface));
            assert!(li <= lf && lf <= ls, "{id:?}: lum(ink)={li} lum(faint)={lf} lum(surface)={ls}");
        }
    }

    // --- window_shadow: theme-tinted, translucent (shadows don't stack
    // like backgrounds do, so alpha compositing is fine here) -----------

    #[test]
    fn window_shadow_color_is_precomputed_ink_tint() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            let expected = t.palette.ink.gamma_multiply_u8(WINDOW_SHADOW_ALPHA);
            assert_eq!(visuals(t).window_shadow.color, expected, "{id:?}");
        }
    }

    // --- widgets grid: 5 states, expansion + noninteractive.bg_stroke --

    #[test]
    fn widgets_grid_all_five_states_have_zero_expansion() {
        for id in ALL_THEME_IDS {
            let w = visuals(registry::get(id)).widgets;
            let states: [(&str, WidgetVisuals); 5] =
                [("noninteractive", w.noninteractive), ("inactive", w.inactive), ("hovered", w.hovered), ("active", w.active), ("open", w.open)];
            for (name, wv) in states {
                assert_eq!(wv.expansion, 0.0, "{id:?} {name}");
            }
        }
    }

    #[test]
    fn noninteractive_bg_stroke_matches_border_token() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            let stroke = visuals(t).widgets.noninteractive.bg_stroke;
            assert_eq!(stroke.width, 1.0, "{id:?}");
            assert_eq!(stroke.color, t.palette.border, "{id:?}");
        }
    }

    // --- radii: metrics scale (2/4/6/10), each radius actually used ----

    #[test]
    fn window_and_menu_corner_radius_conform_to_metrics_radius_lg() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            let expected = CornerRadius::from(t.metrics.radius_lg);
            let v = visuals(t);
            assert_eq!(v.window_corner_radius, expected, "{id:?} window");
            assert_eq!(v.menu_corner_radius, expected, "{id:?} menu");
        }
    }

    // --- style(): visuals + airy spacing --------------------------------

    #[test]
    fn style_visuals_field_matches_visuals_function_output() {
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            assert_eq!(style(t).visuals, visuals(t), "{id:?}");
        }
    }

    #[test]
    fn style_uses_airy_spacing_by_default() {
        // Dense is an explicit scope wired later (Step 8 widgets::dense).
        for id in ALL_THEME_IDS {
            let t = registry::get(id);
            assert_eq!(style(t).spacing.item_spacing, t.metrics.spacing_airy, "{id:?}");
        }
    }
}
