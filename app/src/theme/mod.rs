//! Pure theme data: color
//! palettes, layout/duration/type-scale tokens and a static registry of
//! the 3 shipped themes, plus the `ease_out_kushie` easing curve. Nothing
//! in this module touches egui's `Context`; `bootstrap` and the runtime
//! theme switch apply it.

pub mod easing;
pub mod fonts;
pub mod registry;
pub mod tokens;
pub mod visuals;

/// `Id` key (build with `egui::Id::new(THEME_ID_KEY)`) under which the
/// bootstrap wiring writes the active `ThemeId` into
/// `ctx.data` via `insert_temp`. Read back with
/// `ctx.data(|d| d.get_temp::<registry::ThemeId>(egui::Id::new(THEME_ID_KEY)))`
/// by `widgets.rs` (custom-painted badges/chips not driven by
/// `Style` directly); the runtime theme switch updates it alongside calling
/// `set_style_of` again.
pub const THEME_ID_KEY: &str = "opendrop.theme_id";
