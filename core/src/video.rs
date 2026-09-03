//! Video-loop layer state (Step 14 of the Phase 8 VJ-panels plan): the
//! user-facing half of the video background layer: on/off, opacity,
//! clip-rotation mode, and the 4 beat/volume-reactive toggles. Port of
//! `playback-store.svelte.ts`'s `videoState` plus `onVideoBeat`/
//! `onVideoAudioTick` (OpenDrop-VJ), following the same colocated-test
//! convention as `snapshot.rs`/`timeline.rs`/`strobe.rs`/`lfo.rs`.
//!
//! **No paths, no handles, no GL here**, same boundary `overlay.rs` draws:
//! a clip is identified by an opaque `String` key, and the file it maps to
//! (plus the `ffmpeg` subprocess decoding it and the GL texture it lands
//! in) lives in `app`/`io`. `selected_clip_keys` and `current_clip_index`
//! are therefore expressed against a caller-supplied key list, exactly as
//! the web store expressed them against `[...builtinClips, ...userClips]`.
//!
//! **NDI is not a field here.** In the web, an NDI source was one of the
//! video layer's own `ClipRef` kinds; natively, NDI-in is a separately
//! owned subsystem (`opendrop_io::ndi`) whose "is it actually receiving"
//! truth is `NdiSnapshot::receive_active`. Mirroring that into a second
//! field here would create two sources of truth for one fact, so every
//! guard that needs it takes an explicit `ndi_active: bool` instead: the
//! caller reads it from the NDI snapshot it already has.

use crate::blend::ColorParams;

/// How the layer picks its next clip on the beat. Same three modes as
/// `SidebarVideo.svelte`'s Shuffle/Seq/Manual buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoAdvance {
    #[default]
    Shuffle,
    Sequential,
    Manual,
}

impl VideoAdvance {
    /// In `SidebarVideo.svelte`'s button order, so the native panel lists
    /// them the same way.
    pub const ALL: [VideoAdvance; 3] = [VideoAdvance::Shuffle, VideoAdvance::Sequential, VideoAdvance::Manual];

    pub fn label(self) -> &'static str {
        match self {
            VideoAdvance::Shuffle => "Shuffle",
            VideoAdvance::Sequential => "Seq",
            VideoAdvance::Manual => "Manual",
        }
    }
}

/// The 4 values `SidebarVideo.svelte`'s beats-per-cut `<select>` offers.
pub const BEATS_PER_CUT_CHOICES: [u32; 4] = [4, 8, 16, 32];

/// Brightness multiplier applied on a beat while `react_flash` is on:
/// `videoBrightness = beat && reactFlash ? 1.4 : 1` (`+page.svelte:310`).
pub const BEAT_FLASH_BRIGHTNESS_MUL: f64 = 1.4;

/// Hue rotation (degrees) applied on a beat while `react_hue` is on:
/// `videoHueRotateDeg = beat && reactHue ? 35 : 0` (`+page.svelte:311`).
pub const BEAT_HUE_ROTATE_DEG: f64 = 35.0;

/// Bass-driven playback-rate bounds: `0.6 + bass * 1.4` for `bass` in
/// 0..1 (`onVideoAudioTick`), i.e. 0.6 at silence, 2.0 at a full-scale
/// peak. Exposed so the pacing side (`opendrop_io::video_capture`) can
/// clamp to the same range instead of re-deriving it.
pub const WARP_RATE_MIN: f64 = 0.6;
pub const WARP_RATE_MAX: f64 = 2.0;

/// Smoothing factor of the same one-pole filter the web used
/// (`playbackRate += (target - playbackRate) * 0.15`).
const WARP_SMOOTHING: f64 = 0.15;

/// Largest file the "+ Video" button will copy into the clip folder:
/// `addVideoFromFile`'s `file.size > 50 * 1024 * 1024` guard, kept as-is
/// even though the native side writes to a real filesystem rather than
/// IndexedDB: it's a "don't let one drag-and-drop eat the disk" guard, not
/// a browser-quota one.
pub const MAX_CLIP_BYTES: u64 = 50 * 1024 * 1024;

/// User-facing video-layer state (Video panel, `app::ui::video`).
/// Defaults are `videoState`'s own initial values, field for field.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoState {
    pub enabled: bool,
    /// 0..1, the panel's α crossfader. Sole control of how much of the
    /// layer shows: see `Compositor::composite_video_layer`.
    pub opacity: f64,
    pub advance: VideoAdvance,
    /// Beats between two automatic clip cuts. Only the values in
    /// [`BEATS_PER_CUT_CHOICES`] are reachable from the panel, but nothing
    /// here enforces that set (same latitude `StrobeState::rate` takes).
    pub beats_per_cut: u32,
    /// Cut to another clip on the beat.
    pub react_cut: bool,
    /// Brighten the layer on the beat ([`BEAT_FLASH_BRIGHTNESS_MUL`]).
    pub react_flash: bool,
    /// Speed the clip up/down with the bass ([`Self::on_audio_tick`]).
    pub react_warp: bool,
    /// Rotate the layer's hue on the beat ([`BEAT_HUE_ROTATE_DEG`]).
    pub react_hue: bool,
    /// Which clips take part in the auto-cut rotation, by key. Empty means
    /// "no filter": every clip participates (the web's own default, so an
    /// existing user sees no change until they tick something).
    pub selected_clip_keys: Vec<String>,
    /// Index into the caller's full clip list. Taken modulo the list
    /// length at read time, exactly as `+page.svelte`'s
    /// `allClips[videoState.currentClipIndex % allClips.length]` did, so a
    /// shrinking library never makes this index invalid.
    pub current_clip_index: usize,
    /// Playback speed multiplier driven by [`Self::on_audio_tick`], 1.0
    /// when `react_warp` is off.
    pub playback_rate: f64,
    /// Live-camera device identifier the layer is currently fed from
    /// (`/dev/videoN`, a DirectShow device name, an AVFoundation index),
    /// or `None` for the clip library. The web's `liveDeviceId`.
    pub live_device: Option<String>,
    /// Human-readable name of that camera, for the panel's button label.
    pub live_label: String,
    /// Internal cut-advance counter: `playback-store.svelte.ts`'s
    /// module-level `beatCount`, private for the same reason it was
    /// non-reactive there: nothing renders it.
    beat_count: u32,
}

impl Default for VideoState {
    fn default() -> Self {
        Self {
            enabled: false,
            opacity: 0.6,
            advance: VideoAdvance::Shuffle,
            beats_per_cut: 8,
            react_cut: true,
            react_flash: true,
            react_warp: true,
            react_hue: false,
            selected_clip_keys: Vec::new(),
            current_clip_index: 0,
            playback_rate: 1.0,
            live_device: None,
            live_label: String::new(),
            beat_count: 0,
        }
    }
}

/// Positions in `all_keys` that take part in the auto-cut rotation: every
/// clip when `selected` is empty, otherwise only the selected ones. Port of
/// `activeClipIndices`.
pub fn active_clip_indices(all_keys: &[String], selected: &[String]) -> Vec<usize> {
    if selected.is_empty() {
        return (0..all_keys.len()).collect();
    }
    all_keys.iter().enumerate().filter(|(_, k)| selected.iter().any(|s| s == *k)).map(|(i, _)| i).collect()
}

impl VideoState {
    /// Whether an external single feed (live camera or a running NDI
    /// receive) is driving the layer instead of the clip library. Such a
    /// feed is one stream, not a cycling library: the web's `onVideoBeat`
    /// and `onVideoAudioTick` both bail out on it, and so does everything
    /// below.
    pub fn external_feed_active(&self, ndi_active: bool) -> bool {
        self.live_device.is_some() || ndi_active
    }

    pub fn toggle_clip_selection(&mut self, key: &str) {
        match self.selected_clip_keys.iter().position(|k| k == key) {
            Some(i) => {
                self.selected_clip_keys.remove(i);
            }
            None => self.selected_clip_keys.push(key.to_string()),
        }
    }

    pub fn clear_clip_selection(&mut self) {
        self.selected_clip_keys.clear();
    }

    /// Drops a deleted clip's key from the rotation and clamps
    /// `current_clip_index` back into range: `removeVideoClip`'s two
    /// bookkeeping steps, minus the storage delete (which is `app`'s, since
    /// only it knows the file).
    pub fn forget_clip(&mut self, key: &str, remaining_clips: usize) {
        self.selected_clip_keys.retain(|k| k != key);
        if self.current_clip_index >= remaining_clips {
            self.current_clip_index = 0;
        }
    }

    /// Beat-driven clip cut. Returns `true` when `current_clip_index`
    /// actually moved, so the caller knows to restart its decoder on the
    /// new clip; `false` (no change) on every other beat, and on every beat
    /// at all while the layer is off, in Manual mode, with `react_cut` off,
    /// fed by an external feed, or with fewer than 2 clips in rotation.
    /// Port of `onVideoBeat`.
    ///
    /// `rand01` is the caller's uniform 0..1 draw, used only in Shuffle
    /// mode: `core` owns no RNG of its own outside `rng.rs`'s seeded
    /// generators, and `Show` already threads one (see `Show::on_beat`'s
    /// call site).
    pub fn on_beat(&mut self, all_keys: &[String], ndi_active: bool, rand01: f64) -> bool {
        if self.external_feed_active(ndi_active) {
            return false;
        }
        let active = active_clip_indices(all_keys, &self.selected_clip_keys);
        if !(self.enabled && self.react_cut && self.advance != VideoAdvance::Manual && active.len() > 1) {
            return false;
        }
        self.beat_count = (self.beat_count + 1) % self.beats_per_cut.max(1);
        if self.beat_count != 0 {
            return false;
        }
        let pos = active.iter().position(|&i| i == self.current_clip_index);
        let next_pos = match self.advance {
            VideoAdvance::Shuffle => ((rand01.clamp(0.0, 1.0) * active.len() as f64) as usize).min(active.len() - 1),
            _ => match pos {
                None => 0,
                Some(p) => (p + 1) % active.len(),
            },
        };
        let next = active[next_pos];
        let changed = next != self.current_clip_index;
        self.current_clip_index = next;
        changed
    }

    /// Bass-driven speed warp, called once per render tick. Port of
    /// `onVideoAudioTick`: a one-pole approach toward `0.6 + bass * 1.4`,
    /// snapping straight back to 1.0 whenever warp doesn't apply (off, or
    /// an external feed, whose rate this app cannot steer any more than the
    /// web could steer a `MediaStream`'s).
    pub fn on_audio_tick(&mut self, bass: f64, ndi_active: bool) {
        if self.enabled && self.react_warp && !self.external_feed_active(ndi_active) {
            let target = WARP_RATE_MIN + bass.clamp(0.0, 1.0) * (WARP_RATE_MAX - WARP_RATE_MIN);
            self.playback_rate += (target - self.playback_rate) * WARP_SMOOTHING;
        } else {
            self.playback_rate = 1.0;
        }
    }

    /// Switches the layer to a live camera. Mirrors `setLiveCamera`,
    /// including its auto-enable: picking a source is itself the intent to
    /// see it. Dropping the NDI half of the web's mutual exclusion is the
    /// caller's job (`app` sends `NdiControl::StopReceive`), for the reason
    /// in this module's doc comment.
    pub fn set_live_camera(&mut self, device: String, label: String) {
        self.live_device = Some(device);
        self.live_label = label;
        self.enabled = true;
    }

    /// Drops the live camera and falls back to the clip library. Leaves
    /// `enabled` untouched, same as `clearLiveCamera`.
    pub fn clear_live_camera(&mut self) {
        self.live_device = None;
        self.live_label.clear();
    }

    /// The layer's color correction for this frame, in the same
    /// `ColorParams` units `composite_layer` already consumes (0..1 with
    /// 0.5 = neutral for saturate/brightness/contrast, `hue_rotate * 360`
    /// degrees). This is what lets the video layer reuse the deck shader
    /// verbatim: exactly as `compositor.ts` did, setting `uHueRotateDeg`/
    /// `uBrightnessMul` on the same program for its own 5th layer instead
    /// of writing a second one.
    ///
    /// `beat_pulse` is the same short post-beat window the beat-reactive
    /// overlays use (`app`'s `BEAT_PULSE_DURATION`), standing in for the
    /// web's `beatSyncState.beat`.
    pub fn layer_color_params(&self, beat_pulse: bool) -> ColorParams {
        let brightness_mul = if beat_pulse && self.react_flash { BEAT_FLASH_BRIGHTNESS_MUL } else { 1.0 };
        let hue_deg = if beat_pulse && self.react_hue { BEAT_HUE_ROTATE_DEG } else { 0.0 };
        ColorParams {
            hue_rotate: hue_deg / 360.0,
            saturate: 0.5,
            brightness: brightness_mul / 2.0,
            contrast: 0.5,
            invert: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blend::DEFAULT_COLOR_PARAMS;

    fn keys(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("clip{i}")).collect()
    }

    /// A state that will actually cut on the very next beat.
    fn cutting_state() -> VideoState {
        VideoState { enabled: true, beats_per_cut: 1, advance: VideoAdvance::Sequential, ..VideoState::default() }
    }

    #[test]
    fn defaults_match_the_web_store() {
        let s = VideoState::default();
        assert!(!s.enabled);
        assert_eq!(s.opacity, 0.6);
        assert_eq!(s.advance, VideoAdvance::Shuffle);
        assert_eq!(s.beats_per_cut, 8);
        assert!(s.react_cut);
        assert!(s.react_flash);
        assert!(s.react_warp);
        assert!(!s.react_hue);
        assert_eq!(s.playback_rate, 1.0);
        assert!(s.selected_clip_keys.is_empty());
        assert_eq!(s.current_clip_index, 0);
        assert_eq!(s.live_device, None);
    }

    mod clip_selection {
        use super::*;

        #[test]
        fn an_empty_selection_means_every_clip_participates() {
            assert_eq!(active_clip_indices(&keys(3), &[]), vec![0, 1, 2]);
        }

        #[test]
        fn a_selection_filters_down_to_its_own_keys_in_list_order() {
            let selected = vec!["clip2".to_string(), "clip0".to_string()];
            assert_eq!(active_clip_indices(&keys(4), &selected), vec![0, 2]);
        }

        #[test]
        fn a_selected_key_that_no_longer_exists_is_simply_skipped() {
            let selected = vec!["gone".to_string(), "clip1".to_string()];
            assert_eq!(active_clip_indices(&keys(2), &selected), vec![1]);
        }

        #[test]
        fn toggling_adds_then_removes_a_key() {
            let mut s = VideoState::default();
            s.toggle_clip_selection("a");
            assert_eq!(s.selected_clip_keys, ["a"]);
            s.toggle_clip_selection("b");
            assert_eq!(s.selected_clip_keys, ["a", "b"]);
            s.toggle_clip_selection("a");
            assert_eq!(s.selected_clip_keys, ["b"]);
        }

        #[test]
        fn clearing_drops_every_key() {
            let mut s = VideoState::default();
            s.toggle_clip_selection("a");
            s.clear_clip_selection();
            assert!(s.selected_clip_keys.is_empty());
        }

        #[test]
        fn forgetting_a_clip_unselects_it_and_leaves_a_still_valid_index_alone() {
            let mut s = VideoState { current_clip_index: 1, ..VideoState::default() };
            s.toggle_clip_selection("a");
            s.toggle_clip_selection("b");
            s.forget_clip("a", 3);
            assert_eq!(s.selected_clip_keys, ["b"]);
            assert_eq!(s.current_clip_index, 1, "1 is still in range for 3 remaining clips");
        }

        #[test]
        fn forgetting_a_clip_clamps_an_index_that_just_went_out_of_range() {
            let mut s = VideoState { current_clip_index: 2, ..VideoState::default() };
            s.forget_clip("whatever", 2);
            assert_eq!(s.current_clip_index, 0);
        }
    }

    mod beat_cut {
        use super::*;

        #[test]
        fn a_disabled_layer_never_cuts() {
            let mut s = VideoState { enabled: false, ..cutting_state() };
            assert!(!s.on_beat(&keys(4), false, 0.0));
            assert_eq!(s.current_clip_index, 0);
        }

        #[test]
        fn manual_mode_never_cuts() {
            let mut s = VideoState { advance: VideoAdvance::Manual, ..cutting_state() };
            assert!(!s.on_beat(&keys(4), false, 0.0));
        }

        #[test]
        fn react_cut_off_never_cuts() {
            let mut s = VideoState { react_cut: false, ..cutting_state() };
            assert!(!s.on_beat(&keys(4), false, 0.0));
        }

        #[test]
        fn a_single_clip_library_never_cuts() {
            let mut s = cutting_state();
            assert!(!s.on_beat(&keys(1), false, 0.0));
        }

        #[test]
        fn an_external_feed_never_cuts() {
            let mut s = cutting_state();
            s.set_live_camera("/dev/video0".to_string(), "Webcam".to_string());
            assert!(!s.on_beat(&keys(4), false, 0.0));
            s.clear_live_camera();
            assert!(!s.on_beat(&keys(4), true, 0.0), "an active NDI receive is the same case");
        }

        #[test]
        fn sequential_advances_one_clip_per_beats_per_cut_beats() {
            let mut s = VideoState { enabled: true, beats_per_cut: 3, advance: VideoAdvance::Sequential, ..VideoState::default() };
            let all = keys(3);
            assert!(!s.on_beat(&all, false, 0.0));
            assert!(!s.on_beat(&all, false, 0.0));
            assert!(s.on_beat(&all, false, 0.0), "3rd beat cuts");
            assert_eq!(s.current_clip_index, 1);
            assert!(!s.on_beat(&all, false, 0.0));
            assert!(!s.on_beat(&all, false, 0.0));
            assert!(s.on_beat(&all, false, 0.0));
            assert_eq!(s.current_clip_index, 2);
        }

        #[test]
        fn sequential_wraps_around_the_end_of_the_rotation() {
            let mut s = VideoState { current_clip_index: 2, ..cutting_state() };
            assert!(s.on_beat(&keys(3), false, 0.0));
            assert_eq!(s.current_clip_index, 0);
        }

        #[test]
        fn sequential_only_visits_the_selected_clips() {
            let mut s = cutting_state();
            s.toggle_clip_selection("clip1");
            s.toggle_clip_selection("clip3");
            let all = keys(4);
            assert!(s.on_beat(&all, false, 0.0));
            assert_eq!(s.current_clip_index, 1, "index 0 isn't in the rotation, so pos==None starts at the first active");
            assert!(s.on_beat(&all, false, 0.0));
            assert_eq!(s.current_clip_index, 3);
            assert!(s.on_beat(&all, false, 0.0));
            assert_eq!(s.current_clip_index, 1);
        }

        #[test]
        fn shuffle_picks_the_draw_indexed_clip_and_never_runs_off_the_end() {
            let mut s = VideoState { advance: VideoAdvance::Shuffle, ..cutting_state() };
            let all = keys(4);
            s.on_beat(&all, false, 0.5);
            assert_eq!(s.current_clip_index, 2, "0.5 * 4 == 2");
            // A draw of exactly 1.0 (or anything out of range) must still
            // land on a valid clip, not index 4.
            s.on_beat(&all, false, 1.0);
            assert_eq!(s.current_clip_index, 3);
            s.on_beat(&all, false, -7.0);
            assert_eq!(s.current_clip_index, 0);
        }

        #[test]
        fn a_cut_landing_on_the_same_clip_reports_no_change() {
            // Shuffle can redraw the clip already playing: the caller must
            // not restart the decoder for that.
            let mut s = VideoState { advance: VideoAdvance::Shuffle, ..cutting_state() };
            assert!(!s.on_beat(&keys(2), false, 0.0), "draw 0 == current index 0");
            assert_eq!(s.current_clip_index, 0);
        }

        #[test]
        fn a_zero_beats_per_cut_does_not_divide_by_zero() {
            let mut s = VideoState { beats_per_cut: 0, ..cutting_state() };
            assert!(s.on_beat(&keys(2), false, 0.0), "clamped to 1: every beat cuts");
        }
    }

    mod warp {
        use super::*;

        #[test]
        fn warp_moves_the_rate_toward_the_bass_target() {
            let mut s = VideoState { enabled: true, ..VideoState::default() };
            s.on_audio_tick(1.0, false);
            // target 2.0, one 0.15 step from 1.0
            assert!((s.playback_rate - 1.15).abs() < 1e-9, "rate = {}", s.playback_rate);
            for _ in 0..500 {
                s.on_audio_tick(1.0, false);
            }
            assert!((s.playback_rate - WARP_RATE_MAX).abs() < 1e-6);
        }

        #[test]
        fn silence_settles_at_the_low_end_of_the_range() {
            let mut s = VideoState { enabled: true, ..VideoState::default() };
            for _ in 0..500 {
                s.on_audio_tick(0.0, false);
            }
            assert!((s.playback_rate - WARP_RATE_MIN).abs() < 1e-6);
        }

        #[test]
        fn warp_off_snaps_straight_back_to_one() {
            let mut s = VideoState { enabled: true, playback_rate: 1.8, react_warp: false, ..VideoState::default() };
            s.on_audio_tick(1.0, false);
            assert_eq!(s.playback_rate, 1.0);
        }

        #[test]
        fn an_external_feed_pins_the_rate_at_one() {
            let mut s = VideoState { enabled: true, playback_rate: 1.8, ..VideoState::default() };
            s.on_audio_tick(1.0, true);
            assert_eq!(s.playback_rate, 1.0);
            s.set_live_camera("/dev/video0".to_string(), "Webcam".to_string());
            s.playback_rate = 1.8;
            s.on_audio_tick(1.0, false);
            assert_eq!(s.playback_rate, 1.0);
        }

        #[test]
        fn an_out_of_range_bass_reading_cannot_push_the_rate_past_the_documented_bounds() {
            let mut s = VideoState { enabled: true, ..VideoState::default() };
            for _ in 0..500 {
                s.on_audio_tick(9.0, false);
            }
            assert!(s.playback_rate <= WARP_RATE_MAX + 1e-9, "rate = {}", s.playback_rate);
        }
    }

    mod color_params {
        use super::*;

        #[test]
        fn off_beat_the_layer_is_color_neutral() {
            let s = VideoState::default();
            assert_eq!(s.layer_color_params(false), DEFAULT_COLOR_PARAMS);
        }

        #[test]
        fn a_beat_with_both_reactions_off_is_still_neutral() {
            let s = VideoState { react_flash: false, react_hue: false, ..VideoState::default() };
            assert_eq!(s.layer_color_params(true), DEFAULT_COLOR_PARAMS);
        }

        #[test]
        fn flash_raises_brightness_to_the_webs_1_4x() {
            let s = VideoState { react_flash: true, ..VideoState::default() };
            let p = s.layer_color_params(true);
            // `composite_layer` multiplies this field by 2 to get the shader's
            // uBrightnessMul, so 0.7 here is 1.4x on screen.
            assert!((p.brightness - 0.7).abs() < 1e-12);
            assert!((p.brightness * 2.0 - BEAT_FLASH_BRIGHTNESS_MUL).abs() < 1e-12);
            assert_eq!(p.hue_rotate, 0.0);
        }

        #[test]
        fn hue_rotates_by_the_webs_35_degrees() {
            let s = VideoState { react_hue: true, react_flash: false, ..VideoState::default() };
            let p = s.layer_color_params(true);
            // `composite_layer` multiplies this field by 360 for uHueRotateDeg.
            assert!((p.hue_rotate * 360.0 - BEAT_HUE_ROTATE_DEG).abs() < 1e-12);
            assert!((p.brightness - 0.5).abs() < 1e-12);
        }

        #[test]
        fn saturation_contrast_and_invert_are_never_touched_by_the_video_layer() {
            let s = VideoState { react_flash: true, react_hue: true, ..VideoState::default() };
            let p = s.layer_color_params(true);
            assert_eq!((p.saturate, p.contrast, p.invert), (0.5, 0.5, 0.0));
        }
    }

    mod live_camera {
        use super::*;

        #[test]
        fn selecting_a_camera_enables_the_layer() {
            let mut s = VideoState::default();
            assert!(!s.enabled);
            s.set_live_camera("/dev/video2".to_string(), "C920".to_string());
            assert!(s.enabled);
            assert_eq!(s.live_device.as_deref(), Some("/dev/video2"));
            assert_eq!(s.live_label, "C920");
            assert!(s.external_feed_active(false));
        }

        #[test]
        fn clearing_a_camera_leaves_enabled_alone() {
            let mut s = VideoState::default();
            s.set_live_camera("/dev/video2".to_string(), "C920".to_string());
            s.clear_live_camera();
            assert!(s.enabled, "clearLiveCamera leaves `enabled` untouched");
            assert_eq!(s.live_device, None);
            assert!(s.live_label.is_empty());
            assert!(!s.external_feed_active(false));
        }

        #[test]
        fn a_running_ndi_receive_counts_as_an_external_feed_on_its_own() {
            let s = VideoState::default();
            assert!(!s.external_feed_active(false));
            assert!(s.external_feed_active(true));
        }
    }

    #[test]
    fn every_advance_mode_has_a_distinct_label() {
        let mut labels: Vec<&str> = VideoAdvance::ALL.iter().map(|m| m.label()).collect();
        assert_eq!(labels, ["Shuffle", "Seq", "Manual"]);
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), 3);
    }
}
