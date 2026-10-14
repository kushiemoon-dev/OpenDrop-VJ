use opendrop_core::blend::{should_force_normal_for_lowest_slot, DEFAULT_COLOR_PARAMS, DEFAULT_SLOT_COMPOSITE};
use opendrop_core::strobe::strobe_flash_intensity;
use opendrop_engine::deck::{self};
use std::time::{Duration, Instant};
use crate::overlays::{render_overlays, sync_overlay_textures};
use crate::AppState;
use opendrop_engine::compositor::LayerInput;

/// How long after a beat a `beat_reactive` overlay stays scaled up by its
/// `beat_scale`, the native equivalent of `beatSyncState.beat`, which the
/// JS reference sets true on each beat and clears with
/// `setTimeout(..., 80)` (`beat-tempo-actions.ts:40-43`). Same "per-frame
/// `Instant` deadline instead of an async timer" treatment as
/// `MIDI_LED_FLASH_DURATION` above, for the same reason (no async runtime
/// on this thread).
const BEAT_PULSE_DURATION: Duration = Duration::from_millis(80);

/// Composite pass: draws every visible deck layer, the video and NDI-in layers,
/// the strobe flash and the overlays into the shared composite framebuffer.
/// Returns the lowest-numbered deck that is currently visible, which the
/// timing log reports on.
pub(crate) fn composite_frame(
    state: &mut AppState,
    now: Instant,
    now_ms: f64,
    layer_inputs: &mut [LayerInput; 4],
) -> Option<usize> {
    let deck_beat_pulse = state.last_beat_at.is_some_and(|at| now.duration_since(at) < BEAT_PULSE_DURATION);
    for (i, layer_input) in layer_inputs.iter_mut().enumerate() {
        if state.show.deck_video[i].enabled {
            layer_input.color = state.show.deck_video[i].layer_color_params(deck_beat_pulse);
        }
    }

    let lowest_active = (0..deck::DECK_COUNT).find(|&i| layer_inputs[i].opacity > 0.001);
    state.compositor.begin_frame(&state.gl);
    for (i, layer_input) in layer_inputs.iter().enumerate() {
        // This used to be
        // open-coded (`lowest_active == Some(i)`), untested here;
        // now the tested `opendrop_core::blend::
        // should_force_normal_for_lowest_slot` port of `compositor.
        // ts:140`'s `shouldForceNormalForLowestSlot`.
        let force_normal = should_force_normal_for_lowest_slot(i, lowest_active);
        let deck_tex = if state.show.deck_video[i].enabled { state.deck_video_texture[i] } else { state.decks[i].texture };
        state.compositor.composite_layer(&state.gl, deck_tex, layer_input, force_normal);
    }
    // Video layer, composited over the 4 decks and under
    // the NDI-in layer below. **On top of the decks, not behind
    // them**; see `Compositor::composite_video_layer`'s doc
    // comment for why the video layer is drawn after the decks
    // instead of before them (short version: the web app's
    // compositor this ports records, in its own class header, that
    // drawing it first made it vanish whenever a deck reached full
    // opacity, which is the default at either end of the
    // crossfader).
    //
    // Gated on the capture thread's own `running`, not on merely
    // having a texture: the texture is kept across a Stop (cheap,
    // and a restart reuses it), so without this a stopped layer
    // would keep showing its last frame forever. `running` rather
    // than `video_input.is_some()` (what this app *asked* for)
    // because it also covers ffmpeg dying on its own: an
    // unreadable file, a camera unplugged mid-set, which would
    // otherwise freeze the last decoded frame on screen with no
    // way out short of a clip cut. Exactly the same guard, for
    // exactly the same reason, as the NDI-in layer's
    // `receive_active` check just below.
    //
    // `beat_pulse` is computed here rather than reusing the
    // overlay one further down only because that one is derived a
    // few lines later; both read the same `last_beat_at`.
    if state.video.latest().running {
        if let Some((tex, _, _)) = state.video_texture {
            let pulse = state.last_beat_at.is_some_and(|at| now.duration_since(at) < BEAT_PULSE_DURATION);
            state.compositor.composite_video_layer(
                &state.gl,
                tex,
                state.show.video.opacity as f32,
                state.show.video.layer_color_params(pulse),
            );
        }
    }
    // NDI-in layer, composited last, over every deck, as part of
    // this same shared pass: `render_and_swap*` later just blits
    // the resulting `color_tex` to each window, so drawing this
    // here (not per-window) is what makes it appear in both control
    // and output. `force_normal: false`: there is nothing to
    // override (see `force_normal`'s doc comment in
    // `compositor.rs`); `DEFAULT_SLOT_COMPOSITE` already carries
    // `BlendMode::Normal`, full opacity, no keying, so coercing it
    // would be a no-op. Gated on `receive_active`, not just
    // `ndi_in_texture.is_some()`: the texture is never deleted on
    // `StopReceive` (cheap to keep around for a fast reconnect), so
    // without this check a disconnected session would keep drawing
    // its last received frame forever instead of nothing.
    if state.ndi.latest().receive_active {
        if let Some((tex, _, _)) = state.ndi_in_texture {
            let ndi_in_input = LayerInput { opacity: 1.0, composite: DEFAULT_SLOT_COMPOSITE, color: DEFAULT_COLOR_PARAMS };
            state.compositor.composite_layer(&state.gl, tex, &ndi_in_input, false);
        }
    }
    // Strobe flash: drawn
    // last, on top of every deck/NDI-in layer just composited
    // above, and still inside the `begin_frame`/`end_frame`
    // bracket so it lands in `color_tex` before the readback below
    // and `blit_to_current_window` (called per-window later in
    // this tick) both read it: the same shared FBO is what makes
    // the flash show up in the control preview, the output window,
    // and NDI/v4l2 alike, with no separate wiring for any of them.
    // Absolute beats elapsed (beat_count + phase01), not phase01
    // alone: a rate < 1 (e.g. 0.25, "once every 4 beats") needs to
    // know which beat this is, not just where in the current one
    // we are (see strobe_flash_intensity's own doc comment).
    let clock_beats_abs = state.show.clock.beat_count() as f64 + state.show.clock.phase01();
    let strobe_intensity =
        strobe_flash_intensity(&state.show.strobe, clock_beats_abs, state.show.clock.bpm(), now_ms / 1000.0);
    state.compositor.render_strobe_flash(&state.gl, state.show.strobe.color, strobe_intensity);
    // Overlays: sprites and rasterized text, drawn last
    // of all: over the decks, over the NDI-in layer, and over the
    // strobe flash (see `Compositor::composite_overlay`'s doc
    // comment on that last choice), but still inside the same
    // `begin_frame`/`end_frame` bracket, for exactly the reason
    // the strobe pass is: everything downstream (`FrameReadback`
    // below, `blit_to_current_window` per window later this tick)
    // reads this one `color_tex`.
    //
    // The upload half runs first and separately: it decodes files
    // and rasterizes strings, does no drawing, and only does any
    // work at all on the frame an overlay is added or edited.
    sync_overlay_textures(
        &state.gl,
        &state.show.overlay_store,
        &state.overlay_assets,
        &mut state.overlay_textures,
    );
    let beat_pulse = state.last_beat_at.is_some_and(|at| now.duration_since(at) < BEAT_PULSE_DURATION);
    render_overlays(
        &state.gl,
        &mut state.compositor,
        &state.show.overlay_store,
        &state.overlay_textures,
        now_ms / 1000.0,
        beat_pulse,
    );
    state.compositor.end_frame(&state.gl);
    lowest_active
}
