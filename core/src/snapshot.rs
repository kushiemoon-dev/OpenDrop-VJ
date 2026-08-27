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
}
