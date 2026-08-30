//! Pure theme data (Step 3 of the Phase 7 UI redesign plan): color
//! palettes, layout/duration/type-scale tokens and a static registry of
//! the 3 shipped themes, plus the `ease_out_kushie` easing curve. Nothing
//! in this module touches egui's `Context`/`Visuals`/`Style` and nothing
//! calls it yet from `main.rs` or `ui::`: that wiring starts at Step 4.

pub mod easing;
pub mod registry;
pub mod tokens;
pub mod visuals;
