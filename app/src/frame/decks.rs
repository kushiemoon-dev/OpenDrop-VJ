use glutin::prelude::*;
use opendrop_engine::compositor::LayerInput;
use std::time::{Duration, Instant};
use crate::preset_load::{channel_indices, channel_values, next_param_to_push, resync_deck_q_var_watches, CHANNEL_PARAM_COUNT};
use crate::state::InvisibleMode;
use crate::AppState;

/// Culled (opacity ≤ 0.001) decks still render, just at this much lower
/// rate. They are not stopped outright, so a deck doesn't show a visible cold
/// start (projectM's per-preset warm-up/transition state going stale) the
/// moment the crossfader brings it back in. This is the `Eco` invisible-mode
/// throttle: `Pause` skips rendering the deck entirely instead,
/// and `Off` ignores this constant altogether.
const IDLE_DECK_INTERVAL: Duration = Duration::from_millis(100); // ~10fps floor

/// Renders one projectM frame per due deck.
pub(crate) fn render_decks(state: &mut AppState, now: Instant, audio: &opendrop_audio::AudioSnapshot, layer_inputs: &[LayerInput; 4]) {
    for (i, layer_input) in layer_inputs.iter().enumerate() {
        if state.show.deck_video[i].enabled {
            continue; // this slot's compositor content is a decoded clip, not a projectM preset
        }
        let visible = layer_input.opacity > 0.001;
        // See `InvisibleMode`: `Eco` is the original always-on
        // throttle, `Pause` skips rendering this deck entirely while
        // invisible (its texture keeps showing the last frame it
        // rendered), `Off` renders invisible decks at full rate.
        let should_render = match (visible, state.invisible_mode) {
            (true, _) | (false, InvisibleMode::Off) => true,
            (false, InvisibleMode::Eco) => now >= state.deck_next_render_at[i],
            (false, InvisibleMode::Pause) => false,
        };
        if !should_render {
            continue;
        }
        if let Err(e) = state.decks[i].context.make_current(&state.decks[i].surface) {
            opendrop_core::log_error!("[app] deck {i} make_current failed: {e}");
            continue;
        }
        // Applied here, not before make_current above: `set_mesh_size`
        // is an FFI call into this deck's projectM instance and must
        // run while its context is current, same as `render_frame`.
        if let Some((w, h)) = state.pending_mesh_size[i].take() {
            state.decks[i].set_mesh_size(w, h);
        }
        // Qvar watch set: the only Time/Qvar change the
        // side channel below cannot carry, because which q-vars a
        // preset overrides is compiled into its text. Before the
        // push, so a watch added this frame is already in the preset
        // when its value is sent.
        if state.show.q_var_params[i].enabled != state.deck_q_var_watches[i] {
            resync_deck_q_var_watches(state, i);
        }
        // Time and Qvar values: at most one changed value per deck per
        // frame, shared between the two families, written into the
        // running preset through the side channel, no reload, so the
        // preset's own animation state is untouched (see
        // `engine::preset_patch`). Here, not on the setters, because
        // the channel is a per-frame slot and because this is where
        // this deck's context is already current, same rule
        // `set_mesh_size` follows just above. Before `render_frame`:
        // a word written after it would not reach the frame that was
        // just drawn.
        let values = channel_values(&state.show.time_params[i], &state.show.q_var_params[i]);
        let indices = channel_indices(&state.show.q_var_params[i]);
        if let Some((param, index)) =
            next_param_to_push(&values, &indices, &state.param_last_sent[i], state.param_cursor[i])
        {
            state.decks[i].set_param(index, values[param]);
            state.param_last_sent[i][param] = values[param];
            state.param_cursor[i] = (param + 1) % CHANNEL_PARAM_COUNT;
        }
        state.decks[i].render_frame(&audio.pcm);
        if !visible && state.invisible_mode == InvisibleMode::Eco {
            state.deck_next_render_at[i] = now + IDLE_DECK_INTERVAL;
        }
    }
}
