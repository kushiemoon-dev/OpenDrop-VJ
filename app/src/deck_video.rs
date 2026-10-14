use glow::HasContext;
use std::path::PathBuf;
use std::time::Instant;

/// How far a synced deck's estimated elapsed playback time
/// may diverge from its DJ deck's latest reported `/{deck}/time` before
/// the decoder is reseeked: a few hundred ms to half a second, picked by
/// feel; not user-configurable.
const RKBX_DRIFT_THRESHOLD_SECONDS: f64 = 0.35;

/// Whether estimated elapsed playback (`actual_elapsed` seconds) has
/// drifted from the DJ deck's reported `dj_time` by more than the sync
/// tolerance. Pure and unit-testable in isolation from `Instant`/thread
/// plumbing.
pub(crate) fn rkbx_drift_exceeds_threshold(dj_time: f64, actual_elapsed: f64) -> bool {
    (dj_time - actual_elapsed).abs() > RKBX_DRIFT_THRESHOLD_SECONDS
}

/// Creates a new GL texture sized/uploaded from `frame`'s own dimensions
/// and bytes, storing it (with its size) into `slot`. Used by the NDI-in
/// poll in `about_to_wait` both when no texture exists yet and when the
/// source's resolution changed (the caller deletes the old texture first in
/// that case). NDI-in sources can be any resolution, so this never assumes
/// a fixed size the way the compositor/deck textures do.
pub(crate) fn create_ndi_in_texture(
    gl: &glow::Context,
    frame: &opendrop_io::ndi::NdiFrame,
    slot: &mut Option<(glow::NativeTexture, u32, u32)>,
) {
    create_frame_texture(gl, frame.width, frame.height, &frame.data, slot);
}

/// The shared body of the above: allocate an RGBA8 texture of exactly
/// `width`x`height`, upload `data` into it, and record it (with its size)
/// in `slot`. It is factored out of `create_ndi_in_texture` so the
/// decoded-video path uploads through the exact same code rather than a
/// second copy of it; the two differ only in where the bytes came from.
pub(crate) fn create_frame_texture(
    gl: &glow::Context,
    width: u32,
    height: u32,
    data: &[u8],
    slot: &mut Option<(glow::NativeTexture, u32, u32)>,
) {
    unsafe {
        let tex = match gl.create_texture() {
            Ok(tex) => tex,
            Err(e) => {
                // Out of GPU objects: drop this frame, keep the previous texture.
                eprintln!("[app] glGenTextures failed for an incoming-frame texture: {e}");
                return;
            }
        };
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            width as i32,
            height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(data)),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        *slot = Some((tex, width, height));
    }
}

/// Allocates an empty RGBA8 texture at a fixed `w`x`h`, never resized: the
/// bootstrap-time counterpart of `create_frame_texture` for the 4 deck-video
/// textures, which are always exactly `CAPTURE_W`x`CAPTURE_H`
/// (`opendrop_io::video_capture`'s own guarantee), so there is no `Option`/
/// recreate-on-resize dance to do, unlike `video_texture`/`ndi_in_texture`.
pub(crate) fn create_empty_video_texture(gl: &glow::Context, w: u32, h: u32) -> glow::NativeTexture {
    unsafe {
        let tex = gl.create_texture().expect("glGenTextures failed for a deck-video texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w as i32,
            h as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        tex
    }
}

/// One visual deck slot's active rkbx_link sync: which DJ
/// deck it's tracking, which clip path the match loaded (so a later
/// manual reassignment away from that clip can be detected and drops
/// the sync), and the wall-clock reference point (`seek_seconds` into the
/// file at `seeked_at`) the drift check measures elapsed playback against.
/// Re-set every time a fresh match loads a clip or a drift correction
/// reseeks; the actual currently-decoding-from offset is always
/// `seek_seconds + seeked_at.elapsed()`, never tracked more precisely than
/// that, since the decoder has no true elapsed-time readback.
pub(crate) struct RkbxSyncState {
    pub(crate) dj_deck: usize,
    pub(crate) matched_clip_path: PathBuf,
    pub(crate) seek_seconds: f64,
    pub(crate) seeked_at: Instant,
}

/// What the video-capture thread should be decoding right now, given the
/// layer's state and the clip library. `None` means "nothing": the layer
/// is off, or something else is feeding it.
///
/// Precedence mirrors the web store's mutual exclusion, with NDI on top:
/// an active NDI receive already reaches the compositor through its own
/// (pre-existing) layer, so the decoder must stay out of the way rather
/// than double-driving the frame; then a live camera; then the current
/// clip. `current_clip_index` is taken modulo the library length, exactly
/// as `+page.svelte`'s `allClips[i % allClips.length]` did, so a shrunken
/// library can never index out of range.
///
/// Pure, and public to the test module, because this one function is what
/// decides every Start/Stop the app ever sends.
pub(crate) fn desired_video_input(
    video: &opendrop_core::video::VideoState,
    clips: &[crate::video_clips::VideoClip],
    ndi_active: bool,
) -> Option<opendrop_io::video_capture::VideoInput> {
    use opendrop_io::video_capture::VideoInput;
    if !video.enabled || ndi_active {
        return None;
    }
    if let Some(device) = video.live_device.as_ref() {
        return Some(VideoInput::Camera(device.clone()));
    }
    if clips.is_empty() {
        return None;
    }
    Some(VideoInput::File { path: clips[video.current_clip_index % clips.len()].path.clone(), start_seconds: 0.0 })
}

/// Reconciles one deck slot's video-capture thread against `desired`
/// (the caller passes `desired_video_input(&state.show.deck_video[slot],
/// &state.video_clips, false)`), then uploads its newest decoded frame into
/// `texture`. Mirrors the global video layer's own reconcile-then-upload
/// block (see `desired_video_input`'s call site in `about_to_wait`), but
/// simpler: `texture` was allocated once at bootstrap and is never
/// recreated, since every source is pinned to `opendrop_io::video_capture::
/// CAPTURE_W`x`CAPTURE_H`.
pub(crate) fn tick_deck_video(
    gl: &glow::Context,
    desired: Option<opendrop_io::video_capture::VideoInput>,
    tracked_input: &mut Option<opendrop_io::video_capture::VideoInput>,
    capture: &opendrop_io::video_capture::VideoCaptureHandle,
    frame_seq: &mut u64,
    texture: glow::NativeTexture,
) {
    use opendrop_io::video_capture::VideoCaptureControl;
    if desired != *tracked_input {
        let msg = match desired.clone() {
            Some(input) => VideoCaptureControl::Start(input),
            None => VideoCaptureControl::Stop,
        };
        let _ = capture.control_tx.send(msg);
        *tracked_input = desired;
        *frame_seq = 0;
    }
    if let Some(frame) = capture.latest_frame() {
        let expected_len = opendrop_io::video_capture::frame_len(frame.width, frame.height);
        if frame.seq != *frame_seq && frame.data.len() == expected_len {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    frame.width as i32,
                    frame.height as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&frame.data)),
                );
            }
            *frame_seq = frame.seq;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one function that decides every
    /// Start/Stop the video-capture thread ever receives. Everything else
    /// in the video path only mutates `Show::video`; if this is right, the
    /// decoder follows.
    mod desired_video_input_tests {
        use super::*;
        use crate::video_clips;
        use opendrop_core::video::VideoState;
        use opendrop_io::video_capture::VideoInput;

        fn clips(names: &[&str]) -> Vec<video_clips::VideoClip> {
            names
                .iter()
                .map(|n| video_clips::VideoClip {
                    key: format!("/clips/{n}.webm"),
                    name: (*n).to_string(),
                    path: PathBuf::from(format!("/clips/{n}.webm")),
                    builtin: false,
                })
                .collect()
        }

        fn enabled() -> VideoState {
            let mut state = VideoState::default();
            state.enabled = true;
            state
        }

        #[test]
        fn a_disabled_layer_wants_nothing_even_with_a_full_library() {
            assert_eq!(desired_video_input(&VideoState::default(), &clips(&["a", "b"]), false), None);
        }

        #[test]
        fn an_enabled_layer_wants_the_current_clip() {
            let mut state = enabled();
            state.current_clip_index = 1;
            assert_eq!(
                desired_video_input(&state, &clips(&["a", "b"]), false),
                Some(VideoInput::File { path: PathBuf::from("/clips/b.webm"), start_seconds: 0.0 })
            );
        }

        #[test]
        fn an_out_of_range_index_wraps_instead_of_panicking() {
            // A deletion can leave `current_clip_index` past the end; the
            // web read `allClips[i % allClips.length]` for the same reason.
            let mut state = enabled();
            state.current_clip_index = 7;
            assert_eq!(
                desired_video_input(&state, &clips(&["a", "b"]), false),
                Some(VideoInput::File { path: PathBuf::from("/clips/b.webm"), start_seconds: 0.0 })
            );
        }

        #[test]
        fn an_empty_library_wants_nothing_rather_than_a_bogus_path() {
            assert_eq!(desired_video_input(&enabled(), &[], false), None);
        }

        #[test]
        fn a_live_camera_outranks_the_clip_library() {
            let mut state = enabled();
            state.set_live_camera("/dev/video0".to_string(), "Webcam".to_string());
            assert_eq!(
                desired_video_input(&state, &clips(&["a"]), false),
                Some(VideoInput::Camera("/dev/video0".to_string()))
            );
        }

        #[test]
        fn an_active_ndi_receive_outranks_everything_and_stops_the_decoder() {
            // NDI-in already reaches the compositor through its own
            // (pre-existing) layer; decoding anything here would
            // double-drive the frame.
            let mut state = enabled();
            state.set_live_camera("/dev/video0".to_string(), "Webcam".to_string());
            assert_eq!(desired_video_input(&state, &clips(&["a"]), true), None);
        }

        #[test]
        fn advancing_the_clip_changes_the_answer_which_is_what_triggers_a_restart() {
            let library = clips(&["a", "b", "c"]);
            let mut state = enabled();
            let first = desired_video_input(&state, &library, false);
            state.current_clip_index = 2;
            let second = desired_video_input(&state, &library, false);
            assert_ne!(first, second);
            assert_eq!(second, Some(VideoInput::File { path: PathBuf::from("/clips/c.webm"), start_seconds: 0.0 }));
        }

        #[test]
        fn redrawing_the_same_clip_leaves_the_answer_identical_so_nothing_restarts() {
            // Shuffle can pick the clip already playing; `about_to_wait`
            // compares this value, so that case must be a no-op.
            let library = clips(&["a", "b"]);
            let state = enabled();
            assert_eq!(desired_video_input(&state, &library, false), desired_video_input(&state, &library, false));
        }
    }

    mod rkbx_drift_exceeds_threshold_tests {
        use super::*;

        #[test]
        fn exactly_at_the_threshold_is_not_exceeded() {
            assert!(!rkbx_drift_exceeds_threshold(10.0, 10.0 + RKBX_DRIFT_THRESHOLD_SECONDS));
        }

        #[test]
        fn just_over_the_threshold_is_exceeded() {
            assert!(rkbx_drift_exceeds_threshold(10.0, 10.0 + RKBX_DRIFT_THRESHOLD_SECONDS + 0.001));
        }

        #[test]
        fn a_negative_divergence_is_handled_the_same_as_a_positive_one() {
            assert!(rkbx_drift_exceeds_threshold(10.0, 10.0 - RKBX_DRIFT_THRESHOLD_SECONDS - 0.001));
        }
    }
}
