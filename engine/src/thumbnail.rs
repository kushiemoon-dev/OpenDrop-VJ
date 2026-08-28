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
pub const THUMB_H: u32 = 108; // reprend thumbnailer.svelte.ts:23-24

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
        Ok(pixels)
    }
}
