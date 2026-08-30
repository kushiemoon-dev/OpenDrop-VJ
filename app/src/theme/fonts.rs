//! Pure font data (Step 5 of the Phase 7 UI redesign plan): embeds the 2
//! variable-weight fonts vendored at Step 2 and registers 6 weight-alias
//! `FontFamily::Name`s over them, plus the `text_styles` size map driven by
//! `TypeScale` (Step 3). Nothing here is applied to a `Context` yet (that
//! wiring starts at Step 6).

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::epaint::text::VariationCoords;
use egui::{FontData, FontDefinitions, FontFamily, FontId, FontTweak, TextStyle};

use super::tokens::Theme;

const INTER_VARIABLE: &[u8] = include_bytes!("../../assets/fonts/Inter-Variable.ttf");
const JETBRAINS_MONO_VARIABLE: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Variable.ttf");

/// The 6 weight-alias family names registered by [`font_definitions`].
pub const FAMILY_UI: &str = "ui";
pub const FAMILY_UI_MEDIUM: &str = "ui-medium";
pub const FAMILY_UI_SEMIBOLD: &str = "ui-semibold";
pub const FAMILY_UI_BOLD: &str = "ui-bold";
pub const FAMILY_MONO: &str = "mono";
pub const FAMILY_MONO_BOLD: &str = "mono-bold";

/// Fallback chain already used by `FontDefinitions::default()`'s
/// `FontFamily::Proportional`, appended verbatim after each Inter-based
/// alias's primary font so this alias's glyph coverage is a strict superset
/// of what `Proportional` already covered (see the `fallback_queue_*` test:
/// neither `⇄` nor `🔒` is actually in this particular chain today).
const PROPORTIONAL_FALLBACK: [&str; 3] = ["Ubuntu-Light", "NotoEmoji-Regular", "emoji-icon-font"];

/// Fallback chain already used by `FontDefinitions::default()`'s
/// `FontFamily::Monospace`, appended after each JetBrains-Mono-based
/// alias's primary font.
const MONOSPACE_FALLBACK: [&str; 4] = ["Hack", "Ubuntu-Light", "NotoEmoji-Regular", "emoji-icon-font"];

/// A `FontData` for `blob` at variation coordinate `wght` (variable-font
/// weight axis). `FontData::from_static` stores a `Cow::Borrowed`, so the
/// underlying `.rodata` bytes are shared across every weight alias built
/// from the same blob.
fn weighted(blob: &'static [u8], wght: f32) -> Arc<FontData> {
    Arc::new(FontData::from_static(blob).tweak(FontTweak { coords: VariationCoords::new([("wght", wght)]), ..Default::default() }))
}

/// Register `name` as a family whose primary font is `font_data_key`,
/// falling back to `fallback` (a chain already present in `defs`, courtesy
/// of `FontDefinitions::default()`).
fn register_family(defs: &mut FontDefinitions, name: &str, font_data_key: &str, fallback: &[&str]) {
    defs.families.insert(
        FontFamily::Name(name.into()),
        std::iter::once(font_data_key.to_owned()).chain(fallback.iter().map(|s| s.to_string())).collect(),
    );
}

/// Build the `FontDefinitions` for the app: Inter and JetBrains Mono,
/// each registered at several variation-font weights under a
/// `FontFamily::Name` alias. Built from `FontDefinitions::default()` (never
/// `::empty()`) so the builtin fallback fonts survive: the UI already
/// renders `⇄`/`🔒` (`playlists.rs:72,106,107`), absent from both vendored
/// fonts, via those fallbacks (partially: see `fallback_queue_*` test).
pub fn font_definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();

    defs.font_data.insert("Inter-400".to_owned(), weighted(INTER_VARIABLE, 400.0));
    defs.font_data.insert("Inter-500".to_owned(), weighted(INTER_VARIABLE, 500.0));
    defs.font_data.insert("Inter-600".to_owned(), weighted(INTER_VARIABLE, 600.0));
    defs.font_data.insert("Inter-700".to_owned(), weighted(INTER_VARIABLE, 700.0));
    defs.font_data.insert("JetBrainsMono-400".to_owned(), weighted(JETBRAINS_MONO_VARIABLE, 400.0));
    defs.font_data.insert("JetBrainsMono-700".to_owned(), weighted(JETBRAINS_MONO_VARIABLE, 700.0));

    register_family(&mut defs, FAMILY_UI, "Inter-400", &PROPORTIONAL_FALLBACK);
    register_family(&mut defs, FAMILY_UI_MEDIUM, "Inter-500", &PROPORTIONAL_FALLBACK);
    register_family(&mut defs, FAMILY_UI_SEMIBOLD, "Inter-600", &PROPORTIONAL_FALLBACK);
    register_family(&mut defs, FAMILY_UI_BOLD, "Inter-700", &PROPORTIONAL_FALLBACK);
    register_family(&mut defs, FAMILY_MONO, "JetBrainsMono-400", &MONOSPACE_FALLBACK);
    register_family(&mut defs, FAMILY_MONO_BOLD, "JetBrainsMono-700", &MONOSPACE_FALLBACK);

    defs
}

/// Size + family map for every `TextStyle` known at this step: egui's 5
/// built-ins (`Heading`/`Body`/`Button`/`Small`/`Monospace`) plus the 5
/// custom `TextStyle::Name`s that `widgets.rs` (Step 8) will reference
/// (`Display`/`Section`/`Strong`/`Micro`/`Numeric`). Sizes come from
/// `TypeScale` (Step 3); families follow the app's mockups
/// (`.superpowers/brainstorm/4045861-1788117215/content/{shell,compare,decks-v2}.html`):
/// prose stays on Inter (`ui*`), chrome/labels/numeric readouts use
/// JetBrains Mono (`mono`) for alignment and a technical feel.
///
/// Lives here rather than in `visuals::style()` (Step 4) because `TypeScale`
/// (Step 3) documents its 10 fields as "Point sizes for the 10 `TextStyle`
/// names Step 5's `text_styles` maps" and this keeps every font-related
/// decision in one file, not split across two.
pub fn text_styles(theme: &Theme) -> BTreeMap<TextStyle, FontId> {
    let ts = theme.type_scale;
    let ui = |size: f32| FontId::new(size, FontFamily::Name(FAMILY_UI.into()));
    let ui_semibold = |size: f32| FontId::new(size, FontFamily::Name(FAMILY_UI_SEMIBOLD.into()));
    let ui_bold = |size: f32| FontId::new(size, FontFamily::Name(FAMILY_UI_BOLD.into()));
    let mono = |size: f32| FontId::new(size, FontFamily::Name(FAMILY_MONO.into()));

    BTreeMap::from([
        (TextStyle::Heading, ui_bold(ts.heading)),
        (TextStyle::Body, ui(ts.body)),
        (TextStyle::Button, mono(ts.button)),
        (TextStyle::Small, ui(ts.small)),
        (TextStyle::Monospace, mono(ts.monospace)),
        (TextStyle::Name("Display".into()), ui_bold(ts.display)),
        (TextStyle::Name("Section".into()), mono(ts.section)),
        (TextStyle::Name("Strong".into()), ui_semibold(ts.strong)),
        (TextStyle::Name("Micro".into()), mono(ts.micro)),
        (TextStyle::Name("Numeric".into()), mono(ts.numeric)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::text::{Fonts, TextOptions};
    use crate::theme::registry::{self, ThemeId};

    const ALL_THEME_IDS: [ThemeId; 3] = [ThemeId::Kushie, ThemeId::OpenDropClassic, ThemeId::Cyan];

    const ALL_FAMILIES: [&str; 6] = [FAMILY_UI, FAMILY_UI_MEDIUM, FAMILY_UI_SEMIBOLD, FAMILY_UI_BOLD, FAMILY_MONO, FAMILY_MONO_BOLD];

    // --- FontFamily::Name coverage: all 6 weight aliases exist and point
    // at real font_data entries ------------------------------------------

    #[test]
    fn all_six_weight_aliases_are_registered_families() {
        let defs = font_definitions();
        for name in ALL_FAMILIES {
            let fonts = defs.families.get(&FontFamily::Name(name.into())).unwrap_or_else(|| panic!("missing family {name:?}"));
            let primary = fonts.first().unwrap_or_else(|| panic!("family {name:?} has no fonts"));
            assert!(defs.font_data.contains_key(primary), "family {name:?}'s primary font {primary:?} missing from font_data");
        }
    }

    #[test]
    fn built_from_default_keeps_builtin_emoji_fonts() {
        // Never ::empty(): the builtin NotoEmoji-Regular/emoji-icon-font
        // fonts must still be present for the fallback queue to work.
        let defs = font_definitions();
        assert!(defs.font_data.contains_key("NotoEmoji-Regular"));
        assert!(defs.font_data.contains_key("emoji-icon-font"));
    }

    #[test]
    fn each_weight_alias_falls_back_to_the_builtin_emoji_chain() {
        let defs = font_definitions();
        for name in ALL_FAMILIES {
            let fonts = defs.families.get(&FontFamily::Name(name.into())).unwrap();
            assert!(fonts.contains(&"NotoEmoji-Regular".to_owned()), "family {name:?} missing emoji fallback");
            assert!(fonts.contains(&"emoji-icon-font".to_owned()), "family {name:?} missing emoji-icon fallback");
        }
    }

    // --- TextStyle::Name coverage: every name known at this step exists --

    #[test]
    fn text_styles_covers_every_name_known_at_this_step() {
        for id in ALL_THEME_IDS {
            let theme = registry::get(id);
            let styles = text_styles(theme);
            let expected = [
                TextStyle::Heading,
                TextStyle::Body,
                TextStyle::Button,
                TextStyle::Small,
                TextStyle::Monospace,
                TextStyle::Name("Display".into()),
                TextStyle::Name("Section".into()),
                TextStyle::Name("Strong".into()),
                TextStyle::Name("Micro".into()),
                TextStyle::Name("Numeric".into()),
            ];
            for style in expected {
                assert!(styles.contains_key(&style), "{id:?} missing {style:?}");
            }
        }
    }

    #[test]
    fn text_styles_sizes_match_type_scale() {
        for id in ALL_THEME_IDS {
            let theme = registry::get(id);
            let ts = theme.type_scale;
            let styles = text_styles(theme);
            assert_eq!(styles[&TextStyle::Heading].size, ts.heading, "{id:?}");
            assert_eq!(styles[&TextStyle::Body].size, ts.body, "{id:?}");
            assert_eq!(styles[&TextStyle::Button].size, ts.button, "{id:?}");
            assert_eq!(styles[&TextStyle::Small].size, ts.small, "{id:?}");
            assert_eq!(styles[&TextStyle::Monospace].size, ts.monospace, "{id:?}");
            assert_eq!(styles[&TextStyle::Name("Display".into())].size, ts.display, "{id:?}");
            assert_eq!(styles[&TextStyle::Name("Section".into())].size, ts.section, "{id:?}");
            assert_eq!(styles[&TextStyle::Name("Strong".into())].size, ts.strong, "{id:?}");
            assert_eq!(styles[&TextStyle::Name("Micro".into())].size, ts.micro, "{id:?}");
            assert_eq!(styles[&TextStyle::Name("Numeric".into())].size, ts.numeric, "{id:?}");
        }
    }

    // --- Glyph coverage: the new named families don't regress whatever
    // ⇄/🔒 coverage `FontDefinitions::default()`'s own `Proportional`/
    // `Monospace` chains already had, since `playlists.rs:72,106,107`
    // render those glyphs today via those chains. Measured, not assumed:
    // neither glyph is fully covered by egui's builtin fonts as of egui
    // 0.36.1 (`⇄` isn't in any of them; `🔒` is only in `Monospace`'s
    // chain, via `Hack`): a pre-existing gap Step 15 owns, not one this
    // step introduces or is responsible for closing.

    #[test]
    fn fallback_queue_does_not_regress_builtin_glyph_coverage() {
        let glyphs = ["\u{21c4}", "\u{1f512}"]; // ⇄, 🔒

        let mut baseline_fonts = Fonts::new(TextOptions::default(), FontDefinitions::default());
        let mut baseline = baseline_fonts.with_pixels_per_point(1.0);
        let proportional_baseline: Vec<bool> = glyphs.iter().map(|g| baseline.has_glyphs(&FontId::new(16.0, FontFamily::Proportional), g)).collect();
        let monospace_baseline: Vec<bool> = glyphs.iter().map(|g| baseline.has_glyphs(&FontId::new(16.0, FontFamily::Monospace), g)).collect();

        let mut fonts = Fonts::new(TextOptions::default(), font_definitions());
        let mut view = fonts.with_pixels_per_point(1.0);
        let ui_families = [FAMILY_UI, FAMILY_UI_MEDIUM, FAMILY_UI_SEMIBOLD, FAMILY_UI_BOLD];
        let mono_families = [FAMILY_MONO, FAMILY_MONO_BOLD];

        for name in ui_families {
            for (i, glyph) in glyphs.iter().enumerate() {
                let ok = view.has_glyphs(&FontId::new(16.0, FontFamily::Name(name.into())), glyph);
                assert_eq!(ok, proportional_baseline[i], "{name:?} glyph {glyph:?} regressed vs the Proportional baseline");
            }
        }
        for name in mono_families {
            for (i, glyph) in glyphs.iter().enumerate() {
                let ok = view.has_glyphs(&FontId::new(16.0, FontFamily::Name(name.into())), glyph);
                assert_eq!(ok, monospace_baseline[i], "{name:?} glyph {glyph:?} regressed vs the Monospace baseline");
            }
        }
    }
}
