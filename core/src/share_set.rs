//! Port of OpenDrop-VJ `src/lib/engine/share-set.ts` and
//! `share-set-store.svelte.ts`: the curated, machine-agnostic subset of
//! live VJ state a "share link" carries, aggregating every other ported
//! Phase-1 module's state into one struct, plus the overlay filter used to
//! decide what's safe to put in a URL.
//!
//! Adaptations for a zero-I/O, fully unit-testable `core`:
//! - `encodeSharedSet`/`decodeSharedSet` (gzip + base64url via
//!   `CompressionStream`/`Blob`/`Response`, `JSON.stringify`/`parse`) are
//!   real browser-platform I/O, not pure logic: same rationale as the
//!   IndexedDB asset I/O dropped from `overlay.rs`. Reproducing gzip without
//!   a compression crate (none is a dependency of this crate) would mean
//!   hand-rolling DEFLATE, wildly disproportionate to "pure data shaping";
//!   the whole encode/decode boundary is deferred to a later, I/O-aware
//!   crate. `bytesToBase64Url`/`base64UrlToBytes`, only used by that
//!   boundary, go with it.
//! - The TS `version: 1` field only ever mattered to `decodeSharedSet`'s
//!   runtime check against hand-forged/legacy links: with no decode
//!   boundary here it would be a field that can only ever hold one value
//!   and nothing ever reads it, so it's dropped too.
//! - `decodeSharedSet`'s other runtime shape guards: rejecting a
//!   `timeParams`/`qVarParams`/`slotComposites`/`snapshots` array of the
//!   wrong length after `JSON.parse`: have no equivalent either: with no
//!   deserialization boundary, a `SharedSet` value can only ever be built
//!   already well-typed. Fixed-size arrays (reusing `TimeParamsTuple` and
//!   `QVarParamsTuple` from `time_params.rs`/`q_vars.rs`, plus local
//!   4/8-slot arrays for `slotComposites`/`snapshots`) enforce those
//!   lengths at compile time, so a "hand-forged wrong-length link" simply
//!   isn't a constructible Rust value: the 4 corresponding TS tests have
//!   no runtime port.
//! - `filterShareableOverlays` filters by reference (`&[Overlay] ->
//!   Vec<&Overlay>`) rather than moving/cloning: `Overlay` (see
//!   `overlay.rs`) implements neither `Clone` nor `Copy`, and the original
//!   JS filters an array of live object references without copying either.
//! - `share-set-store.svelte.ts`'s Svelte `$state` reactivity is dropped:
//!   `ShareSetState` is a plain struct with a `Default` impl, same pattern
//!   as `OverlayStore` in `overlay.rs`.

use crate::beat_trigger::BeatTriggerConfig;
use crate::blend::{ColorParams, SlotComposite};
use crate::overlay::{Overlay, OverlayKind};
use crate::q_vars::QVarParamsTuple;
use crate::snapshot::Snapshot;
use crate::time_params::TimeParamsTuple;
use crate::timeline::TimelineKeyframe;

/// Which deck (if any) feeds one crossfader-routed slot: the TS source
/// defines this inline (`Array<'A' | 'B' | 'off'>`), but `show.rs` already
/// has an identical Rust `DeckBus` enum for the same concept (`Show::
/// deck_bus`), so this reuses it rather than redeclaring a second,
/// structurally-identical type (whole-branch review Finding M6).
pub use crate::show::DeckBus;

pub struct SharedSet {
    pub name: String,
    pub preset_a: String,
    pub preset_b: String,
    /// Fixed at 4, one per physical slot: same convention as
    /// `slot_composites`/`time_params` below, not a `Vec` (whole-branch
    /// review Finding M7).
    pub deck_bus: [DeckBus; 4],
    pub crossfader: f64,
    pub transition_time: f64,
    pub color_params_a: ColorParams,
    pub color_params_b: ColorParams,
    pub slot_composites: [SlotComposite; 4],
    pub time_params: TimeParamsTuple,
    pub q_var_params: QVarParamsTuple,
    pub snapshots: [Option<Snapshot>; 8],
    pub snapshot_recall_duration: f64,
    pub timeline_keyframes: Vec<TimelineKeyframe>,
    pub overlays: Vec<Overlay>,
    pub beat_trigger_a: BeatTriggerConfig,
    pub beat_trigger_b: BeatTriggerConfig,
    pub beat_sync_a: bool,
    pub beat_sync_b: bool,
    pub overlay_queue_enabled: bool,
    pub overlay_queue_trigger: BeatTriggerConfig,
}

/// Overlays referencing a local IndexedDB asset (image/video) can never fit
/// in a URL: only text overlays are shareable.
pub fn filter_shareable_overlays(overlays: &[Overlay]) -> Vec<&Overlay> {
    overlays.iter().filter(|o| o.kind == OverlayKind::Text).collect()
}

/// Extracted from `+page.svelte`: the name to embed in a share link, the
/// copy-link button's transient label, and a pending set decoded from an
/// incoming `#share=` URL awaiting confirmation.
pub struct ShareSetState {
    pub name: String,
    pub copy_label: String,
    pub pending: Option<SharedSet>,
}

impl Default for ShareSetState {
    fn default() -> Self {
        Self { name: String::new(), copy_label: "Copier le lien".to_string(), pending: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beat_trigger::default_beat_trigger_config;
    use crate::blend::{DEFAULT_COLOR_PARAMS, DEFAULT_SLOT_COMPOSITE};
    use crate::overlay::{make_overlay, OverlayPatch};
    use crate::q_vars::default_q_var_params;

    mod filter_shareable_overlays_tests {
        use super::*;

        #[test]
        fn keeps_only_text_overlays() {
            let text = make_overlay(
                "id-1".to_string(),
                "Texte".to_string(),
                OverlayPatch {
                    kind: Some(OverlayKind::Text),
                    text: Some("Hello".to_string()),
                    ..Default::default()
                },
            );
            let media = make_overlay("id-2".to_string(), "img.png".to_string(), OverlayPatch::default());
            let overlays = vec![text, media];

            let filtered = filter_shareable_overlays(&overlays);

            assert_eq!(filtered.len(), 1);
            assert_eq!(filtered[0].id, "id-1");
            assert_eq!(filtered[0].kind, OverlayKind::Text);
        }

        #[test]
        fn empty_list_returns_empty_list() {
            assert!(filter_shareable_overlays(&[]).is_empty());
        }
    }

    mod share_set_state_tests {
        use super::*;

        #[test]
        fn starts_with_an_empty_name_the_default_copy_label_and_no_pending_set() {
            let state = ShareSetState::default();
            assert_eq!(state.name, "");
            assert_eq!(state.copy_label, "Copier le lien");
            assert!(state.pending.is_none());
        }
    }

    mod shared_set_aggregation_tests {
        use super::*;
        use std::collections::HashMap;

        use crate::commands::CommandId;
        use crate::time_params::DeckTimeParams;

        #[test]
        fn a_shared_set_aggregates_one_value_from_every_ported_module() {
            let set = SharedSet {
                name: "Mon set de test".to_string(),
                preset_a: "preset-a-slug".to_string(),
                preset_b: "preset-b-slug".to_string(),
                deck_bus: [DeckBus::A, DeckBus::B, DeckBus::Off, DeckBus::Off],
                crossfader: 0.3,
                transition_time: 1.5,
                color_params_a: ColorParams { hue_rotate: 0.2, ..DEFAULT_COLOR_PARAMS },
                color_params_b: DEFAULT_COLOR_PARAMS,
                slot_composites: [DEFAULT_SLOT_COMPOSITE; 4],
                time_params: [DeckTimeParams::default(); 4],
                q_var_params: [default_q_var_params(); 4],
                snapshots: [
                    Some(Snapshot {
                        name: "Slot 0".to_string(),
                        values: HashMap::from([(CommandId::ColorHueA, 0.5)]),
                    }),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
                snapshot_recall_duration: 2.0,
                timeline_keyframes: vec![
                    TimelineKeyframe { slot: 0, time_sec: 0.0 },
                    TimelineKeyframe { slot: 0, time_sec: 5.0 },
                ],
                overlays: vec![
                    make_overlay(
                        "id-1".to_string(),
                        "Texte".to_string(),
                        OverlayPatch {
                            kind: Some(OverlayKind::Text),
                            text: Some("Hello".to_string()),
                            ..Default::default()
                        },
                    ),
                    make_overlay("id-2".to_string(), "img.png".to_string(), OverlayPatch::default()),
                ],
                beat_trigger_a: default_beat_trigger_config(),
                beat_trigger_b: default_beat_trigger_config(),
                beat_sync_a: false,
                beat_sync_b: true,
                overlay_queue_enabled: false,
                overlay_queue_trigger: default_beat_trigger_config(),
            };

            assert_eq!(set.crossfader, 0.3);
            assert_eq!(set.time_params[0].speed_mult, 1.0);
            assert!(set.snapshots[0].is_some());
            assert!(set.snapshots[1].is_none());
            assert_eq!(filter_shareable_overlays(&set.overlays).len(), 1);
        }
    }
}
