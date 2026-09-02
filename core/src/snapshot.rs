//! Port of OpenDrop-VJ `src/lib/engine/snapshot.ts`: capture/recall of a
//! subset of live parameter state with smooth interpolation. Only the pure
//! logic (`Snapshot`, `smoothstep`, `interpolate_snapshot`) is ported here;
//! the RAF-owning `SnapshotEngine` class depends on browser timing APIs and
//! isn't unit-tested in the source either: it belongs in a later, I/O-aware
//! crate.

use std::collections::HashMap;

use crate::commands::CommandId;

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub name: String,
    pub values: HashMap<CommandId, f64>,
}

/// Ease-in-out (smoothstep). `t` is clamped to [0,1].
pub fn smoothstep(t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Lerp each key of `target` from its value in `start` toward `target`, at the
/// (already-eased) `progress01` in [0,1].
///
/// - A key present in `target` but missing from `start` starts from the
///   target value itself (`from = to`), so a recall can't jump on a value
///   that wasn't captured live.
/// - A key present in `start` but missing from `target` is skipped (the loop
///   runs over `target` only): a recall only ever drives what it captured.
pub fn interpolate_snapshot(
    start: &HashMap<CommandId, f64>,
    target: &HashMap<CommandId, f64>,
    progress01: f64,
) -> HashMap<CommandId, f64> {
    let mut out = HashMap::new();
    for (&id, &to) in target {
        let from = *start.get(&id).unwrap_or(&to);
        out.insert(id, from + (to - from) * progress01);
    }
    out
}

/// State for a snapshot recall in progress, advanced by `Show::tick_recall`
/// (via `tick_active_recall`). `elapsed_sec` accumulates from
/// caller-supplied `dt`, the same convention as `Show::tick_playlists`'s
/// `dt_ms`, rather than an absolute wall-clock timestamp: `Show` has no
/// wall clock of its own (see `Show::reseed_rng`'s doc comment).
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveRecall {
    pub slot: usize,
    pub start_values: HashMap<CommandId, f64>,
    pub elapsed_sec: f64,
}

/// Advances `active` by `dt_sec` against `target` (the values of the
/// snapshot slot being recalled) and `duration_sec`
/// (`Show::snapshot_recall_duration_sec`). Returns the eased
/// `(CommandId, value)` pairs to dispatch this tick, and the next
/// `ActiveRecall` to store: `None` once the recall has fully completed
/// (`elapsed_sec >= duration_sec`), signalling the caller to clear
/// `Show::active_recall`.
pub fn tick_active_recall(
    active: &ActiveRecall,
    target: &HashMap<CommandId, f64>,
    duration_sec: f64,
    dt_sec: f64,
) -> (HashMap<CommandId, f64>, Option<ActiveRecall>) {
    let elapsed_sec = active.elapsed_sec + dt_sec;
    let progress01 = (elapsed_sec / duration_sec).clamp(0.0, 1.0);
    let values = interpolate_snapshot(&active.start_values, target, smoothstep(progress01));
    let next = if progress01 >= 1.0 {
        None
    } else {
        Some(ActiveRecall { slot: active.slot, start_values: active.start_values.clone(), elapsed_sec })
    };
    (values, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod smoothstep_tests {
        use super::*;

        #[test]
        fn equals_0_at_t0_and_1_at_t1() {
            assert_eq!(smoothstep(0.0), 0.0);
            assert_eq!(smoothstep(1.0), 1.0);
        }

        #[test]
        fn equals_0_5_at_the_midpoint() {
            assert!((smoothstep(0.5) - 0.5).abs() < 1e-9);
        }

        #[test]
        fn is_not_linear() {
            assert!((smoothstep(0.25) - 0.15625).abs() < 1e-9);
            assert_ne!(smoothstep(0.25), 0.25);
        }

        #[test]
        fn is_symmetric() {
            assert!((smoothstep(0.25) + smoothstep(0.75) - 1.0).abs() < 1e-9);
        }

        #[test]
        fn clamps_out_of_bounds() {
            assert_eq!(smoothstep(-1.0), 0.0);
            assert_eq!(smoothstep(2.0), 1.0);
        }
    }

    mod interpolate_snapshot_tests {
        use super::*;

        const A: CommandId = CommandId::ColorHueA;
        const B: CommandId = CommandId::CompositeBlend0;

        #[test]
        fn progress_0_returns_the_starting_values() {
            let start = HashMap::from([(A, 0.0), (B, 1.0)]);
            let target = HashMap::from([(A, 1.0), (B, 0.0)]);
            let out = interpolate_snapshot(&start, &target, 0.0);
            assert_eq!(out, HashMap::from([(A, 0.0), (B, 1.0)]));
        }

        #[test]
        fn progress_1_returns_exactly_the_target() {
            let start = HashMap::from([(A, 0.0), (B, 1.0)]);
            let target = HashMap::from([(A, 1.0), (B, 0.0)]);
            let out = interpolate_snapshot(&start, &target, 1.0);
            assert_eq!(out, HashMap::from([(A, 1.0), (B, 0.0)]));
        }

        #[test]
        fn progress_0_5_midpoint_per_key() {
            let start = HashMap::from([(A, 0.0)]);
            let target = HashMap::from([(A, 1.0)]);
            let out = interpolate_snapshot(&start, &target, 0.5);
            assert_eq!(out, HashMap::from([(A, 0.5)]));
        }

        #[test]
        fn key_absent_from_start_starts_from_the_target() {
            let start = HashMap::new();
            let target = HashMap::from([(A, 0.8)]);
            let out = interpolate_snapshot(&start, &target, 0.5);
            assert_eq!(out, HashMap::from([(A, 0.8)]));
        }

        #[test]
        fn key_absent_from_target_is_ignored() {
            let start = HashMap::from([(A, 0.0), (CommandId::Crossfader, 0.2)]);
            let target = HashMap::from([(A, 1.0)]);
            let out = interpolate_snapshot(&start, &target, 0.5);
            assert_eq!(out, HashMap::from([(A, 0.5)]));
            assert!(!out.contains_key(&CommandId::Crossfader));
        }
    }

    mod tick_active_recall_tests {
        use super::*;

        const A: CommandId = CommandId::ColorHueA;

        fn recall(elapsed_sec: f64) -> ActiveRecall {
            ActiveRecall { slot: 2, start_values: HashMap::from([(A, 0.0)]), elapsed_sec }
        }

        #[test]
        fn mid_recall_returns_the_eased_value_and_keeps_the_recall_active() {
            let active = recall(0.0);
            let target = HashMap::from([(A, 1.0)]);
            let (values, next) = tick_active_recall(&active, &target, 1.0, 0.5);
            assert_eq!(values, HashMap::from([(A, smoothstep(0.5))]));
            let next = next.expect("recall not yet complete");
            assert_eq!(next.slot, 2);
            assert_eq!(next.elapsed_sec, 0.5);
        }

        #[test]
        fn reaching_the_configured_duration_returns_the_exact_target_and_clears_the_recall() {
            let active = recall(0.9);
            let target = HashMap::from([(A, 1.0)]);
            let (values, next) = tick_active_recall(&active, &target, 1.0, 0.2); // elapsed 1.1 > duration 1.0
            assert_eq!(values, HashMap::from([(A, 1.0)]));
            assert!(next.is_none());
        }

        #[test]
        fn exactly_reaching_the_duration_clears_the_recall() {
            let active = recall(0.5);
            let target = HashMap::from([(A, 1.0)]);
            let (_values, next) = tick_active_recall(&active, &target, 1.0, 0.5); // elapsed exactly 1.0
            assert!(next.is_none());
        }

        #[test]
        fn preserves_the_slot_and_start_values_across_ticks() {
            let active = recall(0.0);
            let target = HashMap::from([(A, 1.0)]);
            let (_values, next) = tick_active_recall(&active, &target, 10.0, 1.0);
            let next = next.expect("recall not yet complete");
            assert_eq!(next.slot, active.slot);
            assert_eq!(next.start_values, active.start_values);
        }
    }
}
