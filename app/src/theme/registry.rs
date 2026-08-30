//! Static registry of the 3 shipped themes. `Metrics`/`Durations`/
//! `TypeScale` are shared (one `&'static` instance for all 3 `Theme`s);
//! only `Palette` differs per theme.

use egui::{vec2, Color32};

use super::tokens::{Durations, Metrics, Palette, Theme, TypeScale};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeId {
    #[default]
    Kushie,
    OpenDropClassic,
    Cyan,
}

static METRICS: Metrics = Metrics {
    radius_sm: 2.0,
    radius_md: 4.0,
    radius_lg: 6.0,
    radius_xl: 10.0,
    border_width: 1.0,
    spacing_airy: vec2(10.0, 8.0),
    spacing_dense: vec2(6.0, 2.0),
    thumb_size: vec2(160.0, 90.0),
    tile_size: vec2(96.0, 54.0),
    tile_content_w: 110.0,
};

static DURATIONS: Durations = Durations { fast: 0.15, base: 0.25, slow: 0.40 };

static TYPE_SCALE: TypeScale =
    TypeScale { display: 24.0, heading: 18.0, section: 15.0, body: 13.0, strong: 13.0, button: 13.0, small: 9.0, micro: 8.0, numeric: 14.0, monospace: 13.0 };

// Shared semantic status colors: same hex across every theme (not part of
// a theme's visual identity), sourced from the companion mockups
// (`shell.html`/`decks-presets.html` `--live`/`--warn`/`--error` custom
// properties, both under `.superpowers/brainstorm/4045861-1788117215/
// content/`).
const OK: Color32 = Color32::from_rgb(0x34, 0xd3, 0x99);
const WARN: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);
const ERROR: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);

static KUSHIE: Theme = Theme {
    id: ThemeId::Kushie,
    palette: Palette {
        ink: Color32::from_rgb(0x0d, 0x11, 0x17),
        surface: Color32::from_rgb(0x13, 0x18, 0x22),
        border: Color32::from_rgb(0x26, 0x2c, 0x38),
        text: Color32::from_rgb(0xe6, 0xed, 0xf3),
        muted: Color32::from_rgb(0x8b, 0x94, 0x9e),
        dim: Color32::from_rgb(0x4a, 0x4f, 0x58),
        accent: Color32::from_rgb(0xa8, 0x55, 0xf7),
        ok: OK,
        warn: WARN,
        error: ERROR,
    },
    metrics: &METRICS,
    durations: &DURATIONS,
    type_scale: &TYPE_SCALE,
};

static OPENDROP_CLASSIC: Theme = Theme {
    id: ThemeId::OpenDropClassic,
    palette: Palette {
        ink: Color32::from_rgb(0x0d, 0x0d, 0x0d),
        surface: Color32::from_rgb(0x14, 0x14, 0x14),
        border: Color32::from_rgb(0x1e, 0x1e, 0x48),
        text: Color32::from_rgb(0xe0, 0xe0, 0xff),
        muted: Color32::from_rgb(0x88, 0x88, 0xbb),
        dim: Color32::from_rgb(0x44, 0x44, 0x7a),
        accent: Color32::from_rgb(0xff, 0x2d, 0x78),
        ok: OK,
        warn: WARN,
        error: ERROR,
    },
    metrics: &METRICS,
    durations: &DURATIONS,
    type_scale: &TYPE_SCALE,
};

// `ink`/`surface` below: see `cyan_palette_hex_values`'s doc comment for
// why `ink` is `#080f12` rather than the plan text's literal `#0e1a1e`.
static CYAN: Theme = Theme {
    id: ThemeId::Cyan,
    palette: Palette {
        ink: Color32::from_rgb(0x08, 0x0f, 0x12),
        surface: Color32::from_rgb(0x0e, 0x1a, 0x1e),
        border: Color32::from_rgb(0x1c, 0x32, 0x38),
        text: Color32::from_rgb(0xe2, 0xf6, 0xf9),
        muted: Color32::from_rgb(0x7e, 0xa8, 0xb0),
        dim: Color32::from_rgb(0x2f, 0x4d, 0x54),
        accent: Color32::from_rgb(0x22, 0xd3, 0xee),
        ok: OK,
        warn: WARN,
        error: ERROR,
    },
    metrics: &METRICS,
    durations: &DURATIONS,
    type_scale: &TYPE_SCALE,
};

pub fn get(id: ThemeId) -> &'static Theme {
    match id {
        ThemeId::Kushie => &KUSHIE,
        ThemeId::OpenDropClassic => &OPENDROP_CLASSIC,
        ThemeId::Cyan => &CYAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::tokens::contrast_ratio;

    #[test]
    fn theme_id_default_is_kushie() {
        assert_eq!(ThemeId::default(), ThemeId::Kushie);
    }

    // --- Kushie: exact hex per palette field -------------------------------

    #[test]
    fn kushie_palette_hex_values() {
        let p = get(ThemeId::Kushie).palette;
        assert_eq!(p.ink, Color32::from_rgb(0x0d, 0x11, 0x17));
        assert_eq!(p.surface, Color32::from_rgb(0x13, 0x18, 0x22));
        assert_eq!(p.border, Color32::from_rgb(0x26, 0x2c, 0x38));
        assert_eq!(p.text, Color32::from_rgb(0xe6, 0xed, 0xf3));
        assert_eq!(p.muted, Color32::from_rgb(0x8b, 0x94, 0x9e));
        assert_eq!(p.dim, Color32::from_rgb(0x4a, 0x4f, 0x58));
        assert_eq!(p.accent, Color32::from_rgb(0xa8, 0x55, 0xf7));
        assert_eq!(p.ok, Color32::from_rgb(0x34, 0xd3, 0x99));
        assert_eq!(p.warn, Color32::from_rgb(0xf5, 0x9e, 0x0b));
        assert_eq!(p.error, Color32::from_rgb(0xff, 0x6b, 0x6b));
    }

    // --- OpenDrop Classic: exact hex per palette field ----------------------

    #[test]
    fn opendrop_classic_palette_hex_values() {
        let p = get(ThemeId::OpenDropClassic).palette;
        assert_eq!(p.ink, Color32::from_rgb(0x0d, 0x0d, 0x0d));
        assert_eq!(p.surface, Color32::from_rgb(0x14, 0x14, 0x14));
        assert_eq!(p.border, Color32::from_rgb(0x1e, 0x1e, 0x48));
        assert_eq!(p.text, Color32::from_rgb(0xe0, 0xe0, 0xff));
        assert_eq!(p.muted, Color32::from_rgb(0x88, 0x88, 0xbb));
        assert_eq!(p.dim, Color32::from_rgb(0x44, 0x44, 0x7a));
        assert_eq!(p.accent, Color32::from_rgb(0xff, 0x2d, 0x78));
        assert_eq!(p.ok, Color32::from_rgb(0x34, 0xd3, 0x99));
        assert_eq!(p.warn, Color32::from_rgb(0xf5, 0x9e, 0x0b));
        assert_eq!(p.error, Color32::from_rgb(0xff, 0x6b, 0x6b));
    }

    // --- Cyan/Midnight: exact hex per palette field -------------------------
    //
    // `ink` is `#080f12`, not the `#0e1a1e` the plan's prose literally
    // states: that literal is `palettes.html`'s `.cyan` `--shell-surface`
    // value, not `--shell-ink` (`--shell-ink:#080f12; --shell-surface:
    // #0e1a1e`). Using `#0e1a1e` for both `ink` and `surface` would make
    // them equal, failing `every_theme_has_monotonically_increasing_
    // elevation` below by construction; `#080f12` is also what every other
    // theme's `ink` vs. `surface` pair in the same file agrees with (ink =
    // the `.odm-nav`/`.odm-card` background, always darker than
    // `.odm-main`'s `surface`).
    #[test]
    fn cyan_palette_hex_values() {
        let p = get(ThemeId::Cyan).palette;
        assert_eq!(p.ink, Color32::from_rgb(0x08, 0x0f, 0x12));
        assert_eq!(p.surface, Color32::from_rgb(0x0e, 0x1a, 0x1e));
        assert_eq!(p.border, Color32::from_rgb(0x1c, 0x32, 0x38));
        assert_eq!(p.text, Color32::from_rgb(0xe2, 0xf6, 0xf9));
        assert_eq!(p.muted, Color32::from_rgb(0x7e, 0xa8, 0xb0));
        assert_eq!(p.dim, Color32::from_rgb(0x2f, 0x4d, 0x54));
        assert_eq!(p.accent, Color32::from_rgb(0x22, 0xd3, 0xee));
        assert_eq!(p.ok, Color32::from_rgb(0x34, 0xd3, 0x99));
        assert_eq!(p.warn, Color32::from_rgb(0xf5, 0x9e, 0x0b));
        assert_eq!(p.error, Color32::from_rgb(0xff, 0x6b, 0x6b));
    }

    // --- Invariants, checked across every theme in the registry -----------

    const ALL_THEME_IDS: [ThemeId; 3] = [ThemeId::Kushie, ThemeId::OpenDropClassic, ThemeId::Cyan];

    #[test]
    fn every_theme_meets_its_wcag_contrast_floor() {
        for id in ALL_THEME_IDS {
            let p = get(id).palette;
            let text_ratio = contrast_ratio(p.text, p.surface);
            let muted_ratio = contrast_ratio(p.muted, p.surface);
            let accent_ratio = contrast_ratio(p.accent, p.surface);
            assert!(text_ratio >= 7.0, "{id:?}: text/surface = {text_ratio}, want >= 7.0");
            assert!(muted_ratio >= 4.5, "{id:?}: muted/surface = {muted_ratio}, want >= 4.5");
            assert!(accent_ratio >= 3.0, "{id:?}: accent/surface = {accent_ratio}, want >= 3.0");
        }
    }

    #[test]
    fn every_palette_color_is_fully_opaque() {
        for id in ALL_THEME_IDS {
            let p = get(id).palette;
            for c in [p.ink, p.surface, p.border, p.text, p.muted, p.dim, p.accent, p.ok, p.warn, p.error] {
                assert_eq!(c.a(), 255, "{id:?}: color {c:?} is not fully opaque");
            }
        }
    }

    #[test]
    fn every_theme_has_monotonically_increasing_elevation() {
        // lum(ink) < lum(surface) < lum(border): a theme switch must never
        // reorder the ink/surface/border elevation stack.
        use super::super::tokens::relative_luminance as lum;
        for id in ALL_THEME_IDS {
            let p = get(id).palette;
            let (li, ls, lb) = (lum(p.ink), lum(p.surface), lum(p.border));
            assert!(li < ls, "{id:?}: lum(ink)={li} not < lum(surface)={ls}");
            assert!(ls < lb, "{id:?}: lum(surface)={ls} not < lum(border)={lb}");
        }
    }

    #[test]
    fn ok_warn_error_are_identical_across_every_theme() {
        // Semantic status colors, not part of a theme's visual identity.
        let kushie = get(ThemeId::Kushie).palette;
        for id in ALL_THEME_IDS {
            let p = get(id).palette;
            assert_eq!(p.ok, kushie.ok);
            assert_eq!(p.warn, kushie.warn);
            assert_eq!(p.error, kushie.error);
        }
    }

    #[test]
    fn all_3_themes_share_the_same_metrics_durations_and_type_scale_allocation() {
        // A theme switch must never reflow a layout: the pointer identity
        // of the shared blocks (not just their value) must be the same
        // `&'static` for every theme.
        let kushie = get(ThemeId::Kushie);
        let classic = get(ThemeId::OpenDropClassic);
        let cyan = get(ThemeId::Cyan);
        assert!(std::ptr::eq(kushie.metrics, classic.metrics));
        assert!(std::ptr::eq(kushie.metrics, cyan.metrics));
        assert!(std::ptr::eq(kushie.durations, classic.durations));
        assert!(std::ptr::eq(kushie.durations, cyan.durations));
        assert!(std::ptr::eq(kushie.type_scale, classic.type_scale));
        assert!(std::ptr::eq(kushie.type_scale, cyan.type_scale));
    }
}
