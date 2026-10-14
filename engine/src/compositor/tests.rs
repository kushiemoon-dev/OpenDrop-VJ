use super::*;

/// The overlay pass's geometry and blend-state math: the
/// parts that decide *where* and *how* a sprite lands, testable with
/// no GL context. The GL calls themselves are covered by the headless
/// EGL tests in `app` (`compositor_overlay_gl_tests`), which is where
/// this workspace keeps its real-driver coverage.
mod overlay_geometry {
    use super::*;

    #[test]
    fn a_small_sprite_keeps_its_intrinsic_pixel_size() {
        // 200x100 fits well inside 80% of 1920x1080, no shrink.
        assert_eq!(overlay_quad_half_size_px(200, 100, 1.0), (100.0, 50.0));
    }

    #[test]
    fn scale_multiplies_the_fitted_size() {
        assert_eq!(overlay_quad_half_size_px(200, 100, 2.0), (200.0, 100.0));
        assert_eq!(overlay_quad_half_size_px(200, 100, 0.5), (50.0, 25.0));
    }

    #[test]
    fn an_oversized_sprite_shrinks_to_the_80_percent_box_preserving_aspect() {
        // 3840x2160 (2x the frame, same 16:9): height is the binding
        // constraint at 0.8*1080 = 864, width follows to 1536.
        let (half_w, half_h) = overlay_quad_half_size_px(3840, 2160, 1.0);
        assert!((half_w - 768.0).abs() < 0.01, "half_w = {half_w}");
        assert!((half_h - 432.0).abs() < 0.01, "half_h = {half_h}");
        // aspect preserved
        assert!(((half_w / half_h) - (3840.0 / 2160.0)).abs() < 1e-4);
    }

    #[test]
    fn a_very_wide_sprite_is_bound_by_width_not_height() {
        // 4000x100: 0.8*1920/4000 = 0.384 < 0.8*1080/100 = 8.64.
        let (half_w, half_h) = overlay_quad_half_size_px(4000, 100, 1.0);
        assert!((half_w - 768.0).abs() < 0.01, "half_w = {half_w}");
        assert!((half_h - 19.2).abs() < 0.01, "half_h = {half_h}");
    }

    #[test]
    fn the_fit_only_ever_shrinks_never_enlarges() {
        // A 1x1 sprite must stay 1x1, not blow up to fill 80%.
        assert_eq!(overlay_quad_half_size_px(1, 1, 1.0), (0.5, 0.5));
    }

    #[test]
    fn a_zero_sized_texture_yields_a_zero_quad() {
        assert_eq!(overlay_quad_half_size_px(0, 100, 1.0), (0.0, 0.0));
        assert_eq!(overlay_quad_half_size_px(100, 0, 1.0), (0.0, 0.0));
    }

    #[test]
    fn the_center_maps_normalized_coords_into_gl_pixels_with_y_flipped() {
        assert_eq!(overlay_center_px(0.5, 0.5), (960.0, 540.0));
        // y = 0 is the TOP in `Overlay`'s CSS convention, which is
        // COMP_H in GL's bottom-left-origin pixel space.
        assert_eq!(overlay_center_px(0.0, 0.0), (0.0, 1080.0));
        assert_eq!(overlay_center_px(1.0, 1.0), (1920.0, 0.0));
    }
}

mod overlay_blend_mode {
    use super::*;

    #[test]
    fn css_round_trips_for_every_mode() {
        for mode in OverlayBlendMode::ALL {
            assert_eq!(OverlayBlendMode::from_css(mode.as_css()), mode);
        }
    }

    #[test]
    fn the_css_keywords_are_exactly_the_web_panels_list() {
        // `SidebarOverlays.svelte:32`, same order.
        let css: Vec<&str> = OverlayBlendMode::ALL.iter().map(|m| m.as_css()).collect();
        assert_eq!(css, ["screen", "normal", "plus-lighter", "multiply", "overlay", "hard-light"]);
    }

    #[test]
    fn an_unknown_keyword_falls_back_to_the_overlay_default() {
        // `core::overlay::Overlay::default().blend_mode` is "screen".
        assert_eq!(OverlayBlendMode::from_css("color-dodge"), OverlayBlendMode::Screen);
        assert_eq!(OverlayBlendMode::from_css(""), OverlayBlendMode::Screen);
        assert_eq!(OverlayBlendMode::default(), OverlayBlendMode::Screen);
    }

    #[test]
    fn only_overlay_and_hard_light_need_a_backdrop_copy() {
        let needing: Vec<OverlayBlendMode> =
            OverlayBlendMode::ALL.into_iter().filter(|m| m.needs_backdrop()).collect();
        assert_eq!(needing, [OverlayBlendMode::Overlay, OverlayBlendMode::HardLight]);
    }

    #[test]
    fn every_mode_has_a_distinct_shader_mode_id() {
        let mut ids: Vec<i32> = OverlayBlendMode::ALL.iter().map(|m| m.shader_mode()).collect();
        ids.sort_unstable();
        assert_eq!(ids, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn the_fixed_function_states_match_their_documented_math() {
        assert_eq!(
            OverlayBlendMode::Normal.blend_state(),
            (glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA)
        );
        assert_eq!(
            OverlayBlendMode::PlusLighter.blend_state(),
            (glow::ONE, glow::ONE, glow::ONE, glow::ONE_MINUS_SRC_ALPHA)
        );
        assert_eq!(
            OverlayBlendMode::Screen.blend_state(),
            (glow::ONE, glow::ONE_MINUS_SRC_COLOR, glow::ONE, glow::ONE_MINUS_SRC_ALPHA)
        );
        assert_eq!(
            OverlayBlendMode::Multiply.blend_state(),
            (glow::ZERO, glow::SRC_COLOR, glow::ONE, glow::ONE_MINUS_SRC_ALPHA)
        );
    }

    #[test]
    fn the_backdrop_modes_write_their_result_through_untouched() {
        for mode in [OverlayBlendMode::Overlay, OverlayBlendMode::HardLight] {
            assert_eq!(mode.blend_state(), (glow::ONE, glow::ZERO, glow::ONE, glow::ZERO));
        }
    }
}
