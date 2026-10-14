use super::*;

fn assign(index: u16, var: &str) -> PatchTarget {
    PatchTarget {
        index,
        initial: 0.0,
        apply: Apply::Assign(var.to_string()),
    }
}

// ---- encode_param ----

#[test]
fn encodes_index_in_the_low_digits_and_value_in_the_high_ones() {
    // 0.7 → (0.7 + 2) * 1000 = 2700, index 1, plus the 10^7 tag.
    assert_eq!(encode_param(1, 0.7), 12_700_001);
}

#[test]
fn encodes_the_range_endpoints_without_overflowing() {
    assert_eq!(encode_param(1, VALUE_MIN), 10_000_001);
    assert_eq!(encode_param(MAX_INDEX, VALUE_MAX), 14_000_999);
    // The Milkdrop evaluator was measured to be 32-bit float, so exactness
    // really does stop at 2^24. This is the binding constraint, not int32.
    assert!(encode_param(MAX_INDEX, VALUE_MAX) < 1 << 24);
}

#[test]
fn encodes_zero_index_as_the_no_op_slot() {
    assert_eq!(encode_param(0, 0.0) % INDEX_SPAN, 0);
}

#[test]
fn clamps_out_of_range_values_and_indices() {
    assert_eq!(encode_param(1, 99.0), encode_param(1, VALUE_MAX));
    assert_eq!(encode_param(1, -99.0), encode_param(1, VALUE_MIN));
    assert_eq!(encode_param(60_000, 0.0), encode_param(MAX_INDEX, 0.0));
}

#[test]
fn round_trips_every_representable_step() {
    let mut v = VALUE_MIN;
    while v <= VALUE_MAX {
        let code = encode_param(7, v);
        assert_eq!(code % INDEX_SPAN, 7);
        let decoded = f64::from((code - WORD_TAG) / INDEX_SPAN) / VALUE_SCALE - VALUE_OFFSET;
        assert!((decoded - v).abs() < 1e-9, "{v} decoded as {decoded}");
        v += 0.01;
    }
}

#[test]
fn tags_every_word_clear_of_any_plausible_raw_frame_rate() {
    // The whole point of the tag: nothing a projectM instance could be
    // holding in `fps` may pass the preset-side guard. 35 is what 4.1.6
    // actually starts at; 30/45/60 are the app's own quality settings.
    for raw in [0, 1, MEASURED_DEFAULT_FPS, 30, 45, 60, 240, 1000, WORD_GUARD] {
        assert!(raw <= WORD_GUARD, "{raw} would be read as a command");
    }
    let mut v = VALUE_MIN;
    while v <= VALUE_MAX {
        for index in [0, 1, 40, MAX_INDEX] {
            assert!(encode_param(index, v) > WORD_GUARD);
        }
        v += 0.01;
    }
}

// ---- fps substitution ----

#[test]
fn substitutes_the_fps_variable_in_every_equation_block() {
    let src = "per_frame_1=vy = vy - 0.0001*60/fps;\n\
               per_pixel_1=zoom = zoom + fps*0.001;\n\
               per_frame_init_1=k = fps;\n\
               shape_0_per_frame1=r = fps/120;\n\
               wave_0_per_frame1=a = fps;\n\
               wave_0_per_point1=x = x + fps;\n";
    let out = patch_preset(src, &[], MEASURED_DEFAULT_FPS);
    assert!(!out.contains("fps"), "{out}");
    assert_eq!(out.matches("35").count(), 6, "{out}");
    // The literal 60 already in the source is left alone.
    assert!(out.contains("vy - 0.0001*60/35;"), "{out}");
}

#[test]
fn substitutes_whatever_literal_the_caller_supplies() {
    // The caller owns this number: 35 preserves today's behaviour, the
    // deck's real rate is more honest. Hardcoding 60 (which the header
    // wrongly documents as the default) would have sped 29% of the corpus
    // up by 42%.
    let src = "per_frame_1=vy = vy - 0.0001*60/fps;\n";
    assert!(patch_preset(src, &[], 45).contains("60/45;"));
    assert!(patch_preset(src, &[], 144).contains("60/144;"));
}

#[test]
fn leaves_shader_blocks_alone() {
    let src = "warp_1=`   ret = float3(fps,0,0);\ncomp_2=`   ret *= fps;\n";
    assert_eq!(patch_preset(src, &[], MEASURED_DEFAULT_FPS), src);
}

#[test]
fn leaves_identifiers_that_merely_contain_fps_alone() {
    let src = "per_frame_1=myfps = fps2 + _fps + bass_fps + fps;\n";
    let out = patch_preset(src, &[], MEASURED_DEFAULT_FPS);
    assert!(out.contains("myfps = fps2 + _fps + bass_fps + 35;"), "{out}");
}

#[test]
fn leaves_non_equation_keys_and_section_headers_alone() {
    let src = "[preset00]\nfRating=5.000\nnWaveMode=4\n";
    assert_eq!(patch_preset(src, &[], MEASURED_DEFAULT_FPS), src);
}

// ---- appended demux / latch / application ----

#[test]
fn appends_the_demux_prologue_after_the_presets_own_equations() {
    let out = patch_preset("per_frame_1=zoom = 1.01;\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
    let own = out.find("zoom = 1.01;").unwrap();
    let demux = out.find("od_c = fps;").unwrap();
    assert!(own < demux, "{out}");
    assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
    assert!(out.contains("per_frame_3=od_g = above(od_c,9999999);"), "{out}");
    assert!(out.contains("per_frame_4=od_i = od_g * (od_c % 1000);"), "{out}");
    assert!(
        out.contains("per_frame_5=od_v = int((od_c - 10000000)/1000)/1000 - 2;"),
        "{out}"
    );
}

#[test]
fn gates_the_index_on_the_guard_so_a_raw_frame_rate_latches_nothing() {
    // The C2 regression. A freshly patched preset with zero set_param
    // calls sits at projectM's raw default (35), which without the guard
    // decoded as "write -2.0 to slot 35". The guard collapses od_i to the
    // reserved 0, and every latch line tests `equal(od_i, N)` for N >= 1,
    // so nothing fires and every slot keeps its baked-in `initial`.
    let targets = [PatchTarget {
        index: MEASURED_DEFAULT_FPS as u16,
        initial: 0.5,
        apply: Apply::Assign("q1".to_string()),
    }];
    let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
    assert!(out.contains("od_g = above(od_c,9999999);"), "{out}");
    assert!(out.contains("od_i = od_g * (od_c % 1000);"), "{out}");
    assert!(out.contains("per_frame_init_1=od_p35 = 0.5;"), "{out}");
    // No latch line may test against the reserved index 0.
    assert!(!out.contains("equal(od_i,0)"), "{out}");
}

#[test]
fn never_lets_a_target_claim_the_reserved_index_zero() {
    let targets = [PatchTarget {
        index: 0,
        initial: 1.0,
        apply: Apply::Multiply("zoom".to_string()),
    }];
    let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
    assert!(!out.contains("od_p0"), "{out}");
    assert!(out.contains("zoom = zoom * od_p1;"), "{out}");
}

#[test]
fn numbers_appended_lines_past_the_highest_existing_index() {
    // Out-of-order numbering, and a per_frame_init block that
    // outnumbers per_frame, must both land past the right maximum.
    let src = "per_frame_9=a = 1;\nper_frame_2=b = 2;\nper_frame_init_4=c = 3;\n";
    let out = patch_preset(src, &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
    assert!(out.contains("per_frame_init_5=;od_p1 = 0;"), "{out}");
    assert!(out.contains("per_frame_10=;od_c = fps;"), "{out}");
}

#[test]
fn does_not_mistake_per_frame_init_for_a_per_frame_line() {
    let out = patch_preset("per_frame_init_50=a = 1;\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
    assert!(out.contains("per_frame_1=od_c = fps;"), "{out}");
    assert!(out.contains("per_frame_init_51=;od_p1 = 0;"), "{out}");
}

#[test]
fn latches_each_target_against_its_own_index() {
    let out = patch_preset("", &[assign(3, "q3"), assign(40, "q7")], MEASURED_DEFAULT_FPS);
    assert!(
        out.contains("od_p3 = equal(od_i,3)*od_v + (1-equal(od_i,3))*od_p3;"),
        "{out}"
    );
    assert!(
        out.contains("od_p40 = equal(od_i,40)*od_v + (1-equal(od_i,40))*od_p40;"),
        "{out}"
    );
}

#[test]
fn emits_multiply_and_assign_application_lines() {
    let targets = [
        PatchTarget {
            index: 1,
            initial: 1.0,
            apply: Apply::Multiply("zoom".to_string()),
        },
        assign(2, "q1"),
    ];
    let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
    assert!(out.contains("zoom = zoom * od_p1;"), "{out}");
    assert!(out.contains("q1 = od_p2;"), "{out}");
}

#[test]
fn emits_the_scale_around_one_application_line() {
    // `zoom`/`sx`/`sy` are neutral at 1, not 0: a plain multiply would
    // make a 0 multiplier collapse the image and a 2 multiplier turn a
    // typical `zoom = 1.01` into 2.02.
    let targets = [PatchTarget {
        index: 2,
        initial: 1.0,
        apply: Apply::ScaleAroundOne("zoom".to_string()),
    }];
    let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
    assert!(out.contains("zoom = 1 + (zoom - 1) * od_p2;"), "{out}");
}

#[test]
fn seeds_and_latches_a_shared_index_exactly_once() {
    // Time's Stretch drives both `sx` and `sy` from one slider, so two
    // targets deliberately share a register. Two applications, but only
    // one seed and one latch.
    let targets = [
        PatchTarget { index: 7, initial: 1.0, apply: Apply::ScaleAroundOne("sx".to_string()) },
        PatchTarget { index: 7, initial: 1.0, apply: Apply::ScaleAroundOne("sy".to_string()) },
    ];
    let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
    assert_eq!(out.matches("od_p7 = 1;").count(), 1, "{out}");
    assert_eq!(out.matches("od_p7 = equal(od_i,7)").count(), 1, "{out}");
    assert!(out.contains("sx = 1 + (sx - 1) * od_p7;"), "{out}");
    assert!(out.contains("sy = 1 + (sy - 1) * od_p7;"), "{out}");
}

#[test]
fn keeps_appended_line_numbering_contiguous_with_a_shared_index() {
    // libprojectM stops reading `per_frame_N` at the first gap
    // (measured), so deduplicating the latch lines must not leave a hole
    // in the numbering of what follows.
    let targets = [
        PatchTarget { index: 7, initial: 1.0, apply: Apply::ScaleAroundOne("sx".to_string()) },
        PatchTarget { index: 7, initial: 1.0, apply: Apply::ScaleAroundOne("sy".to_string()) },
        PatchTarget { index: 8, initial: 1.0, apply: Apply::Multiply("wave_a".to_string()) },
    ];
    let out = patch_preset("per_frame_1=zoom = 1.01;\n", &targets, MEASURED_DEFAULT_FPS);
    assert_contiguous_equation_numbering(&out);
}

#[test]
fn keeps_appended_line_numbering_contiguous_for_plain_targets() {
    let targets = [
        PatchTarget { index: 2, initial: 1.0, apply: Apply::ScaleAroundOne("zoom".to_string()) },
        PatchTarget { index: 3, initial: 1.0, apply: Apply::Multiply("rot".to_string()) },
    ];
    let out = patch_preset(
        "per_frame_init_1=a = 0;\nper_frame_1=zoom = 1.01;\nper_frame_2=rot = 0.1;\n",
        &targets,
        MEASURED_DEFAULT_FPS,
    );
    assert_contiguous_equation_numbering(&out);
}

/// Asserts `per_frame_N` and `per_frame_init_N` both run 1..len with no
/// hole: libprojectM stops at the first missing index, so a gap
/// silently kills every line after it.
fn assert_contiguous_equation_numbering(out: &str) {
    for prefix in ["per_frame_", "per_frame_init_"] {
        let mut seen: Vec<u32> = out
            .lines()
            .filter_map(|l| l.split('=').next())
            .filter_map(|k| equation_index(k, prefix))
            .collect();
        seen.sort_unstable();
        seen.dedup();
        let expected: Vec<u32> = (1..=seen.len() as u32).collect();
        assert_eq!(seen, expected, "{prefix} numbering has a gap:\n{out}");
    }
}

#[test]
fn applies_every_target_after_every_latch() {
    let targets = [
        PatchTarget {
            index: 1,
            initial: 1.0,
            apply: Apply::Multiply("zoom".to_string()),
        },
        assign(2, "q1"),
    ];
    let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
    let last_latch = out.rfind("od_p2 = equal").unwrap();
    assert!(out.find("zoom = zoom * od_p1;").unwrap() > last_latch, "{out}");
}

#[test]
fn bakes_the_initial_value_into_per_frame_init() {
    let targets = [PatchTarget {
        index: 1,
        initial: 1.0,
        apply: Apply::Multiply("zoom".to_string()),
    }];
    // Neutral until the first side-channel write lands, so loading a preset
    // mid-set does not flash the preset's unscaled look for one frame.
    assert!(patch_preset("", &targets, MEASURED_DEFAULT_FPS).contains("od_p1 = 1;"));
}

#[test]
fn renders_fractional_initials_without_rust_float_spellings() {
    let targets = [assign(1, "q1")];
    let mut targets = targets.to_vec();
    targets[0].initial = -1.755;
    let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
    assert!(out.contains("od_p1 = -1.755;"), "{out}");
}

#[test]
fn opens_each_appended_block_with_a_statement_separator() {
    // 703 of the 9795 reference presets (7.2%) end their per_frame
    // program without a `;`, and 1162 (11.9%) end per_frame_init that
    // way. libprojectM concatenates a block's lines into one program, so
    // appending after an unterminated statement is a syntax error that
    // kills the *whole* block. Measured: a preset ending in `q1 = 0.5`
    // stopped evaluating its own equations entirely once one line was
    // appended. Exactly one leading `;` per appended block; a second one
    // on the following lines would be noise.
    let out = patch_preset(
        "per_frame_init_1=a = 1\nper_frame_1=zoom = 1.01\n",
        &[assign(1, "q1"), assign(2, "q2")],
        MEASURED_DEFAULT_FPS,
    );
    assert!(out.contains("per_frame_init_2=;od_p1 = 0;"), "{out}");
    assert!(out.contains("per_frame_init_3=od_p2 = 0;"), "{out}");
    assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
    assert!(out.contains("per_frame_3=od_g = "), "{out}");
    assert_eq!(out.matches("=;").count(), 2, "{out}");
}

#[test]
fn treats_an_all_comment_block_as_having_no_statement() {
    // 25 of the 9795 reference presets have a block whose every line is a
    // comment. Those compile to an *empty* program, so an appended `;`
    // would land first and projectM rejects the whole preset, verified
    // against real libprojectM, and silently, since a rejected load leaves
    // the deck on its previous preset. Gating on "has numbered lines"
    // instead of "has a statement" is what broke them.
    let out = patch_preset(
        "per_frame_init_1=//decay = 0.94\nper_frame_1=zoom = 1.01;\n",
        &[assign(1, "q1")],
        MEASURED_DEFAULT_FPS,
    );
    assert!(out.contains("per_frame_init_2=od_p1 = 0;"), "{out}");
    // The per_frame block does have a statement, so it still gets one.
    assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
    assert_eq!(out.matches("=;").count(), 1, "{out}");
}

#[test]
fn treats_an_all_comment_per_frame_block_as_having_no_statement() {
    let out = patch_preset(
        "per_frame_init_1=k = 1;\nper_frame_1=  // nothing to see\nper_frame_2=\t//still nothing\n",
        &[assign(1, "q1")],
        MEASURED_DEFAULT_FPS,
    );
    assert!(out.contains("per_frame_3=od_c = fps;"), "{out}");
    // The per_frame_init block does have a statement, so it still gets one.
    assert!(out.contains("per_frame_init_2=;od_p1 = 0;"), "{out}");
    assert_eq!(out.matches("=;").count(), 1, "{out}");
}

#[test]
fn a_single_target_is_enough_to_trigger_the_all_comment_case() {
    // Not a line-count effect: one target already appends a first line to
    // each block, which is all it takes.
    let out = patch_preset("per_frame_init_1=// only a comment\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
    assert!(!out.contains("=;"), "{out}");
}

#[test]
fn counts_code_before_a_trailing_comment_as_a_statement() {
    // The inverse mistake: treating any line containing `//` as a comment
    // line would drop the separator where it is genuinely required.
    let out = patch_preset("per_frame_1=zoom = 1.01 // scale it\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
    assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
}

#[test]
fn never_starts_a_block_with_the_separator() {
    // A Milkdrop program that *begins* with `;` fails to compile
    // (measured). The separator is only ever a separator. 121 presets
    // in the library have no per_frame block at all, and every preset
    // that lacks `per_frame_init` hits the same case for that block.
    let out = patch_preset("fRating=5.000\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
    assert!(out.contains("per_frame_init_1=od_p1 = 0;"), "{out}");
    assert!(out.contains("per_frame_1=od_c = fps;"), "{out}");
    assert!(!out.contains("=;"), "{out}");

    // A preset with per_frame but no per_frame_init: separator on one
    // block, not the other.
    let out = patch_preset("per_frame_1=zoom = 1.01\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
    assert!(out.contains("per_frame_init_1=od_p1 = 0;"), "{out}");
    assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
    assert_eq!(out.matches("=;").count(), 1, "{out}");
}

#[test]
fn appends_nothing_when_there_are_no_targets() {
    let out = patch_preset("per_frame_1=zoom = 1.01;\n", &[], MEASURED_DEFAULT_FPS);
    assert!(!out.contains("od_c"), "{out}");
}

#[test]
fn handles_crlf_input_normalising_it_to_lf() {
    // Plenty of .milk files in the wild are CRLF. The patched text only
    // ever goes straight to projectm_load_preset_data, never back to disk,
    // so normalising is fine; parsing it correctly is what matters.
    let out = patch_preset("fRating=5.000\r\nper_frame_1=a = fps;\r\n", &[], MEASURED_DEFAULT_FPS);
    assert_eq!(out, "fRating=5.000\nper_frame_1=a = 35;\n");
}
