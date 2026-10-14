use glutin::prelude::*;
use opendrop_core::commands::{CommandContext, CommandId, CommandKind};
use opendrop_io::midi::MidiTriggerKey;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use crate::preset_load::{load_preset_onto_deck, request_preset_load};
use crate::{preflight, AppState};

/// How long a MIDI-triggered command's LED confirmation flash stays on
/// before `about_to_wait` pushes it back off: mirrors the JS reference's
/// `setTimeout(..., 120)` (`midi-connection-actions.ts:90`), but as a
/// per-frame `Instant` deadline check instead of an async timer, since this
/// thread has no async runtime.
const MIDI_LED_FLASH_DURATION: Duration = Duration::from_millis(120);

/// Max messages kept in `AppState::chat_log`: a bounded ring buffer,
/// oldest dropped first, so the Streaming panel's chat display can't grow
/// unbounded over a long-running session.
const CHAT_LOG_CAP: usize = 50;

/// Whether MIDI learn mode has finished for the command being learned: the
/// snapshot's current mapping entry no longer equals what it was when
/// Learn was clicked (`prev_trigger`). Deliberately compares for a
/// *change*, not merely "is now `Some`": `MidiControl::StartLearn`
/// doesn't clear the pre-existing mapping entry (`io/src/midi/control.rs`),
/// so re-learning an already-mapped command would otherwise read as
/// complete on the very next frame, before the controller was even
/// touched.
pub(crate) fn midi_learn_completed(prev_trigger: Option<&MidiTriggerKey>, current_trigger: Option<&MidiTriggerKey>) -> bool {
    current_trigger != prev_trigger
}

/// Whether a MIDI-dispatched command with no
/// known persistent state (`about_to_wait`'s `persistent_led == None`)
/// should still get a confirmation flash. Mirrors the JS reference's
/// positive test (`midi-connection-actions.ts:92`: `cmd?.kind === 'trigger'
/// && getCommandLedState(action) === null`). Only genuine `Trigger`-kind
/// commands flash. The previous Rust port flashed for "everything else",
/// which wrongly included `Range`-kind commands (faders/knobs): every
/// incoming CC from a mapped fader sent a real 3-byte MIDI LED write,
/// potentially hundreds per second, fighting a motorized fader or flooding
/// a DIN-MIDI link. `kind` is `None` for a `CommandId` the registry has no
/// entry for.
pub(crate) fn should_flash_led(kind: Option<CommandKind>) -> bool {
    kind == Some(CommandKind::Trigger)
}

/// Pushes `msg` onto `log`, then trims from the front until `log.len() <=
/// cap`: keeps only the most recent `cap` messages. Pure and
/// side-effect-free so the capping behavior is testable without a real
/// chat channel.
pub(crate) fn push_chat_message(log: &mut VecDeque<opendrop_io::chat::ChatMessage>, msg: opendrop_io::chat::ChatMessage, cap: usize) {
    log.push_back(msg);
    while log.len() > cap {
        log.pop_front();
    }
}

/// Drains every input queue once per event-loop wake: preflight verdicts, MIDI, OSC,
/// remote WebSocket, Ableton Link and chat.
pub(crate) fn drain_inputs(state: &mut AppState) {
    // Non-blocking drain so results from spawn_preflight's threads
    // never back up in the channel. This is the only place that ever
    // loads a preset onto a running deck (besides the 4-preset
    // bootstrap load). A preset only reaches here after its own
    // preflight child process has already loaded it successfully in
    // isolation.
    while let Ok((slot, name, verdict)) = state.preflight_rx.try_recv() {
        state.pending_validations.remove(&slot);
        match verdict {
            preflight::PreflightVerdict::Ok => {
                if let Some(path) = state.path_by_name.get(&name) {
                    if let Err(e) = state.decks[slot].context.make_current(&state.decks[slot].surface) {
                        state.preset_errors.insert(slot, format!("GL error: {e}"));
                    } else {
                        state.decks[slot].set_soft_cut_duration(state.transition_seconds);
                        if let Err(e) = load_preset_onto_deck(
                            &state.decks[slot],
                            path,
                            &state.show.time_params[slot],
                            &state.show.q_var_params[slot],
                            &mut state.param_last_sent[slot],
                            &mut state.deck_q_var_watches[slot],
                            state.transition_seconds > 0.0,
                        ) {
                            state.preset_errors.insert(slot, e);
                        } else {
                            state.preset_errors.remove(&slot);
                            state.preset_history.record(&name);
                                state.track_memory.record_preset(&name);
                            state.deck_preset_names[slot] = name;
                            // A slot in video mode switches back to preset mode the moment a
                            // preset actually lands on it.
                            state.show.deck_video[slot].enabled = false;
                        }
                    }
                }
            }
            preflight::PreflightVerdict::Failed(msg) => {
                state.preset_errors.insert(slot, msg);
            }
        }
    }
    for load in state.show.take_fired_presets() {
        request_preset_load(state, load.slot, load.name);
    }

    // MIDI. `midi.events` carries raw, unfiltered
    // `(CommandId, value01)` dispatches (`MidiDispatch`):
    // soft-takeover (crossfader only), LED flash vs.
    // persistent-state confirmation, and mapping/clock/hotplug sync all
    // happen here, mirroring `midi-connection-actions.ts:70-134`.
    while let Ok((id, value01)) = state.midi.events.try_recv() {
        if id == CommandId::Crossfader && !state.midi_crossfader_taken_over {
            if (value01 - state.show.get_crossfader()).abs() > 0.08 {
                continue; // soft-takeover gate: knob not yet in phase with the live value
            }
            state.midi_crossfader_taken_over = true;
        }
        state.registry.dispatch(id, value01, &mut state.show);

        // Persistent-state commands push their real on/off state
        // immediately, with no confirmation flash (mirrors the
        // `cmd?.kind === 'trigger'` guard in the JS reference; a
        // flash here would wrongly overwrite the state just pushed).
        // Every other dispatched *Trigger*-kind command gets a 120ms
        // flash instead; a `Range`-kind command (fader/knob) gets no
        // LED write at all. The previous `_ => flash` branch swept in every Range-kind
        // command too, sending a real MIDI LED write for every incoming
        // CC message from a mapped fader, potentially hundreds per
        // second, fighting a motorized fader or flooding a DIN-MIDI
        // link. See `should_flash_led`'s doc comment for the JS
        // reference this now mirrors.
        let persistent_led = match id {
            CommandId::PlaylistToggleA => Some(state.show.get_playlist_playing(opendrop_core::commands::Deck::A)),
            CommandId::PlaylistToggleB => Some(state.show.get_playlist_playing(opendrop_core::commands::Deck::B)),
            CommandId::PlaylistToggleActive => {
                let deck = state.show.get_active_deck();
                Some(state.show.get_playlist_playing(deck))
            }
            _ => None,
        };
        if let Some(on) = persistent_led {
            state.midi.push_led(id, on);
            state.midi_led_state.insert(id, on);
        } else if should_flash_led(state.registry.get(id).map(|cmd| cmd.kind)) {
            state.midi.push_led(id, true);
            state.midi_led_state.insert(id, true);
            state.midi_led_flash_off_at.insert(id, Instant::now() + MIDI_LED_FLASH_DURATION);
        }
    }

    // LED flash expiry: a per-frame `Instant` deadline check, not a
    // blocking timer (this thread has no async runtime). Swapped out
    // via `mem::take` so the `retain` closure can freely touch
    // `state.midi`/`state.midi_led_state` without aliasing the very
    // map it's iterating.
    let midi_flash_now = Instant::now();
    let mut midi_led_flash_off_at = std::mem::take(&mut state.midi_led_flash_off_at);
    midi_led_flash_off_at.retain(|&id, &mut deadline| {
        if midi_flash_now >= deadline {
            state.midi.push_led(id, false);
            state.midi_led_state.insert(id, false);
            false
        } else {
            true
        }
    });
    state.midi_led_flash_off_at = midi_led_flash_off_at;

    // Clock sync, hotplug LED replay, and learn-mode completion, all
    // diffed against the same snapshot fetch. A learning command never
    // appears on `midi.events` (the io thread consumes that message to
    // complete the learn instead of dispatching it), so learn
    // completion can only be observed via the mapping snapshot here.
    let midi_snapshot = state.midi.latest();
    let midi_new_beats = midi_snapshot.clock_beat_count.wrapping_sub(state.midi_last_beat_count);
    for _ in 0..midi_new_beats {
        state.show.clock.pulse(Some(midi_snapshot.clock_bpm));
        // Pulse-only mode (bpm unknown/0, e.g. right as the MIDI clock
        // locks on): `step()` never advances phase at bpm 0, so this
        // mirrors the audio-detector's own `if clock.bpm() == 0.0 {
        // on_beat() }` fallback a few lines below. Without it, a MIDI
        // clock beat fired while bpm is still 0 would silently never
        // call `on_beat()`.
        if state.show.clock.bpm() == 0.0 {
            state.show.on_beat();
        }
    }
    state.midi_last_beat_count = midi_snapshot.clock_beat_count;

    if midi_snapshot.hotplug_epoch != state.midi_last_hotplug_epoch {
        for (&id, &on) in &state.midi_led_state {
            state.midi.push_led(id, on);
        }
        state.midi_last_hotplug_epoch = midi_snapshot.hotplug_epoch;
        // A (re)connection may bring in a different controller (or the
        // same one moved) now out of phase with the live crossfader
        // value; mirrors the JS reference's per-connection `takenOver`
        // reset (a fresh `Set` on every `toggleMidi` call).
        state.midi_crossfader_taken_over = false;
    }

    let midi_learn_done = match &state.midi_learning {
        Some((learning_id, prev_trigger)) => {
            midi_learn_completed(prev_trigger.as_ref(), midi_snapshot.mapping.get(learning_id))
        }
        None => false,
    };
    if midi_learn_done {
        state.midi_learning = None;
    }

    // OSC. `osc.events` carries `(CommandId, value01)`
    // dispatches already filtered/clamped by the OSC thread (address
    // prefix + command-name lookup + 0..1 clamp all happen in
    // `opendrop_io::osc`, not here). No soft-takeover: unlike MIDI's
    // crossfader, OSC has no such gate in the existing app.
    while let Ok((id, value01)) = state.osc.events.try_recv() {
        state.registry.dispatch(id, value01, &mut state.show);
    }

    // Remote WS. Same shape as the OSC drain just above:
    // `remote_ws.events` carries `(CommandId, value01)` dispatches
    // already filtered (token check + command-name lookup) and
    // clamped by the remote-WS thread itself (`opendrop_io::
    // remote_ws`, not here). No soft-takeover, same as OSC.
    while let Ok((id, value01)) = state.remote_ws.events.try_recv() {
        state.registry.dispatch(id, value01, &mut state.show);
    }

    // Ableton Link. The Link thread never touches `Show`
    // itself (see `opendrop_io::link`'s module doc comment); it only
    // publishes the latest polled `(tempo, phase01)`; applying that
    // to the clock happens here, once per `about_to_wait` call, same
    // as the MIDI clock-pulse handling above. `sync_external` forces
    // both bpm and phase from Link's authoritative timeline and
    // fires a beat when phase wraps.
    #[cfg(feature = "link")]
    {
        let link_snapshot = state.link.latest();
        if link_snapshot.enabled {
            for _ in 0..state.show.clock.sync_external(link_snapshot.tempo, link_snapshot.phase01) {
                state.show.on_beat();
            }
        }
    }

    // Chat. Same
    // non-blocking drain shape as the OSC/remote-WS drains above, but
    // there's no command dispatch here: just a bounded history so
    // this unbounded `mpsc::Sender` (fed by `twitch`/`kick`) never
    // backs up, mirroring `io::ndi::out.rs`'s `drain_slot` doc comment
    // on why an undrained unbounded channel is a real leak.
    while let Ok(msg) = state.chat_events.try_recv() {
        push_chat_message(&mut state.chat_log, msg, CHAT_LOG_CAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod midi_learn_completed_tests {
        use super::*;
        use opendrop_io::midi::TriggerKind;

        fn key(number: u8) -> MidiTriggerKey {
            MidiTriggerKey { device_id: "dev".to_string(), kind: TriggerKind::Cc, channel: 1, number }
        }

        #[test]
        fn unmapped_to_freshly_mapped_is_complete() {
            assert!(midi_learn_completed(None, Some(&key(1))));
        }

        #[test]
        fn still_unmapped_is_not_complete() {
            assert!(!midi_learn_completed(None, None));
        }

        #[test]
        fn remapping_and_still_seeing_the_old_entry_is_not_complete() {
            // Regression: StartLearn doesn't clear the pre-existing mapping
            // entry, so the snapshot briefly still holds the OLD trigger
            // while the thread waits for the next MIDI message.
            let old = key(1);
            assert!(!midi_learn_completed(Some(&old), Some(&old)));
        }

        #[test]
        fn remapping_to_a_different_trigger_is_complete() {
            assert!(midi_learn_completed(Some(&key(1)), Some(&key(2))));
        }
    }

    /// The LED-flash Trigger-kind gating.
    mod should_flash_led_tests {
        use super::*;

        #[test]
        fn trigger_kind_flashes() {
            assert!(should_flash_led(Some(CommandKind::Trigger)));
        }

        #[test]
        fn range_kind_does_not_flash() {
            // Regression: the previous "not a persistent-state command ->
            // flash" logic wrongly included Range-kind commands (faders/
            // knobs), sending a real MIDI LED write for every incoming CC
            // message, potentially hundreds per second.
            assert!(!should_flash_led(Some(CommandKind::Range)));
        }

        #[test]
        fn unknown_command_does_not_flash() {
            assert!(!should_flash_led(None));
        }
    }

    /// The chat-log ring-buffer capping.
    mod push_chat_message_tests {
        use super::*;
        use opendrop_io::chat::{ChatMessage, ChatPlatform};

        fn msg(content: &str) -> ChatMessage {
            ChatMessage {
                platform: ChatPlatform::Twitch,
                user_id: "1".to_string(),
                username: "someviewer".to_string(),
                content: content.to_string(),
            }
        }

        #[test]
        fn under_cap_keeps_everything_in_order() {
            let mut log = VecDeque::new();
            push_chat_message(&mut log, msg("a"), 5);
            push_chat_message(&mut log, msg("b"), 5);
            let contents: Vec<&str> = log.iter().map(|m| m.content.as_str()).collect();
            assert_eq!(contents, vec!["a", "b"]);
        }

        #[test]
        fn over_cap_drops_the_oldest_first() {
            let mut log = VecDeque::new();
            for i in 0..5 {
                push_chat_message(&mut log, msg(&i.to_string()), 3);
            }
            let contents: Vec<&str> = log.iter().map(|m| m.content.as_str()).collect();
            assert_eq!(contents, vec!["2", "3", "4"]);
        }
    }
}
