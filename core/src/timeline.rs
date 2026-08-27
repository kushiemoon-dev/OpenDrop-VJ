//! Port of OpenDrop-VJ `src/lib/engine/timeline.ts`: sequences the app's
//! existing 8 snapshot slots across a wall-clock loop.
//!
//! Only `timelineLoopDuration`/`timelineValuesAt` are ported: they're the
//! pure, unit-tested half of the source file. `TimelineEngine`, the
//! `requestAnimationFrame`-owning class, is explicitly documented in the TS
//! source as verified in a real browser rather than unit tested: same
//! precedent as `SnapshotEngine` in `snapshot.rs`, which was dropped for the
//! same reason. A caller-driven replacement belongs in a later, I/O-aware
//! crate once there's an actual behavior to drive it with.

use std::collections::HashMap;

use crate::commands::CommandId;
use crate::snapshot::{interpolate_snapshot, smoothstep, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineKeyframe {
    pub slot: usize,
    pub time_sec: f64,
}

/// Loop length in seconds: the timestamp of the last keyframe. Callers are
/// responsible for keeping `keyframes` sorted by `time_sec`. Fewer than 2
/// keyframes means there's nothing to interpolate between, so the loop has
/// no length.
pub fn timeline_loop_duration(keyframes: &[TimelineKeyframe]) -> f64 {
    if keyframes.len() < 2 {
        return 0.0;
    }
    keyframes[keyframes.len() - 1].time_sec
}

/// Resolves the interpolated "look" values at a point in time within one loop
/// cycle. `t_sec` is assumed already reduced modulo `timeline_loop_duration`
/// by the caller.
///
/// Edge cases (decided):
///  - Fewer than 2 keyframes → empty map (nothing to drive).
///  - A keyframe referencing an empty snapshot slot is treated as empty for
///    that endpoint: same absent-key semantics as `interpolate_snapshot`
///    itself (a missing key never invents a jump).
///  - `t_sec` before the first keyframe's `time_sec` → progress goes
///    negative, `smoothstep` clamps it to 0, so the first keyframe's value is
///    held until reached. No special-casing needed.
pub fn timeline_values_at(
    keyframes: &[TimelineKeyframe],
    snapshots: &[Option<Snapshot>],
    t_sec: f64,
) -> HashMap<CommandId, f64> {
    if keyframes.len() < 2 {
        return HashMap::new();
    }

    let mut i = 0;
    while i < keyframes.len() - 2 && t_sec >= keyframes[i + 1].time_sec {
        i += 1;
    }
    let from = &keyframes[i];
    let to = &keyframes[i + 1];

    let span = to.time_sec - from.time_sec;
    let progress = if span <= 0.0 {
        1.0
    } else {
        (t_sec - from.time_sec) / span
    };

    let empty = HashMap::new();
    let start_values = snapshot_values(snapshots, from.slot, &empty);
    let target_values = snapshot_values(snapshots, to.slot, &empty);

    interpolate_snapshot(start_values, target_values, smoothstep(progress))
}

fn snapshot_values<'a>(
    snapshots: &'a [Option<Snapshot>],
    slot: usize,
    empty: &'a HashMap<CommandId, f64>,
) -> &'a HashMap<CommandId, f64> {
    snapshots
        .get(slot)
        .and_then(Option::as_ref)
        .map_or(empty, |s| &s.values)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: CommandId = CommandId::ColorHueA;

    fn snap(value: f64) -> Snapshot {
        Snapshot {
            name: "s".to_string(),
            values: HashMap::from([(A, value)]),
        }
    }

    mod timeline_loop_duration_tests {
        use super::*;

        #[test]
        fn fewer_than_2_keyframes_returns_0() {
            assert_eq!(timeline_loop_duration(&[]), 0.0);
            assert_eq!(
                timeline_loop_duration(&[TimelineKeyframe {
                    slot: 0,
                    time_sec: 5.0
                }]),
                0.0
            );
        }

        #[test]
        fn returns_the_timestamp_of_the_last_keyframe() {
            let kfs = [
                TimelineKeyframe {
                    slot: 0,
                    time_sec: 0.0,
                },
                TimelineKeyframe {
                    slot: 1,
                    time_sec: 8.0,
                },
                TimelineKeyframe {
                    slot: 2,
                    time_sec: 20.0,
                },
            ];
            assert_eq!(timeline_loop_duration(&kfs), 20.0);
        }
    }

    mod timeline_values_at_tests {
        use super::*;

        fn snapshots() -> Vec<Option<Snapshot>> {
            vec![Some(snap(0.0)), Some(snap(1.0)), None]
        }

        #[test]
        fn fewer_than_2_keyframes_returns_empty() {
            let snaps = snapshots();
            assert!(timeline_values_at(&[], &snaps, 0.0).is_empty());
            assert!(timeline_values_at(
                &[TimelineKeyframe {
                    slot: 0,
                    time_sec: 0.0
                }],
                &snaps,
                0.0
            )
            .is_empty());
        }

        #[test]
        fn at_the_first_keyframe_returns_exact_values_of_the_first_slot() {
            let kfs = [
                TimelineKeyframe {
                    slot: 0,
                    time_sec: 0.0,
                },
                TimelineKeyframe {
                    slot: 1,
                    time_sec: 10.0,
                },
            ];
            let out = timeline_values_at(&kfs, &snapshots(), 0.0);
            assert_eq!(out, HashMap::from([(A, 0.0)]));
        }

        #[test]
        fn at_mid_segment_uses_smoothstep_interpolation_not_linear() {
            let kfs = [
                TimelineKeyframe {
                    slot: 0,
                    time_sec: 0.0,
                },
                TimelineKeyframe {
                    slot: 1,
                    time_sec: 10.0,
                },
            ];
            let snaps = snapshots();

            let mid = timeline_values_at(&kfs, &snaps, 5.0);
            assert!((mid[&A] - 0.5).abs() < 1e-9); // smoothstep(0.5) = 0.5, exact midpoint

            let quarter = timeline_values_at(&kfs, &snaps, 2.5);
            assert!((quarter[&A] - 0.25).abs() > 0.005); // non-linear
        }

        #[test]
        fn just_before_the_last_keyframe_is_close_to_its_value() {
            let kfs = [
                TimelineKeyframe {
                    slot: 0,
                    time_sec: 0.0,
                },
                TimelineKeyframe {
                    slot: 1,
                    time_sec: 10.0,
                },
            ];
            let out = timeline_values_at(&kfs, &snapshots(), 9.999);
            assert!((out[&A] - 1.0).abs() < 0.05);
        }

        #[test]
        fn three_keyframes_selects_the_correct_segment() {
            let kfs = [
                TimelineKeyframe {
                    slot: 0,
                    time_sec: 0.0,
                },
                TimelineKeyframe {
                    slot: 1,
                    time_sec: 10.0,
                },
                TimelineKeyframe {
                    slot: 0,
                    time_sec: 20.0,
                },
            ];
            let out = timeline_values_at(&kfs, &snapshots(), 15.0);
            assert!((out[&A] - 0.5).abs() < 1e-9); // midpoint of the 2nd segment (1->0)
        }

        #[test]
        fn empty_slot_referenced_is_treated_as_empty() {
            let kfs = [
                TimelineKeyframe {
                    slot: 0,
                    time_sec: 0.0,
                },
                TimelineKeyframe {
                    slot: 2,
                    time_sec: 10.0,
                }, // slot 2 = None
            ];
            let out = timeline_values_at(&kfs, &snapshots(), 5.0);
            assert!(out.is_empty());
        }

        #[test]
        fn t_before_the_first_keyframe_holds_the_first_value() {
            let kfs = [
                TimelineKeyframe {
                    slot: 0,
                    time_sec: 5.0,
                },
                TimelineKeyframe {
                    slot: 1,
                    time_sec: 10.0,
                },
            ];
            let out = timeline_values_at(&kfs, &snapshots(), 0.0);
            assert_eq!(out, HashMap::from([(A, 0.0)]));
        }
    }
}
