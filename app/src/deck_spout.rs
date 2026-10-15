//! A deck slot fed by a Spout sender: the slot's video texture, which the
//! compositor already draws in place of the preset while the slot's video is
//! on, is filled from another application's Spout output instead of from a
//! clip or a camera.
//!
//! Each slot that uses Spout gets its own receive thread (`io::spout_in`),
//! started when the slot chooses a sender and dropped when it stops. Frames
//! are stretched to the video texture's fixed size, since that texture was
//! registered with the UI once at startup and cannot change shape.

use glow::HasContext;
use opendrop_engine::deck;
use opendrop_io::spout_in::{spawn, SpoutInControl};
use opendrop_io::video_capture::{CAPTURE_H, CAPTURE_W};

use crate::frame::sources::drain_to_latest;
use crate::AppState;

/// What a slot's receiver has to do to match what the slot now wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeckSpoutAction {
    Keep,
    /// Start receiving from this sender (`None`: whichever one is active).
    Start(Option<String>),
    Stop,
}

/// `want` is the slot's `spout_source` while its video is on (`Some("")` is
/// the active sender); `started` is what the receiver was last told.
pub(crate) fn deck_spout_action(want: &Option<String>, started: &Option<String>) -> DeckSpoutAction {
    match (want, started) {
        (None, None) => DeckSpoutAction::Keep,
        (None, Some(_)) => DeckSpoutAction::Stop,
        (Some(wanted), Some(current)) if wanted == current => DeckSpoutAction::Keep,
        (Some(wanted), _) => DeckSpoutAction::Start((!wanted.is_empty()).then(|| wanted.clone())),
    }
}

/// Nearest-neighbour resize of tightly packed RGBA. `None` when a size is
/// zero or `src` is not exactly `src_w * src_h * 4` bytes.
pub(crate) fn scale_rgba_nearest(src: &[u8], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Option<Vec<u8>> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 || src.len() != src_w * src_h * 4 {
        return None;
    }
    let mut out = Vec::with_capacity(dst_w * dst_h * 4);
    for y in 0..dst_h {
        let sy = y * src_h / dst_h;
        for x in 0..dst_w {
            let sx = x * src_w / dst_w;
            let i = (sy * src_w + sx) * 4;
            out.extend_from_slice(&src[i..i + 4]);
        }
    }
    Some(out)
}

/// Brings each slot's receiver in line with its settings, then uploads the
/// newest frame of every active receiver into that slot's video texture.
pub(crate) fn sync(state: &mut AppState) {
    for slot in 0..deck::DECK_COUNT {
        let video = &state.show.deck_video[slot];
        let want = if video.enabled { video.spout_source.clone() } else { None };
        match deck_spout_action(&want, &state.deck_spout_started[slot]) {
            DeckSpoutAction::Keep => {}
            DeckSpoutAction::Start(sender) => {
                let handle = state.deck_spout[slot].get_or_insert_with(spawn);
                let _ = handle.control_tx.send(SpoutInControl::Start(sender));
                state.deck_spout_started[slot] = want;
            }
            DeckSpoutAction::Stop => {
                if let Some(handle) = state.deck_spout[slot].take() {
                    let _ = handle.control_tx.send(SpoutInControl::Stop);
                }
                state.deck_spout_started[slot] = None;
            }
        }
        let Some(handle) = &state.deck_spout[slot] else { continue };
        let Some(frame) = drain_to_latest(&handle.frame_rx) else { continue };
        let (w, h) = (CAPTURE_W as usize, CAPTURE_H as usize);
        let scaled;
        let pixels: &[u8] = if (frame.width as usize, frame.height as usize) == (w, h) {
            &frame.data
        } else {
            match scale_rgba_nearest(&frame.data, frame.width as usize, frame.height as usize, w, h) {
                Some(resized) => {
                    scaled = resized;
                    &scaled
                }
                None => continue, // a frame whose size does not match its length
            }
        };
        unsafe {
            state.gl.bind_texture(glow::TEXTURE_2D, Some(state.deck_video_texture[slot]));
            state.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(pixels)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn the_receiver_follows_what_the_slot_wants() {
        assert_eq!(deck_spout_action(&None, &None), DeckSpoutAction::Keep);
        assert_eq!(deck_spout_action(&some("Resolume"), &None), DeckSpoutAction::Start(some("Resolume")));
        assert_eq!(deck_spout_action(&some("Resolume"), &some("Resolume")), DeckSpoutAction::Keep);
        assert_eq!(deck_spout_action(&some("OBS"), &some("Resolume")), DeckSpoutAction::Start(some("OBS")), "a new sender restarts");
        assert_eq!(deck_spout_action(&None, &some("Resolume")), DeckSpoutAction::Stop);
    }

    #[test]
    fn an_empty_sender_name_means_the_active_sender() {
        assert_eq!(deck_spout_action(&some(""), &None), DeckSpoutAction::Start(None));
        assert_eq!(deck_spout_action(&some(""), &some("")), DeckSpoutAction::Keep);
        assert_eq!(deck_spout_action(&some("OBS"), &some("")), DeckSpoutAction::Start(some("OBS")));
    }

    #[test]
    fn scaling_up_repeats_pixels_and_scaling_down_picks_them() {
        // 2x1: red, blue -> 4x2: red red blue blue on both rows.
        let src = [255, 0, 0, 255, 0, 0, 255, 255];
        let up = scale_rgba_nearest(&src, 2, 1, 4, 2).unwrap();
        assert_eq!(up.len(), 4 * 2 * 4);
        let pixel = |x: usize, y: usize| &up[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
        assert_eq!(pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(1, 1), [255, 0, 0, 255]);
        assert_eq!(pixel(2, 0), [0, 0, 255, 255]);
        assert_eq!(pixel(3, 1), [0, 0, 255, 255]);

        let down = scale_rgba_nearest(&up, 4, 2, 2, 1).unwrap();
        assert_eq!(down, src.to_vec());
    }

    #[test]
    fn the_same_size_is_returned_unchanged_and_bad_input_is_rejected() {
        let src: Vec<u8> = (0..16).collect();
        assert_eq!(scale_rgba_nearest(&src, 2, 2, 2, 2).unwrap(), src);
        assert!(scale_rgba_nearest(&src, 2, 2, 0, 2).is_none());
        assert!(scale_rgba_nearest(&src, 0, 2, 2, 2).is_none());
        assert!(scale_rgba_nearest(&src[..10], 2, 2, 2, 2).is_none(), "length must match the size");
    }
}
