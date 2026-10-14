use crate::AppState;
use opendrop_engine::compositor::{COMP_H, COMP_W};
use opendrop_engine::deck::{self, DECK_H, DECK_W};
use opendrop_io::spout::SpoutSlot;

/// GPU to CPU readback feeding the NDI, v4l2loopback and Spout output paths,
/// gated per consumer so an idle session never pays for a copy nobody wants.
///
/// The composite readback is needed by NDI, v4l2 or Spout; each deck readback
/// by that deck's NDI or Spout stream (v4l2 only ever pipes the composite).
/// A polled `Some(bytes)` is pushed onto each wanting consumer's channel and
/// silently dropped if nothing is receiving, the same non-blocking,
/// ignore-on-fail convention as `AudioHandle::set_device`.
pub(crate) fn feed_outputs(state: &mut AppState) {
    let composite_wanted = [state.v4l2_active, state.ndi_composite_active, state.spout_composite_active];
    if composite_wanted.contains(&true) {
        state.compositor_readback.begin_read(&state.gl);
        if let Some(bytes) = state.compositor_readback.poll(&state.gl) {
            let [v4l2, ndi, spout] = composite_wanted;
            // Hand the one copy to the last consumer and clone for the others.
            fan_out(
                bytes,
                [v4l2, ndi, spout],
                [
                    &|b| {
                        let _ = state.v4l2_frame_tx.send(b);
                    },
                    &|b| {
                        let _ = state.compositor_frame_tx.send(b);
                    },
                    &|b| state.spout.send_frame(SpoutSlot::Composite, COMP_W, COMP_H, b),
                ],
            );
        }
    }
    for i in 0..deck::DECK_COUNT {
        let wanted = [state.ndi_deck_active[i], state.spout_deck_active[i]];
        if !wanted.contains(&true) {
            continue;
        }
        state.deck_readback[i].begin_read(&state.gl);
        if let Some(bytes) = state.deck_readback[i].poll(&state.gl) {
            let [ndi, spout] = wanted;
            fan_out(
                bytes,
                [ndi, spout],
                [
                    &|b| {
                        let _ = state.deck_frame_tx[i].send(b);
                    },
                    &|b| state.spout.send_frame(SpoutSlot::Deck(i), DECK_W, DECK_H, b),
                ],
            );
        }
    }
}

/// Calls each `sinks[k]` whose `wanted[k]` is set, giving the last one the
/// original buffer and the earlier ones a clone, so a single consumer never
/// pays for a copy.
fn fan_out<const N: usize>(bytes: Vec<u8>, wanted: [bool; N], sinks: [&dyn Fn(Vec<u8>); N]) {
    let mut remaining = wanted.iter().filter(|w| **w).count();
    let mut bytes = Some(bytes);
    for (sink, wanted) in sinks.into_iter().zip(wanted) {
        if !wanted {
            continue;
        }
        remaining -= 1;
        let payload = if remaining == 0 { bytes.take() } else { bytes.clone() };
        if let Some(payload) = payload {
            sink(payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn run(wanted: [bool; 3]) -> Vec<(usize, usize)> {
        let log = RefCell::new(Vec::new());
        let s0 = |b: Vec<u8>| log.borrow_mut().push((0, b.len()));
        let s1 = |b: Vec<u8>| log.borrow_mut().push((1, b.len()));
        let s2 = |b: Vec<u8>| log.borrow_mut().push((2, b.len()));
        fan_out(vec![0; 4], wanted, [&s0, &s1, &s2]);
        log.into_inner()
    }

    #[test]
    fn only_wanted_consumers_receive_the_frame() {
        assert!(run([false, false, false]).is_empty());
        assert_eq!(run([false, true, false]), vec![(1, 4)]);
        assert_eq!(run([true, false, true]), vec![(0, 4), (2, 4)]);
        assert_eq!(run([true, true, true]), vec![(0, 4), (1, 4), (2, 4)]);
    }
}
