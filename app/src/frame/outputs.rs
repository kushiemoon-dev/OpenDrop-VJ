use opendrop_engine::deck::{self};
use crate::AppState;

/// GPU to CPU readback feeding the NDI and v4l2loopback output paths, gated per
/// consumer so an idle session never pays for a copy nobody wants.
pub(crate) fn feed_outputs(state: &mut AppState) {
    // GPU->CPU readback feeding the NDI / v4l2loopback
    // output paths: gated per consumer so an idle (or
    // partially-idle) session never pays for a copy nobody wants.
    // The composite readback is
    // needed by either `ndi_composite_active` or `v4l2_active`
    // (v4l2 only ever pipes the composite stream); each deck
    // readback is needed only by that specific deck's
    // `ndi_deck_active[i]`; v4l2 never needs a per-deck readback.
    // A polled `Some(bytes)` is just pushed onto its channel and
    // silently dropped if nothing is receiving, same non-blocking,
    // ignore-on-fail convention as `AudioHandle::set_device`.
    if state.ndi_composite_active || state.v4l2_active {
        state.compositor_readback.begin_read(&state.gl);
        if let Some(bytes) = state.compositor_readback.poll(&state.gl) {
            // Two consumers, two channels (see `v4l2_frame_tx`'s doc
            // comment); only clone when both actually want a copy,
            // otherwise hand the one copy straight to whichever
            // consumer needs it.
            match (state.v4l2_active, state.ndi_composite_active) {
                (true, true) => {
                    let _ = state.v4l2_frame_tx.send(bytes.clone());
                    let _ = state.compositor_frame_tx.send(bytes);
                }
                (true, false) => {
                    let _ = state.v4l2_frame_tx.send(bytes);
                }
                (false, true) => {
                    let _ = state.compositor_frame_tx.send(bytes);
                }
                (false, false) => {} // unreachable: the outer `if` guarantees one of the two
            }
        }
    }
    for i in 0..deck::DECK_COUNT {
        if state.ndi_deck_active[i] {
            state.deck_readback[i].begin_read(&state.gl);
            if let Some(bytes) = state.deck_readback[i].poll(&state.gl) {
                let _ = state.deck_frame_tx[i].send(bytes);
            }
        }
    }
}
