//! A 6th EGL pbuffer context: sharing the same share group as the 4 deck
//! contexts, but never one of them: dedicated to lazy, offscreen preset
//! thumbnail rendering for the preset browser UI.
//!
//! `ThumbnailRenderer::render_thumbnail`'s `glReadPixels` is a *synchronous*
//! CPU readback. That's deliberate and is not the readback this codebase's
//! "never a synchronous readback on the render path" rule (see
//! `copy_fbo0_to_shared_texture` in `deck.rs`) is about: that rule targets
//! the future NDI/streaming output path (Phase 5, potentially 60 fps, on
//! the live per-frame render path). This renderer has its own dedicated
//! context, is invoked at most once per preset actually visited, and is
//! never called from the live per-frame render loop.

use glow::HasContext;
use glutin::config::Config;
use glutin::context::{PossiblyCurrentContext, PossiblyCurrentGlContext};
use glutin::display::{Display, GlDisplay};
use glutin::surface::{PbufferSurface, SurfaceAttributesBuilder};
use std::num::NonZeroU32;
use std::path::Path;

use crate::deck::{create_one_deck_context, Deck};

pub const THUMB_W: u32 = 192;
pub const THUMB_H: u32 = 108; // matches thumbnailer.svelte.ts:23-24

/// One dedicated deck-like context for offscreen preset thumbnail
/// rendering. Wraps a single `Deck` on its own pbuffer, sized for
/// thumbnails rather than live output.
pub struct ThumbnailRenderer {
    deck: Deck,
}

impl ThumbnailRenderer {
    pub fn new(display: &Display, config: &Config, anchor: &PossiblyCurrentContext) -> Result<Self, String> {
        let attrs = SurfaceAttributesBuilder::<PbufferSurface>::new().build(
            NonZeroU32::new(THUMB_W).expect("THUMB_W is nonzero"),
            NonZeroU32::new(THUMB_H).expect("THUMB_H is nonzero"),
        );
        let surface = unsafe { display.create_pbuffer_surface(config, &attrs) }
            .map_err(|e| format!("failed to create thumbnail pbuffer surface: {e}"))?;
        let deck = create_one_deck_context(display, config, anchor, surface, THUMB_W, THUMB_H, "thumb")?;
        Ok(Self { deck })
    }

    /// Loads `path` into this renderer's dedicated context, renders a few
    /// warmup frames with synthetic PCM noise injected as fake audio (so
    /// audio-reactive presets don't render a frozen/silent frame), then
    /// reads back the result synchronously. Makes this renderer's context
    /// current itself: the caller doesn't need to.
    ///
    /// The returned RGBA8 rows are top-first (see `flip_rows_vertically`),
    /// not the bottom-first order `glReadPixels` hands back.
    ///
    /// Nothing validates `path` before it reaches projectM here: this
    /// renderer runs in-process, so a preset that crashes projectM takes
    /// the whole app down with it. The out-of-process pre-flight check
    /// (`app::preflight`) guards live deck loads only, never this path.
    pub fn render_thumbnail(&mut self, path: &Path) -> Result<Vec<u8>, String> {
        const WARMUP_FRAMES: usize = 30; // thumbnailer.svelte.ts:25

        self.deck.context.make_current(&self.deck.surface).map_err(|e| e.to_string())?;
        self.deck.load_preset(path, false)?;

        // xorshift64, same pattern as core/src/playlist.rs's next_random:
        // no rand dependency needed for this.
        let mut noise_state: u64 = 0x9E3779B97F4A7C15;
        let mut next_noise = move || {
            noise_state ^= noise_state << 13;
            noise_state ^= noise_state >> 7;
            noise_state ^= noise_state << 17;
            ((noise_state >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
        };
        // ~10ms of synthetic stereo PCM at 48kHz.
        let pcm: Vec<f32> = (0..960).map(|_| next_noise()).collect();

        for _ in 0..=WARMUP_FRAMES {
            self.deck.render_frame(&pcm);
        }

        let mut pixels = vec![0u8; (THUMB_W * THUMB_H * 4) as usize];
        unsafe {
            self.deck.gl.read_pixels(
                0,
                0,
                THUMB_W as i32,
                THUMB_H as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
        }
        Ok(flip_rows_vertically(&pixels, (THUMB_W * 4) as usize))
    }
}

/// Reverses the row order of a tightly packed image, `row_bytes` per row.
///
/// `glReadPixels` returns rows bottom-first, GL's lower-left origin;
/// `egui::ColorImage` (and therefore this pipeline's on-disk cache, which
/// stores `render_thumbnail`'s bytes verbatim) wants them top-first.
/// Reversed once here, at the source, so both the uploaded texture and the
/// cache file end up the right way up and no consumer needs a compensating
/// UV flip. egui_glow does exactly this in its own `read_screen_rgba`
/// (egui_glow 0.36.1 src/painter.rs:678-682).
fn flip_rows_vertically(pixels: &[u8], row_bytes: usize) -> Vec<u8> {
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
}
