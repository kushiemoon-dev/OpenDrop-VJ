//! Pure, egui-independent theme data: colors, spacing/radius/layout
//! constants, animation durations and the type scale, plus the WCAG
//! contrast formula used to keep every registry theme (`registry.rs`)
//! legible. Nothing here touches an egui `Context`/`Visuals`/`Style`: that
//! wiring starts at Step 4.

use egui::{Color32, Vec2};

use super::registry::ThemeId;

/// Per-theme colors. Shared field names are reused as-is by `widgets.rs`
/// (Step 8) and by Decks (Step 13: bus A = `accent`, bus B = `ok`, off =
/// `dim`): `ok`/`warn`/`error` are the same hex across all 3 themes
/// (semantic status colors, not part of a theme's visual identity), only
/// `accent` and the backgrounds differ per theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub ink: Color32,
    pub surface: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub error: Color32,
}

/// Layout constants shared by all 3 themes (the same `&'static Metrics` for
/// every `Theme`), so switching themes never reflows a layout. `thumb_size`
/// (`ui::decks`, Step 13) and `tile_size`/`tile_content_w` (`ui::
/// preset_browser`, Step 14) were repatriated from those panels' own former
/// local consts; both panels now read straight from here. `ROW_HEIGHT`
/// never joined this struct: Step 14 replaced that fixed constant with
/// `preset_browser::row_height`, derived from `tile_size` plus the live
/// style instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub radius_sm: f32,
    pub radius_md: f32,
    pub radius_lg: f32,
    pub radius_xl: f32,
    pub border_width: f32,
    /// Default spacing scale (loose/"aéré"), used everywhere except the
    /// scopes `dense` (Step 8's density-scope helper) overrides.
    pub spacing_airy: Vec2,
    /// Tighter spacing scale, frozen (no user toggle) to the presets grid,
    /// MIDI-learn rows and playlist lists.
    pub spacing_dense: Vec2,
    pub thumb_size: Vec2,
    pub tile_size: Vec2,
    pub tile_content_w: f32,
}

/// Animation durations, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Durations {
    pub fast: f32,
    pub base: f32,
    pub slow: f32,
}

/// Point sizes for the 10 `TextStyle` names Step 5's `text_styles` maps:
/// egui's built-ins (`heading`/`body`/`button`/`small`/`monospace`) plus
/// the custom `TextStyle::Name`s (`display`/`section`/`strong`/`micro`/
/// `numeric`) referenced by `widgets.rs` (Step 8). Shared by all 3 themes,
/// same reflow-safety reasoning as `Metrics`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeScale {
    pub display: f32,
    pub heading: f32,
    pub section: f32,
    pub body: f32,
    pub strong: f32,
    pub button: f32,
    pub small: f32,
    pub micro: f32,
    pub numeric: f32,
    pub monospace: f32,
}

/// One theme: a per-theme `Palette` plus the 3 token groups shared by every
/// theme in the registry (`&'static`, same allocation for all 3: see
/// `Metrics`'s doc comment for why).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub id: ThemeId,
    pub palette: Palette,
    pub metrics: &'static Metrics,
    pub durations: &'static Durations,
    pub type_scale: &'static TypeScale,
}

/// WCAG 2.x relative luminance
/// (<https://www.w3.org/TR/WCAG21/#dfn-relative-luminance>).
///
/// Test-oracle only (whole-branch review fix wave, finding 2): every
/// caller is a palette invariant test in this file, `visuals.rs`, or
/// `registry.rs`, never production code.
#[cfg(test)]
pub(crate) fn relative_luminance(color: Color32) -> f32 {
    fn channel(c: u8) -> f32 {
        let c = c as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

/// WCAG 2.x contrast ratio between two colors
/// (<https://www.w3.org/TR/WCAG21/#dfn-contrast-ratio>): `(L1 + 0.05) /
/// (L2 + 0.05)` where `L1` is the lighter of the two relative luminances.
///
/// Test-oracle only (whole-branch review fix wave, finding 2): every
/// caller is a palette invariant test, never production code.
#[cfg(test)]
pub(crate) fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_of_black_and_white_is_maximal() {
        let ratio = contrast_ratio(Color32::BLACK, Color32::WHITE);
        assert!((ratio - 21.0).abs() < 0.01, "expected ~21.0, got {ratio}");
    }

    #[test]
    fn contrast_of_a_color_with_itself_is_one() {
        assert!((contrast_ratio(Color32::from_rgb(120, 80, 200), Color32::from_rgb(120, 80, 200)) - 1.0).abs() < 0.001);
    }

    #[test]
    fn contrast_ratio_is_order_independent() {
        let a = Color32::from_rgb(230, 237, 243);
        let b = Color32::from_rgb(19, 24, 34);
        assert_eq!(contrast_ratio(a, b), contrast_ratio(b, a));
    }
}
