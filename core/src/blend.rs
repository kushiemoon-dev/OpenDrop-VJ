//! Port of OpenDrop-VJ `src/lib/engine/compositor.ts` (lines 1-104) and
//! `src/lib/engine/sync.ts` (lines 1-60): the blend-mode enum, symbolic GL
//! blend-factor state, and the per-slot/per-deck compositing config. Pure
//! state/logic only; the real WebGL blend-equation wiring belongs to a
//! later `engine` crate.

/// Blend mode for one deck slot. Matches the TS `BlendMode` union shared by
/// compositor.ts and sync.ts (unified here as a single enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Additive,
    Screen,
    Multiply,
}

/// Symbolic GL blend factors: NOT real OpenGL enum values. The GL-facing
/// compositor (later `engine` crate) maps these to real gl::* constants at
/// draw time, so this module stays testable without a GL context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlBlend {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    SrcAlpha,
    OneMinusSrcAlpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlendState {
    pub src_rgb: GlBlend,
    pub dst_rgb: GlBlend,
    pub src_a: GlBlend,
    pub dst_a: GlBlend,
}

const BLEND_MODES: [BlendMode; 4] =
    [BlendMode::Normal, BlendMode::Additive, BlendMode::Screen, BlendMode::Multiply];

/// GPU blend-equation factors for each mode. Alpha coverage is constant
/// across all modes so keyed-out / transparent regions still reveal
/// whatever is behind the compositor canvas (video layer, background).
pub fn blend_state_for(mode: BlendMode) -> BlendState {
    let (src_a, dst_a) = (GlBlend::One, GlBlend::OneMinusSrcAlpha);
    match mode {
        BlendMode::Normal => {
            BlendState { src_rgb: GlBlend::One, dst_rgb: GlBlend::OneMinusSrcAlpha, src_a, dst_a }
        }
        BlendMode::Additive => {
            BlendState { src_rgb: GlBlend::One, dst_rgb: GlBlend::One, src_a, dst_a }
        }
        BlendMode::Screen => {
            BlendState { src_rgb: GlBlend::One, dst_rgb: GlBlend::OneMinusSrcColor, src_a, dst_a }
        }
        BlendMode::Multiply => {
            BlendState { src_rgb: GlBlend::Zero, dst_rgb: GlBlend::SrcColor, src_a, dst_a }
        }
    }
}

/// Decode a MIDI/keyboard range value (0..1) into one of the 4 blend modes.
/// 4 equal buckets: [0,.25)->normal [.25,.5)->additive [.5,.75)->screen [.75,1]->multiply.
pub fn blend_mode_from_value01(v: f64) -> BlendMode {
    let idx = (v * BLEND_MODES.len() as f64)
        .floor()
        .max(0.0)
        .min((BLEND_MODES.len() - 1) as f64);
    BLEND_MODES[idx as usize]
}

/// Inverse of `blend_mode_from_value01`: the bucket center for a mode, used
/// so MIDI soft-takeover has a "current value" to compare an incoming CC against.
pub fn blend_mode_to_value01(mode: BlendMode) -> f64 {
    let idx = BLEND_MODES.iter().position(|&m| m == mode).unwrap();
    (idx as f64 + 0.5) / BLEND_MODES.len() as f64
}

/// One-shot migration from the old global CSS `mix-blend-mode` string
/// (od-blendmode) to the new `BlendMode` enum. Modes with no equivalent
/// collapse to `Normal`.
pub fn migrate_blend_mode_string(old: &str) -> BlendMode {
    match old {
        "screen" => BlendMode::Screen,
        "multiply" => BlendMode::Multiply,
        "plus-lighter" => BlendMode::Additive,
        _ => BlendMode::Normal,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorParams {
    pub hue_rotate: f64, // 0..1 -> 0..360deg
    pub saturate: f64,   // 0..1 -> 0..200% (0.5 = 100% = normal)
    pub brightness: f64, // 0..1 -> 0..200% (0.5 = 100% = normal)
    pub contrast: f64,   // 0..1 -> 0..200% (0.5 = 100% = normal)
    pub invert: f64,     // 0..1
}

pub const DEFAULT_COLOR_PARAMS: ColorParams = ColorParams {
    hue_rotate: 0.0,
    saturate: 0.5,
    brightness: 0.5,
    contrast: 0.5,
    invert: 0.0,
};

/// CSS `filter` value for these color params; `"none"` when every channel
/// sits at its no-op default (0.5 = 100% for saturate/brightness/contrast).
pub fn color_params_to_filter(p: ColorParams) -> String {
    let is_default = p.hue_rotate == 0.0
        && p.saturate == 0.5
        && p.brightness == 0.5
        && p.contrast == 0.5
        && p.invert == 0.0;
    if is_default {
        return "none".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    if p.hue_rotate != 0.0 {
        parts.push(format!("hue-rotate({}deg)", (p.hue_rotate * 360.0).round() as i64));
    }
    if p.saturate != 0.5 {
        parts.push(format!("saturate({}%)", (p.saturate * 200.0).round() as i64));
    }
    if p.brightness != 0.5 {
        parts.push(format!("brightness({}%)", (p.brightness * 200.0).round() as i64));
    }
    if p.contrast != 0.5 {
        parts.push(format!("contrast({}%)", (p.contrast * 200.0).round() as i64));
    }
    if p.invert != 0.0 {
        parts.push(format!("invert({}%)", (p.invert * 100.0).round() as i64));
    }
    parts.join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotComposite {
    pub blend: BlendMode,
    pub luma_key: bool,
    pub luma_black: f64, // 0..1
    pub luma_white: f64, // 0..1
    pub color_key: bool,
    pub color_hue: f64, // 0..1 -> 0..360deg
    pub color_tol: f64, // 0..1
}

pub const DEFAULT_SLOT_COMPOSITE: SlotComposite = SlotComposite {
    blend: BlendMode::Normal,
    luma_key: false,
    luma_black: 0.0,
    luma_white: 1.0,
    color_key: false,
    color_hue: 0.0,
    color_tol: 0.0,
};

/// Port of OpenDrop-VJ `compositor.ts:140` `shouldForceNormalForLowestSlot`
///: whole-branch review Finding I5. Whether the lowest active deck slot
/// should be forced to `BlendMode::Normal`: multiply/screen/additive against
/// a still-transparent framebuffer reads wrong (e.g. multiply -> black).
/// Independent of any video/NDI-in layer, which draws last, on top of the
/// deck stack, not underneath it.
///
/// `lowest_active` is `None` when no slot is active at all (every slot at or
/// below the compositor's 0.001 opacity floor): the idiomatic Rust
/// equivalent of the TS caller's `lowestActive: number` sentinel, and the
/// same `Option<usize>` shape `app`'s own `(0..DECK_COUNT).find(...)` call
/// site already produces.
pub fn should_force_normal_for_lowest_slot(slot: usize, lowest_active: Option<usize>) -> bool {
    lowest_active == Some(slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod blend_state_for {
        use super::*;

        #[test]
        fn normal_is_classic_over_with_constant_alpha_coverage() {
            assert_eq!(
                blend_state_for(BlendMode::Normal),
                BlendState {
                    src_rgb: GlBlend::One,
                    dst_rgb: GlBlend::OneMinusSrcAlpha,
                    src_a: GlBlend::One,
                    dst_a: GlBlend::OneMinusSrcAlpha,
                }
            );
        }

        #[test]
        fn additive_is_one_one_in_rgb() {
            assert_eq!(
                blend_state_for(BlendMode::Additive),
                BlendState {
                    src_rgb: GlBlend::One,
                    dst_rgb: GlBlend::One,
                    src_a: GlBlend::One,
                    dst_a: GlBlend::OneMinusSrcAlpha,
                }
            );
        }

        #[test]
        fn screen_is_one_one_minus_src_color_in_rgb() {
            assert_eq!(
                blend_state_for(BlendMode::Screen),
                BlendState {
                    src_rgb: GlBlend::One,
                    dst_rgb: GlBlend::OneMinusSrcColor,
                    src_a: GlBlend::One,
                    dst_a: GlBlend::OneMinusSrcAlpha,
                }
            );
        }

        #[test]
        fn multiply_is_zero_src_color_in_rgb() {
            assert_eq!(
                blend_state_for(BlendMode::Multiply),
                BlendState {
                    src_rgb: GlBlend::Zero,
                    dst_rgb: GlBlend::SrcColor,
                    src_a: GlBlend::One,
                    dst_a: GlBlend::OneMinusSrcAlpha,
                }
            );
        }

        #[test]
        fn coverage_alpha_is_identical_across_the_4_modes() {
            for mode in [BlendMode::Normal, BlendMode::Additive, BlendMode::Screen, BlendMode::Multiply] {
                let bs = blend_state_for(mode);
                assert_eq!(bs.src_a, GlBlend::One);
                assert_eq!(bs.dst_a, GlBlend::OneMinusSrcAlpha);
            }
        }
    }

    mod blend_mode_from_value01 {
        use super::*;

        #[test]
        fn zero_is_normal() {
            assert_eq!(blend_mode_from_value01(0.0), BlendMode::Normal);
        }

        #[test]
        fn zero_point_24_is_normal() {
            assert_eq!(blend_mode_from_value01(0.24), BlendMode::Normal);
        }

        #[test]
        fn zero_point_25_is_additive() {
            assert_eq!(blend_mode_from_value01(0.25), BlendMode::Additive);
        }

        #[test]
        fn zero_point_5_is_screen() {
            assert_eq!(blend_mode_from_value01(0.5), BlendMode::Screen);
        }

        #[test]
        fn zero_point_75_is_multiply() {
            assert_eq!(blend_mode_from_value01(0.75), BlendMode::Multiply);
        }

        #[test]
        fn one_is_multiply_clamped() {
            assert_eq!(blend_mode_from_value01(1.0), BlendMode::Multiply);
        }

        #[test]
        fn negative_out_of_range_is_normal_clamped() {
            assert_eq!(blend_mode_from_value01(-0.5), BlendMode::Normal);
        }
    }

    mod blend_mode_to_value01 {
        use super::*;

        #[test]
        fn returns_the_bucket_center_for_each_mode() {
            assert!((blend_mode_to_value01(BlendMode::Normal) - 0.125).abs() < 1e-9);
            assert!((blend_mode_to_value01(BlendMode::Additive) - 0.375).abs() < 1e-9);
            assert!((blend_mode_to_value01(BlendMode::Screen) - 0.625).abs() < 1e-9);
            assert!((blend_mode_to_value01(BlendMode::Multiply) - 0.875).abs() < 1e-9);
        }

        #[test]
        fn round_trips_with_blend_mode_from_value01() {
            for mode in [BlendMode::Normal, BlendMode::Additive, BlendMode::Screen, BlendMode::Multiply] {
                assert_eq!(blend_mode_from_value01(blend_mode_to_value01(mode)), mode);
            }
        }
    }

    mod migrate_blend_mode_string {
        use super::*;

        #[test]
        fn screen_maps_to_screen() {
            assert_eq!(migrate_blend_mode_string("screen"), BlendMode::Screen);
        }

        #[test]
        fn multiply_maps_to_multiply() {
            assert_eq!(migrate_blend_mode_string("multiply"), BlendMode::Multiply);
        }

        #[test]
        fn plus_lighter_maps_to_additive() {
            assert_eq!(migrate_blend_mode_string("plus-lighter"), BlendMode::Additive);
        }

        #[test]
        fn overlay_unsupported_maps_to_normal() {
            assert_eq!(migrate_blend_mode_string("overlay"), BlendMode::Normal);
        }

        #[test]
        fn lighten_unsupported_maps_to_normal() {
            assert_eq!(migrate_blend_mode_string("lighten"), BlendMode::Normal);
        }

        #[test]
        fn any_unknown_value_maps_to_normal() {
            assert_eq!(migrate_blend_mode_string("garbage"), BlendMode::Normal);
        }
    }

    mod color_params_to_filter {
        use super::*;

        #[test]
        fn defaults_produce_none() {
            assert_eq!(color_params_to_filter(DEFAULT_COLOR_PARAMS), "none");
        }

        #[test]
        fn hue_rotate_channel() {
            let p = ColorParams { hue_rotate: 0.5, ..DEFAULT_COLOR_PARAMS };
            assert_eq!(color_params_to_filter(p), "hue-rotate(180deg)");
        }

        #[test]
        fn saturate_channel() {
            let p = ColorParams { saturate: 1.0, ..DEFAULT_COLOR_PARAMS };
            assert_eq!(color_params_to_filter(p), "saturate(200%)");
        }

        #[test]
        fn brightness_channel() {
            let p = ColorParams { brightness: 0.0, ..DEFAULT_COLOR_PARAMS };
            assert_eq!(color_params_to_filter(p), "brightness(0%)");
        }

        #[test]
        fn contrast_channel() {
            let p = ColorParams { contrast: 1.0, ..DEFAULT_COLOR_PARAMS };
            assert_eq!(color_params_to_filter(p), "contrast(200%)");
        }

        #[test]
        fn invert_channel() {
            let p = ColorParams { invert: 1.0, ..DEFAULT_COLOR_PARAMS };
            assert_eq!(color_params_to_filter(p), "invert(100%)");
        }

        #[test]
        fn multiple_non_default_channels_join_with_a_space_in_declared_order() {
            let p = ColorParams { hue_rotate: 0.5, invert: 1.0, ..DEFAULT_COLOR_PARAMS };
            assert_eq!(color_params_to_filter(p), "hue-rotate(180deg) invert(100%)");
        }
    }

    /// Port of `compositor.test.ts`'s `shouldForceNormalForLowestSlot` tests.
    mod should_force_normal_for_lowest_slot {
        use super::*;

        #[test]
        fn forces_normal_on_the_lowest_active_slot() {
            assert!(should_force_normal_for_lowest_slot(0, Some(0)));
        }

        #[test]
        fn never_forces_a_non_lowest_slot() {
            assert!(!should_force_normal_for_lowest_slot(1, Some(0)));
        }

        #[test]
        fn no_slot_active_at_all_forces_nothing() {
            assert!(!should_force_normal_for_lowest_slot(0, None));
        }
    }

    mod defaults {
        use super::*;

        #[test]
        fn default_color_params() {
            assert_eq!(
                DEFAULT_COLOR_PARAMS,
                ColorParams { hue_rotate: 0.0, saturate: 0.5, brightness: 0.5, contrast: 0.5, invert: 0.0 }
            );
        }

        #[test]
        fn default_slot_composite() {
            assert_eq!(
                DEFAULT_SLOT_COMPOSITE,
                SlotComposite {
                    blend: BlendMode::Normal,
                    luma_key: false,
                    luma_black: 0.0,
                    luma_white: 1.0,
                    color_key: false,
                    color_hue: 0.0,
                    color_tol: 0.0,
                }
            );
        }
    }
}
