//! Preset thumbnail format, plus the pure helpers the out-of-process
//! thumbnail renderer and its parent both have to agree on.
//!
//! The rendering itself is deliberately not here, and not in the app
//! process at all. A thumbnail is requested just by scrolling a preset tile
//! into view, and a preset that crashes projectM while its thumbnail is
//! rendering would take the whole app down with it, live decks included.
//! `app::thumbnail_child` renders one thumbnail inside a
//! `--render-thumbnail` child process instead: same isolation the
//! pre-flight check (`app::preflight`) already gives live deck loads: and
//! `app::thumbnails` drives those children from the parent.
//!
//! What has to stay shared is the wire format between the two: the fixed
//! output size, the warmup frame count, the synthetic PCM fed to
//! audio-reactive presets, and the row flip that puts `glReadPixels`'
//! output the right way up before it is written to the cache file.

/// Thumbnail size in pixels. Also the exact size of a cache-file entry,
/// `THUMB_W * THUMB_H * 4` RGBA8 bytes with no header, which is what lets
/// `app::thumbnails::read_cached` detect a truncated or stale entry with a
/// plain length check.
pub const THUMB_W: u32 = 192;
pub const THUMB_H: u32 = 108; // matches thumbnailer.svelte.ts:23-24

/// Frames rendered before the readback, so a preset is past its warm-up
/// and transition state by the time it is captured. The renderer loops
/// `0..=WARMUP_FRAMES`, so 31 frames in total.
pub const WARMUP_FRAMES: usize = 30; // thumbnailer.svelte.ts:25

/// ~10ms of synthetic stereo PCM noise at 48kHz, injected once per warmup
/// frame so an audio-reactive preset doesn't get captured on a frozen,
/// silent frame.
///
/// Deterministic on purpose: the same preset renders the same thumbnail on
/// every machine and every run, so a cache entry written by one run is
/// exactly what the next run would have produced.
pub fn synthetic_pcm() -> Vec<f32> {
    // xorshift64, same pattern as core/src/playlist.rs's next_random: no
    // rand dependency needed for this.
    let mut noise_state: u64 = 0x9E3779B97F4A7C15;
    let mut next_noise = move || {
        noise_state ^= noise_state << 13;
        noise_state ^= noise_state >> 7;
        noise_state ^= noise_state << 17;
        ((noise_state >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
    };
    (0..960).map(|_| next_noise()).collect()
}

/// Reverses the row order of a tightly packed image, `row_bytes` per row.
///
/// `glReadPixels` returns rows bottom-first, GL's lower-left origin;
/// `egui::ColorImage` (and therefore this pipeline's on-disk cache, which
/// stores exactly what the renderer produced) wants them top-first.
/// Reversed once, in the child process, before the cache file is written,
/// so both the file and the texture the parent uploads from it end up the
/// right way up and no consumer needs a compensating UV flip. egui_glow
/// does exactly this in its own `read_screen_rgba` (egui_glow 0.36.1
/// src/painter.rs:678-682).
pub fn flip_rows_vertically(pixels: &[u8], row_bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len());
    for row in pixels.chunks_exact(row_bytes).rev() {
        out.extend_from_slice(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    mod flip_rows_vertically_tests {
        use super::*;

        #[test]
        fn reverses_row_order_and_keeps_each_row_intact() {
            // 2 pixels wide, 3 rows tall, 1 byte per "pixel" for legibility.
            let pixels = [1, 1, 2, 2, 3, 3];
            assert_eq!(flip_rows_vertically(&pixels, 2), vec![3, 3, 2, 2, 1, 1]);
        }

        #[test]
        fn is_its_own_inverse() {
            let pixels: Vec<u8> = (0..24).collect();
            let once = flip_rows_vertically(&pixels, 4);
            assert_eq!(flip_rows_vertically(&once, 4), pixels);
        }

        #[test]
        fn a_single_row_image_is_unchanged() {
            let pixels = [7, 8, 9, 10];
            assert_eq!(flip_rows_vertically(&pixels, 4), pixels.to_vec());
        }
    }

    mod synthetic_pcm_tests {
        use super::*;

        #[test]
        fn is_a_whole_number_of_stereo_frames() {
            let pcm = synthetic_pcm();
            assert_eq!(pcm.len(), 960);
            assert_eq!(pcm.len() % 2, 0); // projectm_pcm_add_float takes len/2 stereo frames
        }

        #[test]
        fn stays_within_the_normalized_sample_range() {
            assert!(synthetic_pcm().iter().all(|s| (-1.0..=1.0).contains(s)));
        }

        #[test]
        fn is_deterministic_across_calls() {
            assert_eq!(synthetic_pcm(), synthetic_pcm());
        }
    }
}
