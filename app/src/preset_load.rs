use opendrop_engine::deck::Deck;
use opendrop_engine::qvar_patch;
use opendrop_engine::time_patch;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use crate::{preflight, AppState};

/// Single entry point for loading a preset onto a live deck, used by both
/// the preset-browser click and `Show::take_fired_presets()`
/// (keyboard navigation + playlist/beat-sync advances). Never touches a
/// deck directly: it only marks the slot pending and hands the request off
/// to `preflight::spawn_preflight`. `about_to_wait`'s verdict-handling drain
/// is the only place that actually loads a preset onto a live deck (through
/// `load_preset_onto_deck`).
pub(crate) fn request_preset_load(state: &mut AppState, slot: usize, name: String) {
    let Some(path) = should_spawn_preflight(&state.path_by_name, &mut state.pending_validations, slot, &name) else {
        return;
    };
    preflight::spawn_preflight(path, slot, name, state.preflight_tx.clone());
}

/// The dedup/backpressure guard for
/// `request_preset_load`, split out so it's testable without a real
/// `AppState` (which needs a live GL context to construct). Resolves
/// `name` to a path; the load-bearing part is that it only claims `slot` (via
/// `HashSet::insert`, marking it pending for the UI's "validating…" card)
/// and returns `Some(path)` if no validation is already in flight for that
/// slot; a slot already pending returns `None`, leaving `pending_validations`
/// untouched, so the caller never spawns a second concurrent preflight
/// child for it.
///
/// Without this guard, holding a key bound to `PresetNextActive`/
/// `PlaylistNextActive`, auto-repeating at OS key-repeat rate, ~25-30/sec,
/// spawned a brand new out-of-process `projectm_create()` validation on
/// every repeat: a couple of seconds of a held key could spawn 50-60
/// concurrent instances alongside the 4 live decks already running. This
/// also closes a stale-verdict race as a side effect: with at
/// most one in-flight validation per slot, there is never a second one
/// around to land its verdict out of order against the first. Verified by
/// grepping every `pending_validations` read/write site (`main.rs`,
/// `ui/decks.rs`): the only insert is here, the only removal is
/// `about_to_wait`'s verdict drain, once the real verdict comes back; no
/// separate Clear/Cancel path exists that could remove a slot early.
pub(crate) fn should_spawn_preflight(
    path_by_name: &HashMap<String, PathBuf>,
    pending_validations: &mut HashSet<usize>,
    slot: usize,
    name: &str,
) -> Option<PathBuf> {
    let path = path_by_name.get(name).cloned()?;
    if !pending_validations.insert(slot) {
        return None;
    }
    Some(path)
}

/// Every parameter that competes for one deck's side channel: the 8 Time
/// multipliers first (`engine::time_patch`), then the 32 Qvar watches
/// (`engine::qvar_patch`). One flat space on purpose: the channel carries a
/// single word per deck per frame for *both* families, so they have to be
/// scheduled against each other rather than each against itself.
pub(crate) const CHANNEL_PARAM_COUNT: usize = time_patch::TIME_PARAM_COUNT + qvar_patch::QVAR_WATCH_COUNT;

/// Loads `path` onto `deck` through the patched path, and resyncs the
/// caller's per-deck "already in the preset" baselines to what was just
/// baked into it.
///
/// **The only way a preset reaches a deck in this app**, on purpose. Once
/// `Deck::set_param` has been used on a deck, projectM keeps the code word in
/// that instance's `fps` across preset loads, so an unpatched preset loaded
/// afterwards reads a ~10^7 word as its own frame rate and its
/// framerate-dependent physics is silently destroyed.
/// Routing *every* load through the patched path, from the very first one at
/// bootstrap, on every deck, whether or not Time has ever been touched, makes
/// that invariant structural instead of a rule someone has to remember, and
/// removes any need to track "has this deck been modulated yet" or to call
/// `Deck::reset_param_channel`.
///
/// The `fps` literal substituted into the preset is
/// [`preset_patch::MEASURED_DEFAULT_FPS`], not this app's target frame rate.
/// 29% of the reference library divides by `fps` for framerate-independent
/// physics, and libprojectM 4.1.6 hands them 35 today regardless of how fast
/// the app actually runs; substituting the real target instead would speed
/// that 29% up by up to 42% on every deck, as a side effect of a Time-panel
/// change and even for a user who never opens the panel. Preserving what
/// presets see today is the conservative half of that choice; switching to the
/// real rate is a deliberate look change that belongs in its own step.
pub(crate) fn load_preset_onto_deck(
    deck: &Deck,
    path: &Path,
    time: &opendrop_core::time_params::DeckTimeParams,
    q_vars: &opendrop_core::q_vars::DeckQVarParams,
    last_sent: &mut [f64; CHANNEL_PARAM_COUNT],
    baked_watches: &mut [bool; opendrop_core::q_vars::Q_VAR_COUNT],
    smooth_transition: bool,
) -> Result<(), String> {
    let mut targets = time_patch::targets(time);
    targets.extend(qvar_patch::targets(q_vars));
    // Recorded before the load can fail, and unlike `last_sent` below:
    // this is "what this deck was last *asked* to hold", and its only
    // reader (`resync_deck_q_var_watches`) reloads whenever it disagrees
    // with `Show`. Updating it only on success would turn a preset projectM
    // rejects into a fresh reload attempt on every single frame.
    *baked_watches = q_vars.enabled;
    deck.load_preset_patched(
        path,
        &targets,
        opendrop_engine::preset_patch::MEASURED_DEFAULT_FPS,
        smooth_transition,
    )?;
    // Not optional, and not the same call `Deck::reset_param_channel`'s first
    // paragraph is about: the word left in the channel by the last
    // `set_param` outlives this load, and the new preset's prologue latches
    // it on its first frame, over the `initial` just baked in. Found end to
    // end against real libprojectM by removing and re-adding a q-var, which
    // is the case where the two disagree (`with_q_var_watch` zeroes the
    // value): the preset came back up holding the pre-removal value while
    // the panel read 0.00, and no push could fix it because the value the
    // push loop compares against was already correct.
    deck.reset_param_channel();
    // `patch_preset` bakes these in as the preset's starting values, so the
    // push loop must not re-send them. Only on success: a rejected load
    // leaves the deck on its previous preset, which still holds the previous
    // baseline.
    *last_sent = channel_values(time, q_vars);
    Ok(())
}

/// The current value of each of the [`CHANNEL_PARAM_COUNT`] side-channel
/// parameters, in the flat order `next_param_to_push` and
/// `AppState::param_last_sent` both index by.
pub(crate) fn channel_values(
    time: &opendrop_core::time_params::DeckTimeParams,
    q_vars: &opendrop_core::q_vars::DeckQVarParams,
) -> [f64; CHANNEL_PARAM_COUNT] {
    let time_values = time_patch::param_values(time);
    std::array::from_fn(|p| match p.checked_sub(time_patch::TIME_PARAM_COUNT) {
        None => time_values[p],
        Some(watch) => q_vars.value[watch],
    })
}

/// Where each of those parameters writes on the side channel this frame, or
/// `None` for one that cannot be pushed at all right now.
///
/// Two reasons a parameter is unpushable, and both must skip rather than
/// consume the frame's one word: Time's Speed has no reachable Milkdrop
/// variable (`engine::time_patch`), and a q-var that is not currently watched
/// has no register in the loaded preset to latch into (`engine::qvar_patch`).
pub(crate) fn channel_indices(
    q_vars: &opendrop_core::q_vars::DeckQVarParams,
) -> [Option<u16>; CHANNEL_PARAM_COUNT] {
    std::array::from_fn(|p| match p.checked_sub(time_patch::TIME_PARAM_COUNT) {
        None => time_patch::side_channel_index(p),
        Some(watch) => qvar_patch::side_channel_index(watch).filter(|_| q_vars.enabled[watch]),
    })
}

/// Picks the one parameter to push into a deck's running preset this frame:
/// the first one at or after `cursor` (wrapping) that is pushable at all and
/// whose value has drifted from what the preset already holds, paired with
/// its side-channel index.
///
/// One per deck per frame is the channel's hard capacity: it is a single
/// `projectm_set_fps` word per projectM instance per frame (see
/// `engine::preset_patch`), and Time and Qvar share it, which is why this
/// scans one flat list instead of one list each. The scan starts at a
/// rotating cursor rather than at 0 so a continuously-moving param (an LFO
/// target) cannot starve the others: with K params moving
/// at once each is refreshed every K frames, instead of the lowest-numbered
/// one taking the channel forever. A single slider dragged by hand is the
/// common case and gets the full frame rate.
pub(crate) fn next_param_to_push(
    current: &[f64; CHANNEL_PARAM_COUNT],
    indices: &[Option<u16>; CHANNEL_PARAM_COUNT],
    last_sent: &[f64; CHANNEL_PARAM_COUNT],
    cursor: usize,
) -> Option<(usize, u16)> {
    (0..CHANNEL_PARAM_COUNT)
        .map(|offset| (cursor + offset) % CHANNEL_PARAM_COUNT)
        .find_map(|param| {
            let index = indices[param]?;
            (current[param] != last_sent[param]).then_some((param, index))
        })
}

/// Re-patches and reloads deck `i`'s current preset when the set of q-vars it
/// overrides no longer matches `Show`: the one thing about Qvar that the
/// per-frame side channel cannot carry.
///
/// A watched *value* is a register the patched preset already has; the *set*
/// of watches decides which registers and which `q{n} = od_p{i};` lines exist
/// at all, and those are compiled in when the preset text is loaded. So
/// adding or removing a watch costs a reload, which re-runs the preset's
/// `per_frame_init` and restarts its own animation exactly as switching
/// preset does. That is the accepted cost of an explicit, occasional click;
/// see `engine::qvar_patch` for the alternative that was weighed against it.
///
/// Must be called with deck `i`'s GL context current. Loads hard-cut
/// (`smooth_transition: false`): the target is the preset already on the
/// deck, and cross-fading a preset with itself would only smear the
/// animation restart over a second instead of hiding it.
pub(crate) fn resync_deck_q_var_watches(state: &mut AppState, i: usize) {
    // Cloned rather than borrowed so the `&mut` fields below stay reachable;
    // this runs on a watch add/remove, not per frame.
    let Some(path) = state.path_by_name.get(&state.deck_preset_names[i]).cloned() else {
        // No known file for what this deck is showing (a preset that has
        // left the catalog since it was loaded). Record the watches as baked
        // anyway so this does not retry every frame; they take effect at the
        // deck's next real preset load.
        state.deck_q_var_watches[i] = state.show.q_var_params[i].enabled;
        return;
    };
    match load_preset_onto_deck(
        &state.decks[i],
        &path,
        &state.show.time_params[i],
        &state.show.q_var_params[i],
        &mut state.param_last_sent[i],
        &mut state.deck_q_var_watches[i],
        false,
    ) {
        Ok(()) => {
            state.preset_errors.remove(&i);
        }
        Err(e) => {
            state.preset_errors.insert(i, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The preset-load dedup/backpressure
    /// guard.
    mod should_spawn_preflight_tests {
        use super::*;

        fn catalog() -> HashMap<String, PathBuf> {
            HashMap::from([("Preset A".to_string(), PathBuf::from("/presets/a.milk"))])
        }

        #[test]
        fn first_request_for_a_slot_is_allowed_and_marks_it_pending() {
            let mut pending = HashSet::new();
            let path = should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A");
            assert_eq!(path, Some(PathBuf::from("/presets/a.milk")));
            assert!(pending.contains(&0));
        }

        #[test]
        fn a_second_request_for_the_same_in_flight_slot_is_a_no_op() {
            // Regression: holding a key bound to PresetNextActive/
            // PlaylistNextActive auto-repeats at OS rate and used to spawn
            // a fresh preflight child on every repeat.
            let mut pending = HashSet::new();
            assert!(should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A").is_some());
            assert_eq!(should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A"), None);
            assert_eq!(pending, HashSet::from([0]));
        }

        #[test]
        fn a_different_slot_is_independent() {
            let mut pending = HashSet::new();
            assert!(should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A").is_some());
            assert!(should_spawn_preflight(&catalog(), &mut pending, 1, "Preset A").is_some());
            assert_eq!(pending, HashSet::from([0, 1]));
        }

        #[test]
        fn an_unknown_preset_name_is_a_no_op_and_never_marks_the_slot_pending() {
            let mut pending = HashSet::new();
            assert_eq!(should_spawn_preflight(&catalog(), &mut pending, 0, "Nonexistent"), None);
            assert!(pending.is_empty());
        }

        #[test]
        fn once_the_slot_is_cleared_a_new_request_is_allowed_again() {
            // Mirrors about_to_wait's verdict-drain: `pending_validations.
            // remove(&slot)` runs once the real verdict comes back.
            let mut pending = HashSet::from([0]);
            pending.remove(&0);
            assert!(should_spawn_preflight(&catalog(), &mut pending, 0, "Preset A").is_some());
        }
    }

    /// Steps 8 and 9: the per-frame push loop's scheduling half: which
    /// single parameter gets the one word this deck's side channel carries
    /// this frame, across the Time multipliers and the Qvar watches that
    /// share it.
    mod next_param_to_push_tests {
        use super::*;
        use opendrop_core::q_vars::default_q_var_params;

        const N: usize = CHANNEL_PARAM_COUNT;
        /// Where the Qvar half of the flat parameter space starts.
        const Q: usize = time_patch::TIME_PARAM_COUNT;

        /// Every parameter at its neutral value: 1.0 for the 8 Time
        /// multipliers, 0.0 for the 32 q-var overrides.
        fn neutral() -> [f64; N] {
            channel_values(&Default::default(), &default_q_var_params())
        }

        /// Side-channel indices with `watches` (0-based q-vars) enabled.
        fn indices(watches: &[usize]) -> [Option<u16>; N] {
            let mut q_vars = default_q_var_params();
            for &w in watches {
                q_vars.enabled[w] = true;
            }
            channel_indices(&q_vars)
        }

        #[test]
        fn the_flat_space_is_the_8_time_params_then_the_32_watches() {
            assert_eq!(N, 40);
            assert_eq!(Q, 8);
            let values = channel_values(
                &opendrop_core::time_params::DeckTimeParams { zoom_mult: 0.5, ..Default::default() },
                &{
                    let mut q = default_q_var_params();
                    q.value[31] = -1.5;
                    q
                },
            );
            assert_eq!(values[1], 0.5);
            assert_eq!(values[Q + 31], -1.5);
        }

        #[test]
        fn nothing_to_push_when_the_preset_already_holds_every_value() {
            assert_eq!(next_param_to_push(&neutral(), &indices(&[0, 5]), &neutral(), 0), None);
        }

        #[test]
        fn pushes_a_changed_time_param_with_its_side_channel_index() {
            let mut current = neutral();
            current[2] = 0.5; // Rotation
            assert_eq!(next_param_to_push(&current, &indices(&[]), &neutral(), 0), Some((2, 3)));
        }

        #[test]
        fn pushes_a_changed_q_var_with_its_side_channel_index() {
            let mut current = neutral();
            current[Q + 6] = 1.25; // Q7
            assert_eq!(next_param_to_push(&current, &indices(&[6]), &neutral(), 0), Some((Q + 6, 15)));
        }

        #[test]
        fn never_pushes_speed_even_when_it_changed() {
            // Speed has no reachable Milkdrop variable (see
            // `engine::time_patch`); sending it would burn a frame's worth of
            // the one-word channel for nothing.
            let mut current = neutral();
            current[0] = 0.25;
            assert_eq!(next_param_to_push(&current, &indices(&[]), &neutral(), 0), None);
        }

        #[test]
        fn never_pushes_an_unwatched_q_var_even_when_it_changed() {
            // An unwatched q-var has no register in the loaded preset (see
            // `engine::qvar_patch`), so the word would latch nothing.
            let mut current = neutral();
            current[Q + 6] = 1.25;
            assert_eq!(next_param_to_push(&current, &indices(&[]), &neutral(), 0), None);
            // ...and is pushed as soon as it *is* watched.
            assert_eq!(next_param_to_push(&current, &indices(&[6]), &neutral(), 0), Some((Q + 6, 15)));
        }

        #[test]
        fn starts_the_scan_at_the_cursor_and_wraps() {
            let mut current = neutral();
            current[1] = 0.5; // Zoom
            current[6] = 0.5; // Stretch
            let ix = indices(&[]);
            assert_eq!(next_param_to_push(&current, &ix, &neutral(), 0), Some((1, 2)));
            assert_eq!(next_param_to_push(&current, &ix, &neutral(), 2), Some((6, 7)));
            // Past the last dirty param, the scan wraps back to the first.
            assert_eq!(next_param_to_push(&current, &ix, &neutral(), 7), Some((1, 2)));
        }

        #[test]
        fn round_robins_between_simultaneously_moving_params() {
            // Two params moving at once must alternate, not let the
            // lowest-numbered one hold the channel forever.
            let mut current = neutral();
            current[1] = 0.5;
            current[6] = 0.5;
            let ix = indices(&[]);
            let mut last_sent = neutral();
            let mut cursor = 0;
            let mut order = Vec::new();
            for _ in 0..4 {
                match next_param_to_push(&current, &ix, &last_sent, cursor) {
                    Some((param, _)) => {
                        order.push(param);
                        last_sent[param] = current[param];
                        cursor = (param + 1) % N;
                        // Simulate the value still moving.
                        current[param] += 0.01;
                    }
                    None => break,
                }
            }
            assert_eq!(order, vec![1, 6, 1, 6]);
        }

        #[test]
        fn a_moving_time_param_and_a_moving_q_var_take_turns() {
            // The point of one shared cursor: neither family may monopolise
            // the deck's single word while the other one is also moving.
            let mut current = neutral();
            current[1] = 0.5; // Zoom
            current[Q] = 1.0; // Q1
            let ix = indices(&[0]);
            let mut last_sent = neutral();
            let mut cursor = 0;
            let mut order = Vec::new();
            for _ in 0..4 {
                let Some((param, _)) = next_param_to_push(&current, &ix, &last_sent, cursor) else {
                    break;
                };
                order.push(param);
                last_sent[param] = current[param];
                cursor = (param + 1) % N;
                current[param] += 0.01;
            }
            assert_eq!(order, vec![1, Q, 1, Q]);
        }

        #[test]
        fn covers_every_sendable_param_when_all_of_them_move() {
            let current = [0.5; N];
            let ix = indices(&(0..qvar_patch::QVAR_WATCH_COUNT).collect::<Vec<_>>());
            let mut last_sent = neutral();
            let mut cursor = 0;
            let mut seen = Vec::new();
            while let Some((param, index)) = next_param_to_push(&current, &ix, &last_sent, cursor) {
                seen.push((param, index));
                last_sent[param] = current[param];
                cursor = (param + 1) % N;
            }
            // Every param moved; the 7 Time ones with a Milkdrop target and
            // all 32 watches each get exactly one push, in index order, and
            // Speed gets none.
            let expected: Vec<(usize, u16)> = (1..=7)
                .map(|p| (p, p as u16 + 1))
                .chain((0..qvar_patch::QVAR_WATCH_COUNT).map(|w| (Q + w, 9 + w as u16)))
                .collect();
            assert_eq!(seen, expected);
        }

        #[test]
        fn every_index_the_scheduler_can_emit_is_unique() {
            // Time and Qvar share one channel: two params mapping to the
            // same index would make one silently drive the other.
            let all = indices(&(0..qvar_patch::QVAR_WATCH_COUNT).collect::<Vec<_>>());
            let mut used: Vec<u16> = all.into_iter().flatten().collect();
            assert_eq!(used.len(), 39, "7 Time params + 32 watches");
            used.sort_unstable();
            used.dedup();
            assert_eq!(used.len(), 39);
        }
    }
}
