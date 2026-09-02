//! Share-link codec: `SharedSet` (`opendrop_core::share_set`) <-> a
//! copyable, URL-safe string. Port of OpenDrop-VJ's `encodeSharedSet`/
//! `decodeSharedSet` (`share-set.ts`): the JSON + gzip + base64url step
//! `core::share_set`'s own module doc comment explicitly deferred to "a
//! later, I/O-aware crate" (`core` has zero dependencies by design; this is
//! that crate, Step 13 of the Phase 8 VJ-panels plan).
//!
//! `SharedSet` and everything it's built from (`DeckBus`, `ColorParams`,
//! `SlotComposite`, `DeckTimeParams`, `DeckQVarParams`, `Snapshot`,
//! `TimelineKeyframe`, `Overlay`, `BeatTriggerConfig`, ...) don't derive
//! `Serialize`/`Deserialize` themselves, and shouldn't grow one-off impls
//! just for this: same reasoning `io::midi::mapping`'s module doc comment
//! gives for not deriving those on `CommandId`. Every wire type below is a
//! local, serde-derived mirror converted to/from its `core` counterpart at
//! this boundary, the same idiom `app::config` uses for `PanelId`/
//! `ThemeIdWire`. `Snapshot::values` (`HashMap<CommandId, f64>`) goes
//! through the same kebab-case wire-name table
//! (`command_names::command_id_name`/`parse_command_id`) `io::midi::
//! mapping` already uses for the identical reason: an unrecognized name on
//! decode is skipped, not an error, matching that module's graceful-
//! degradation convention.
//!
//! `decode_shared_set` is the symmetric inverse of `encode_shared_set`:
//! not required by this step's AC (only "generate a link" is), kept for
//! symmetry since the wire types already exist either way, consistent with
//! the panel's name ("Share", not "Export").

use std::collections::HashMap;
use std::io::{Read, Write};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use flate2::write::GzEncoder;
use flate2::{read::GzDecoder, Compression};
use serde::{Deserialize, Serialize};

use opendrop_core::beat_trigger::{BeatTriggerConfig, BeatTriggerMode};
use opendrop_core::blend::{BlendMode, ColorParams, SlotComposite};
use opendrop_core::overlay::{FontFamily, Overlay, OverlayKind};
use opendrop_core::q_vars::DeckQVarParams;
use opendrop_core::share_set::{DeckBus, SharedSet};
use opendrop_core::snapshot::Snapshot;
use opendrop_core::time_params::DeckTimeParams;
use opendrop_core::timeline::TimelineKeyframe;

use crate::command_names::{command_id_name, parse_command_id};

// --- Wire mirrors (private to this module) ------------------------------

#[derive(Serialize, Deserialize)]
enum DeckBusWire {
    A,
    B,
    Off,
}

impl From<DeckBus> for DeckBusWire {
    fn from(v: DeckBus) -> Self {
        match v {
            DeckBus::A => DeckBusWire::A,
            DeckBus::B => DeckBusWire::B,
            DeckBus::Off => DeckBusWire::Off,
        }
    }
}

impl From<DeckBusWire> for DeckBus {
    fn from(v: DeckBusWire) -> Self {
        match v {
            DeckBusWire::A => DeckBus::A,
            DeckBusWire::B => DeckBus::B,
            DeckBusWire::Off => DeckBus::Off,
        }
    }
}

#[derive(Serialize, Deserialize)]
enum BlendModeWire {
    Normal,
    Additive,
    Screen,
    Multiply,
}

impl From<BlendMode> for BlendModeWire {
    fn from(v: BlendMode) -> Self {
        match v {
            BlendMode::Normal => BlendModeWire::Normal,
            BlendMode::Additive => BlendModeWire::Additive,
            BlendMode::Screen => BlendModeWire::Screen,
            BlendMode::Multiply => BlendModeWire::Multiply,
        }
    }
}

impl From<BlendModeWire> for BlendMode {
    fn from(v: BlendModeWire) -> Self {
        match v {
            BlendModeWire::Normal => BlendMode::Normal,
            BlendModeWire::Additive => BlendMode::Additive,
            BlendModeWire::Screen => BlendMode::Screen,
            BlendModeWire::Multiply => BlendMode::Multiply,
        }
    }
}

#[derive(Serialize, Deserialize)]
enum OverlayKindWire {
    Media,
    Text,
}

impl From<OverlayKind> for OverlayKindWire {
    fn from(v: OverlayKind) -> Self {
        match v {
            OverlayKind::Media => OverlayKindWire::Media,
            OverlayKind::Text => OverlayKindWire::Text,
        }
    }
}

impl From<OverlayKindWire> for OverlayKind {
    fn from(v: OverlayKindWire) -> Self {
        match v {
            OverlayKindWire::Media => OverlayKind::Media,
            OverlayKindWire::Text => OverlayKind::Text,
        }
    }
}

#[derive(Serialize, Deserialize)]
enum FontFamilyWire {
    Sans,
    Serif,
    Mono,
    Impact,
    Comic,
}

impl From<FontFamily> for FontFamilyWire {
    fn from(v: FontFamily) -> Self {
        match v {
            FontFamily::Sans => FontFamilyWire::Sans,
            FontFamily::Serif => FontFamilyWire::Serif,
            FontFamily::Mono => FontFamilyWire::Mono,
            FontFamily::Impact => FontFamilyWire::Impact,
            FontFamily::Comic => FontFamilyWire::Comic,
        }
    }
}

impl From<FontFamilyWire> for FontFamily {
    fn from(v: FontFamilyWire) -> Self {
        match v {
            FontFamilyWire::Sans => FontFamily::Sans,
            FontFamilyWire::Serif => FontFamily::Serif,
            FontFamilyWire::Mono => FontFamily::Mono,
            FontFamilyWire::Impact => FontFamily::Impact,
            FontFamilyWire::Comic => FontFamily::Comic,
        }
    }
}

#[derive(Serialize, Deserialize)]
enum BeatTriggerModeWire {
    Beat,
    VolumePeak,
}

impl From<BeatTriggerMode> for BeatTriggerModeWire {
    fn from(v: BeatTriggerMode) -> Self {
        match v {
            BeatTriggerMode::Beat => BeatTriggerModeWire::Beat,
            BeatTriggerMode::VolumePeak => BeatTriggerModeWire::VolumePeak,
        }
    }
}

impl From<BeatTriggerModeWire> for BeatTriggerMode {
    fn from(v: BeatTriggerModeWire) -> Self {
        match v {
            BeatTriggerModeWire::Beat => BeatTriggerMode::Beat,
            BeatTriggerModeWire::VolumePeak => BeatTriggerMode::VolumePeak,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ColorParamsWire {
    hue_rotate: f64,
    saturate: f64,
    brightness: f64,
    contrast: f64,
    invert: f64,
}

impl From<ColorParams> for ColorParamsWire {
    fn from(p: ColorParams) -> Self {
        Self { hue_rotate: p.hue_rotate, saturate: p.saturate, brightness: p.brightness, contrast: p.contrast, invert: p.invert }
    }
}

impl From<ColorParamsWire> for ColorParams {
    fn from(w: ColorParamsWire) -> Self {
        Self { hue_rotate: w.hue_rotate, saturate: w.saturate, brightness: w.brightness, contrast: w.contrast, invert: w.invert }
    }
}

#[derive(Serialize, Deserialize)]
struct SlotCompositeWire {
    blend: BlendModeWire,
    luma_key: bool,
    luma_black: f64,
    luma_white: f64,
    color_key: bool,
    color_hue: f64,
    color_tol: f64,
}

impl From<SlotComposite> for SlotCompositeWire {
    fn from(s: SlotComposite) -> Self {
        Self {
            blend: s.blend.into(),
            luma_key: s.luma_key,
            luma_black: s.luma_black,
            luma_white: s.luma_white,
            color_key: s.color_key,
            color_hue: s.color_hue,
            color_tol: s.color_tol,
        }
    }
}

impl From<SlotCompositeWire> for SlotComposite {
    fn from(w: SlotCompositeWire) -> Self {
        Self {
            blend: w.blend.into(),
            luma_key: w.luma_key,
            luma_black: w.luma_black,
            luma_white: w.luma_white,
            color_key: w.color_key,
            color_hue: w.color_hue,
            color_tol: w.color_tol,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DeckTimeParamsWire {
    speed_mult: f64,
    zoom_mult: f64,
    rot_mult: f64,
    warp_mult: f64,
    dx_mult: f64,
    dy_mult: f64,
    stretch_mult: f64,
    wave_mult: f64,
}

impl From<DeckTimeParams> for DeckTimeParamsWire {
    fn from(p: DeckTimeParams) -> Self {
        Self {
            speed_mult: p.speed_mult,
            zoom_mult: p.zoom_mult,
            rot_mult: p.rot_mult,
            warp_mult: p.warp_mult,
            dx_mult: p.dx_mult,
            dy_mult: p.dy_mult,
            stretch_mult: p.stretch_mult,
            wave_mult: p.wave_mult,
        }
    }
}

impl From<DeckTimeParamsWire> for DeckTimeParams {
    fn from(w: DeckTimeParamsWire) -> Self {
        Self {
            speed_mult: w.speed_mult,
            zoom_mult: w.zoom_mult,
            rot_mult: w.rot_mult,
            warp_mult: w.warp_mult,
            dx_mult: w.dx_mult,
            dy_mult: w.dy_mult,
            stretch_mult: w.stretch_mult,
            wave_mult: w.wave_mult,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DeckQVarParamsWire {
    enabled: [bool; 32],
    value: [f64; 32],
}

impl From<DeckQVarParams> for DeckQVarParamsWire {
    fn from(p: DeckQVarParams) -> Self {
        Self { enabled: p.enabled, value: p.value }
    }
}

impl From<DeckQVarParamsWire> for DeckQVarParams {
    fn from(w: DeckQVarParamsWire) -> Self {
        Self { enabled: w.enabled, value: w.value }
    }
}

#[derive(Serialize, Deserialize)]
struct SnapshotWire {
    name: String,
    /// Keyed by the same kebab-case wire name OSC/remote-WS/MIDI-mapping
    /// already use (`command_names::command_id_name`), not a derived
    /// `CommandId` serialization: see this module's doc comment. An
    /// unrecognized name on decode is skipped, not fatal.
    values: HashMap<String, f64>,
}

impl From<&Snapshot> for SnapshotWire {
    fn from(s: &Snapshot) -> Self {
        Self { name: s.name.clone(), values: s.values.iter().map(|(&id, &v)| (command_id_name(id).to_string(), v)).collect() }
    }
}

impl From<SnapshotWire> for Snapshot {
    fn from(w: SnapshotWire) -> Self {
        Self { name: w.name, values: w.values.into_iter().filter_map(|(name, v)| parse_command_id(&name).map(|id| (id, v))).collect() }
    }
}

#[derive(Serialize, Deserialize)]
struct TimelineKeyframeWire {
    slot: usize,
    time_sec: f64,
}

impl From<TimelineKeyframe> for TimelineKeyframeWire {
    fn from(k: TimelineKeyframe) -> Self {
        Self { slot: k.slot, time_sec: k.time_sec }
    }
}

impl From<TimelineKeyframeWire> for TimelineKeyframe {
    fn from(w: TimelineKeyframeWire) -> Self {
        Self { slot: w.slot, time_sec: w.time_sec }
    }
}

#[derive(Serialize, Deserialize)]
struct OverlayWire {
    id: String,
    name: String,
    x: f64,
    y: f64,
    scale: f64,
    rotation: f64,
    opacity: f64,
    blend_mode: String,
    beat_reactive: bool,
    beat_scale: f64,
    video: bool,
    spin: f64,
    drift_x: f64,
    drift_y: f64,
    kind: OverlayKindWire,
    text: String,
    font_family: FontFamilyWire,
    font_size: f64,
    color: String,
    in_queue: bool,
}

impl From<&Overlay> for OverlayWire {
    fn from(o: &Overlay) -> Self {
        Self {
            id: o.id.clone(),
            name: o.name.clone(),
            x: o.x,
            y: o.y,
            scale: o.scale,
            rotation: o.rotation,
            opacity: o.opacity,
            blend_mode: o.blend_mode.clone(),
            beat_reactive: o.beat_reactive,
            beat_scale: o.beat_scale,
            video: o.video,
            spin: o.spin,
            drift_x: o.drift_x,
            drift_y: o.drift_y,
            kind: o.kind.into(),
            text: o.text.clone(),
            font_family: o.font_family.into(),
            font_size: o.font_size,
            color: o.color.clone(),
            in_queue: o.in_queue,
        }
    }
}

impl From<OverlayWire> for Overlay {
    fn from(w: OverlayWire) -> Self {
        Self {
            id: w.id,
            name: w.name,
            x: w.x,
            y: w.y,
            scale: w.scale,
            rotation: w.rotation,
            opacity: w.opacity,
            blend_mode: w.blend_mode,
            beat_reactive: w.beat_reactive,
            beat_scale: w.beat_scale,
            video: w.video,
            spin: w.spin,
            drift_x: w.drift_x,
            drift_y: w.drift_y,
            kind: w.kind.into(),
            text: w.text,
            font_family: w.font_family.into(),
            font_size: w.font_size,
            color: w.color,
            in_queue: w.in_queue,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct BeatTriggerConfigWire {
    mode: BeatTriggerModeWire,
    beats_per_change: u32,
    offset: u32,
    sensitivity: f64,
}

impl From<BeatTriggerConfig> for BeatTriggerConfigWire {
    fn from(c: BeatTriggerConfig) -> Self {
        Self { mode: c.mode.into(), beats_per_change: c.beats_per_change, offset: c.offset, sensitivity: c.sensitivity }
    }
}

impl From<BeatTriggerConfigWire> for BeatTriggerConfig {
    fn from(w: BeatTriggerConfigWire) -> Self {
        Self { mode: w.mode.into(), beats_per_change: w.beats_per_change, offset: w.offset, sensitivity: w.sensitivity }
    }
}

#[derive(Serialize, Deserialize)]
struct SharedSetWire {
    name: String,
    preset_a: String,
    preset_b: String,
    deck_bus: [DeckBusWire; 4],
    crossfader: f64,
    transition_time: f64,
    color_params_a: ColorParamsWire,
    color_params_b: ColorParamsWire,
    slot_composites: [SlotCompositeWire; 4],
    time_params: [DeckTimeParamsWire; 4],
    q_var_params: [DeckQVarParamsWire; 4],
    snapshots: [Option<SnapshotWire>; 8],
    snapshot_recall_duration: f64,
    timeline_keyframes: Vec<TimelineKeyframeWire>,
    overlays: Vec<OverlayWire>,
    beat_trigger_a: BeatTriggerConfigWire,
    beat_trigger_b: BeatTriggerConfigWire,
    beat_sync_a: bool,
    beat_sync_b: bool,
    overlay_queue_enabled: bool,
    overlay_queue_trigger: BeatTriggerConfigWire,
}

impl From<&SharedSet> for SharedSetWire {
    fn from(s: &SharedSet) -> Self {
        Self {
            name: s.name.clone(),
            preset_a: s.preset_a.clone(),
            preset_b: s.preset_b.clone(),
            deck_bus: s.deck_bus.map(Into::into),
            crossfader: s.crossfader,
            transition_time: s.transition_time,
            color_params_a: s.color_params_a.into(),
            color_params_b: s.color_params_b.into(),
            slot_composites: s.slot_composites.map(Into::into),
            time_params: s.time_params.map(Into::into),
            q_var_params: s.q_var_params.map(Into::into),
            snapshots: std::array::from_fn(|i| s.snapshots[i].as_ref().map(SnapshotWire::from)),
            snapshot_recall_duration: s.snapshot_recall_duration,
            timeline_keyframes: s.timeline_keyframes.iter().copied().map(Into::into).collect(),
            overlays: s.overlays.iter().map(OverlayWire::from).collect(),
            beat_trigger_a: s.beat_trigger_a.into(),
            beat_trigger_b: s.beat_trigger_b.into(),
            beat_sync_a: s.beat_sync_a,
            beat_sync_b: s.beat_sync_b,
            overlay_queue_enabled: s.overlay_queue_enabled,
            overlay_queue_trigger: s.overlay_queue_trigger.into(),
        }
    }
}

impl From<SharedSetWire> for SharedSet {
    fn from(w: SharedSetWire) -> Self {
        Self {
            name: w.name,
            preset_a: w.preset_a,
            preset_b: w.preset_b,
            deck_bus: w.deck_bus.map(Into::into),
            crossfader: w.crossfader,
            transition_time: w.transition_time,
            color_params_a: w.color_params_a.into(),
            color_params_b: w.color_params_b.into(),
            slot_composites: w.slot_composites.map(Into::into),
            time_params: w.time_params.map(Into::into),
            q_var_params: w.q_var_params.map(Into::into),
            snapshots: w.snapshots.map(|opt| opt.map(Snapshot::from)),
            snapshot_recall_duration: w.snapshot_recall_duration,
            timeline_keyframes: w.timeline_keyframes.into_iter().map(Into::into).collect(),
            overlays: w.overlays.into_iter().map(Into::into).collect(),
            beat_trigger_a: w.beat_trigger_a.into(),
            beat_trigger_b: w.beat_trigger_b.into(),
            beat_sync_a: w.beat_sync_a,
            beat_sync_b: w.beat_sync_b,
            overlay_queue_enabled: w.overlay_queue_enabled,
            overlay_queue_trigger: w.overlay_queue_trigger.into(),
        }
    }
}

// --- Encode / decode ------------------------------------------------------

/// `SharedSet` -> a URL-safe, copyable link fragment: JSON (via the wire
/// types above) -> gzip -> base64url, no padding.
pub fn encode_shared_set(set: &SharedSet) -> Result<String, String> {
    let wire = SharedSetWire::from(set);
    let json = serde_json::to_vec(&wire).map_err(|e| format!("serializing SharedSet: {e}"))?;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&json).map_err(|e| format!("gzip-compressing SharedSet: {e}"))?;
    let compressed = encoder.finish().map_err(|e| format!("finishing gzip stream: {e}"))?;

    Ok(URL_SAFE_NO_PAD.encode(compressed))
}

/// The inverse of [`encode_shared_set`]. Not required by this step's AC
/// (only "generate a link" is), kept for symmetry: see this module's doc
/// comment.
pub fn decode_shared_set(encoded: &str) -> Result<SharedSet, String> {
    let compressed = URL_SAFE_NO_PAD.decode(encoded).map_err(|e| format!("base64url-decoding share link: {e}"))?;

    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut json = Vec::new();
    decoder.read_to_end(&mut json).map_err(|e| format!("gzip-decompressing share link: {e}"))?;

    let wire: SharedSetWire = serde_json::from_slice(&json).map_err(|e| format!("parsing SharedSet JSON: {e}"))?;
    Ok(wire.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrop_core::beat_trigger::default_beat_trigger_config;
    use opendrop_core::blend::{DEFAULT_COLOR_PARAMS, DEFAULT_SLOT_COMPOSITE};
    use opendrop_core::commands::CommandId;
    use opendrop_core::overlay::{make_overlay, OverlayPatch};
    use opendrop_core::q_vars::default_q_var_params;
    use opendrop_core::time_params::DeckTimeParams;

    fn sample_shared_set() -> SharedSet {
        SharedSet {
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
                Some(Snapshot { name: "Slot 0".to_string(), values: HashMap::from([(CommandId::ColorHueA, 0.5)]) }),
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
            overlays: vec![make_overlay(
                "id-1".to_string(),
                "Texte".to_string(),
                OverlayPatch { kind: Some(OverlayKind::Text), text: Some("Hello".to_string()), ..Default::default() },
            )],
            beat_trigger_a: default_beat_trigger_config(),
            beat_trigger_b: default_beat_trigger_config(),
            beat_sync_a: false,
            beat_sync_b: true,
            overlay_queue_enabled: false,
            overlay_queue_trigger: default_beat_trigger_config(),
        }
    }

    #[test]
    fn encode_then_decode_round_trips_every_field() {
        let set = sample_shared_set();

        let encoded = encode_shared_set(&set).expect("encode should succeed");
        let decoded = decode_shared_set(&encoded).expect("decode should succeed");

        assert_eq!(decoded.name, set.name);
        assert_eq!(decoded.preset_a, set.preset_a);
        assert_eq!(decoded.preset_b, set.preset_b);
        assert_eq!(decoded.deck_bus, set.deck_bus);
        assert_eq!(decoded.crossfader, set.crossfader);
        assert_eq!(decoded.transition_time, set.transition_time);
        assert_eq!(decoded.color_params_a, set.color_params_a);
        assert_eq!(decoded.color_params_b, set.color_params_b);
        assert_eq!(decoded.slot_composites, set.slot_composites);
        assert_eq!(decoded.time_params, set.time_params);
        assert_eq!(decoded.q_var_params, set.q_var_params);
        assert_eq!(decoded.snapshots, set.snapshots);
        assert_eq!(decoded.snapshot_recall_duration, set.snapshot_recall_duration);
        assert_eq!(decoded.timeline_keyframes, set.timeline_keyframes);
        assert_eq!(decoded.overlays.len(), set.overlays.len());
        assert_eq!(decoded.overlays[0].id, set.overlays[0].id);
        assert_eq!(decoded.overlays[0].text, set.overlays[0].text);
        assert_eq!(decoded.beat_trigger_a, set.beat_trigger_a);
        assert_eq!(decoded.beat_trigger_b, set.beat_trigger_b);
        assert_eq!(decoded.beat_sync_a, set.beat_sync_a);
        assert_eq!(decoded.beat_sync_b, set.beat_sync_b);
        assert_eq!(decoded.overlay_queue_enabled, set.overlay_queue_enabled);
        assert_eq!(decoded.overlay_queue_trigger, set.overlay_queue_trigger);
    }

    #[test]
    fn encoded_link_is_url_safe_and_unpadded() {
        let encoded = encode_shared_set(&sample_shared_set()).expect("encode should succeed");
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
    }

    #[test]
    fn empty_snapshot_slots_and_overlays_round_trip() {
        let mut set = sample_shared_set();
        set.snapshots = std::array::from_fn(|_| None);
        set.overlays = Vec::new();

        let encoded = encode_shared_set(&set).expect("encode should succeed");
        let decoded = decode_shared_set(&encoded).expect("decode should succeed");

        assert!(decoded.snapshots.iter().all(Option::is_none));
        assert!(decoded.overlays.is_empty());
    }

    #[test]
    fn decode_rejects_garbage_input() {
        assert!(decode_shared_set("not a valid share link").is_err());
    }

    #[test]
    fn an_unknown_command_id_key_in_a_snapshot_is_skipped_not_fatal() {
        let wire = SnapshotWire { name: "Weird".to_string(), values: HashMap::from([("not-a-real-command".to_string(), 1.0)]) };
        let snapshot: Snapshot = wire.into();
        assert!(snapshot.values.is_empty());
    }
}
