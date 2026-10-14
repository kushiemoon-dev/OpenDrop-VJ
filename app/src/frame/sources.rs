use glow::HasContext;
use glutin::prelude::*;
use std::sync::mpsc;
use crate::deck_video::{create_frame_texture, create_ndi_in_texture, desired_video_input};
use crate::AppState;

/// Drains `rx` completely, returning only the most recently received item
/// (or `None` if nothing was waiting). Used where a consumer only cares
/// about the latest value from an unbounded channel and polls it once per
/// tick; mirrors `io::ndi::out::drain_slot`'s "never let this back up"
/// reasoning (see that function's doc comment), applied to a `Receiver<T>`
/// a caller reads directly instead of forwarding into a sender.
pub(crate) fn drain_to_latest<T>(rx: &mpsc::Receiver<T>) -> Option<T> {
    let mut newest = None;
    while let Ok(item) = rx.try_recv() {
        newest = Some(item);
    }
    newest
}

/// Makes the main context current again, then uploads the newest NDI-in frame and
/// reconciles and uploads the global video layer, both before compositing.
pub(crate) fn upload_source_frames(state: &mut AppState, ndi_active: bool) {
    // Reacquire the main context (any of its surfaces works; the
    // composite FBO belongs to the context, not the surface) before
    // touching the compositor or either window.
    if let Err(e) = state.main_ctx.make_current(&state.control.surface) {
        eprintln!("[app] failed to reacquire main context: {e}");
    }

    // NDI-in: drains `frame_rx` to its newest frame per
    // tick, before compositing. A single `try_recv()` took exactly
    // one frame per app tick, which falls behind and can never
    // recatch against this unbounded channel if the NDI source's
    // rate ever briefly exceeds the app's tick rate; mirrors
    // `io::ndi::out.rs`'s own `drain_slot`, which loops for exactly
    // this reason. `frame_rx` only yields a frame once
    // `NdiControl::StartReceive` is active and a source is actually
    // connected (io/src/ndi/in_.rs). Most ticks this is empty. The
    // texture is sized from the frame's own width/height
    // (`NdiFrame`) rather than a fixed constant, since NDI-in
    // sources can be any resolution; a same-size frame is uploaded
    // in place, a resolution change (or first connect) recreates
    // the texture.
    if let Some(frame) = drain_to_latest(&state.ndi.frame_rx) {
        let expected_len = frame.width as usize * frame.height as usize * 4;
        if frame.data.len() != expected_len {
            eprintln!(
                "[app] dropping NDI-in frame with unexpected size: {} bytes, expected {expected_len} for {}x{}",
                frame.data.len(),
                frame.width,
                frame.height
            );
        } else if let Some((tex, w, h)) = state.ndi_in_texture {
            if (w, h) == (frame.width, frame.height) {
                unsafe {
                    state.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    state.gl.tex_sub_image_2d(
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
            } else {
                unsafe { state.gl.delete_texture(tex) };
                state.ndi_in_texture = None;
                create_ndi_in_texture(&state.gl, &frame, &mut state.ndi_in_texture);
            }
        } else {
            create_ndi_in_texture(&state.gl, &frame, &mut state.ndi_in_texture);
        }
    }

    // Video layer, the decode half. Two steps, in this
    // order and both before the composite pass below:
    //
    // 1. Reconciliation. `desired_video_input` is the single
    //    decision point for what should be playing; comparing its
    //    answer against what the capture thread was last told is
    //    what turns a clip cut, a camera toggle, an NDI connect
    //    and the panel's on/off switch into at most one
    //    Start/Stop per tick, with no message at all on the
    //    overwhelming majority of ticks, where nothing changed.
    let desired = desired_video_input(&state.show.video, &state.video_clips, ndi_active);
    if desired != state.video_input {
        use opendrop_io::video_capture::VideoCaptureControl;
        let msg = match desired.clone() {
            Some(input) => VideoCaptureControl::Start(input),
            None => VideoCaptureControl::Stop,
        };
        let _ = state.video.control_tx.send(msg);
        state.video_input = desired;
        // The old source's last frame must not linger under a new
        // one: the capture thread clears its own published frame,
        // and this is the app-side half of the same clear.
        state.video_frame_seq = 0;
    }
    // 2. Upload. The capture thread publishes latest-wins into an
    //    `ArcSwap` (not a queue; see `io::video_capture`'s module
    //    doc comment), so there is nothing to drain; the `seq`
    //    guard is what keeps a tick faster than the source's frame
    //    rate from re-uploading 3.5 MB it already has.
    if let Some(frame) = state.video.latest_frame() {
        let expected_len = frame.width as usize * frame.height as usize * 4;
        if frame.seq == state.video_frame_seq {
            // already uploaded
        } else if frame.data.len() != expected_len {
            eprintln!(
                "[app] dropping video frame with unexpected size: {} bytes, expected {expected_len} for {}x{}",
                frame.data.len(),
                frame.width,
                frame.height
            );
        } else {
            match state.video_texture {
                Some((tex, w, h)) if (w, h) == (frame.width, frame.height) => unsafe {
                    state.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    state.gl.tex_sub_image_2d(
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
                },
                slot => {
                    if let Some((tex, _, _)) = slot {
                        unsafe { state.gl.delete_texture(tex) };
                    }
                    state.video_texture = None;
                    create_frame_texture(
                        &state.gl,
                        frame.width,
                        frame.height,
                        &frame.data,
                        &mut state.video_texture,
                    );
                }
            }
            state.video_frame_seq = frame.seq;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The NDI-in drain-to-latest behavior.
    mod drain_to_latest_tests {
        use super::*;

        #[test]
        fn empty_channel_returns_none() {
            let (_tx, rx) = mpsc::channel::<i32>();
            assert_eq!(drain_to_latest(&rx), None);
        }

        #[test]
        fn keeps_only_the_most_recently_sent_item() {
            let (tx, rx) = mpsc::channel::<i32>();
            tx.send(1).unwrap();
            tx.send(2).unwrap();
            tx.send(3).unwrap();
            assert_eq!(drain_to_latest(&rx), Some(3));
            // Fully drained by the call above: nothing left for a second call.
            assert_eq!(drain_to_latest(&rx), None);
        }
    }
}
