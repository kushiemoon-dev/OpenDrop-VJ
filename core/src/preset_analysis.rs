//! Measuring a rendered preset: how bright it is, how much it moves, and
//! from that a verdict (black, frozen, too dark) and an energy score.
//!
//! The scan renders a preset for a short while in a child process and hands
//! this module a few sampled frames. Nothing here renders or touches the
//! disk, so the thresholds are testable with synthetic frames.

use std::collections::HashMap;

/// Side of the grayscale thumbnail frames are reduced to before comparing.
pub const SAMPLE_W: usize = 64;
pub const SAMPLE_H: usize = 36;

/// Below this mean brightness (0 to 255) a preset is effectively black.
const BLACK_MEAN_LUMA: f32 = 3.0;
/// Below this it is dark enough to be a poor choice on a projector.
const DARK_MEAN_LUMA: f32 = 14.0;
/// Mean absolute change between sampled frames below which it is frozen.
const FROZEN_MOTION: f32 = 0.6;
/// Motion at which the energy score saturates.
const MOTION_FOR_FULL_ENERGY: f32 = 18.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresetAnalysis {
    /// Mean brightness over all sampled frames, 0 to 255.
    pub mean_luma: f32,
    /// Mean absolute per-pixel change between consecutive sampled frames.
    pub motion: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetVerdict {
    Fine,
    /// Renders nothing visible.
    Black,
    /// Renders something but it does not change.
    Frozen,
    /// Renders, moves, but is very dim.
    Dark,
}

impl PresetVerdict {
    /// Whether the "hide flagged presets" filter hides this preset.
    pub fn is_flagged(self) -> bool {
        self != PresetVerdict::Fine
    }
}

impl PresetAnalysis {
    pub fn verdict(&self) -> PresetVerdict {
        if self.mean_luma < BLACK_MEAN_LUMA {
            PresetVerdict::Black
        } else if self.motion < FROZEN_MOTION {
            PresetVerdict::Frozen
        } else if self.mean_luma < DARK_MEAN_LUMA {
            PresetVerdict::Dark
        } else {
            PresetVerdict::Fine
        }
    }

    /// How lively the preset looks, 0 (still) to 1 (very busy): the number
    /// the energy auto-pilot matches against the music.
    pub fn energy(&self) -> f32 {
        (self.motion / MOTION_FOR_FULL_ENERGY).clamp(0.0, 1.0)
    }
}

impl PresetAnalysis {
    /// One-line text form used to pass a result from the scan child process
    /// to the app.
    pub fn to_line(&self) -> String {
        format!("{:.3} {:.3}", self.mean_luma, self.motion)
    }

    pub fn from_line(line: &str) -> Option<Self> {
        let mut parts = line.split_whitespace();
        let mean_luma: f32 = parts.next()?.parse().ok()?;
        let motion: f32 = parts.next()?.parse().ok()?;
        (parts.next().is_none() && mean_luma.is_finite() && motion.is_finite()).then_some(Self { mean_luma, motion })
    }
}

/// Results of scanning a library, by preset name. A preset the renderer
/// rejected is recorded as `None` and counts as flagged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanTable {
    entries: HashMap<String, Option<PresetAnalysis>>,
}

impl ScanTable {
    pub fn from_entries(entries: HashMap<String, Option<PresetAnalysis>>) -> Self {
        Self { entries }
    }

    pub fn entries(&self) -> &HashMap<String, Option<PresetAnalysis>> {
        &self.entries
    }

    pub fn record(&mut self, preset: &str, result: Option<PresetAnalysis>) {
        self.entries.insert(preset.to_string(), result);
    }

    pub fn contains(&self, preset: &str) -> bool {
        self.entries.contains_key(preset)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether the scan judged `preset` unfit. A preset never scanned is not
    /// flagged: no result means no verdict.
    pub fn is_flagged(&self, preset: &str) -> bool {
        match self.entries.get(preset) {
            Some(Some(analysis)) => analysis.verdict().is_flagged(),
            Some(None) => true,
            None => false,
        }
    }

    pub fn flagged_count(&self) -> usize {
        self.entries.keys().filter(|name| self.is_flagged(name)).count()
    }

    /// Energy score of a scanned, usable preset.
    pub fn energy(&self, preset: &str) -> Option<f32> {
        self.entries.get(preset).copied().flatten().map(|a| a.energy())
    }
}

/// Reduces an RGBA frame to a `SAMPLE_W` x `SAMPLE_H` grayscale grid by
/// averaging blocks, so comparing frames costs a few thousand operations.
/// Returns an empty vector when the buffer does not match `width * height`.
pub fn downsample_luma(rgba: &[u8], width: usize, height: usize) -> Vec<f32> {
    if width < SAMPLE_W || height < SAMPLE_H || rgba.len() != width * height * 4 {
        return Vec::new();
    }
    let mut out = vec![0.0f32; SAMPLE_W * SAMPLE_H];
    let mut counts = vec![0u32; SAMPLE_W * SAMPLE_H];
    for y in 0..height {
        let cell_y = y * SAMPLE_H / height;
        for x in 0..width {
            let cell = cell_y * SAMPLE_W + x * SAMPLE_W / width;
            let i = (y * width + x) * 4;
            let luma = 0.2126 * f32::from(rgba[i]) + 0.7152 * f32::from(rgba[i + 1]) + 0.0722 * f32::from(rgba[i + 2]);
            out[cell] += luma;
            counts[cell] += 1;
        }
    }
    for (value, count) in out.iter_mut().zip(counts) {
        *value /= count.max(1) as f32;
    }
    out
}

/// Combines downsampled frames (in time order) into one analysis. Fewer than
/// two usable frames cannot show motion and yields `None`.
pub fn analyze_frames(frames: &[Vec<f32>]) -> Option<PresetAnalysis> {
    let usable: Vec<&Vec<f32>> = frames.iter().filter(|f| f.len() == SAMPLE_W * SAMPLE_H).collect();
    if usable.len() < 2 {
        return None;
    }
    let cells = (SAMPLE_W * SAMPLE_H) as f32;
    let mean_luma = usable.iter().map(|f| f.iter().sum::<f32>() / cells).sum::<f32>() / usable.len() as f32;
    let motion = usable
        .windows(2)
        .map(|pair| pair[0].iter().zip(pair[1].iter()).map(|(a, b)| (a - b).abs()).sum::<f32>() / cells)
        .sum::<f32>()
        / (usable.len() - 1) as f32;
    Some(PresetAnalysis { mean_luma, motion })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(value: f32) -> Vec<f32> {
        vec![value; SAMPLE_W * SAMPLE_H]
    }

    fn rgba_frame(width: usize, height: usize, gray: u8) -> Vec<u8> {
        (0..width * height).flat_map(|_| [gray, gray, gray, 255]).collect()
    }

    #[test]
    fn a_black_render_is_black() {
        let a = analyze_frames(&[flat(0.0), flat(0.0), flat(0.0)]).unwrap();
        assert_eq!(a.verdict(), PresetVerdict::Black);
        assert!(a.verdict().is_flagged());
    }

    #[test]
    fn a_bright_but_unchanging_render_is_frozen() {
        let a = analyze_frames(&[flat(120.0), flat(120.0), flat(120.0)]).unwrap();
        assert_eq!(a.verdict(), PresetVerdict::Frozen);
        assert_eq!(a.energy(), 0.0);
    }

    #[test]
    fn a_dim_moving_render_is_dark() {
        let a = analyze_frames(&[flat(6.0), flat(12.0), flat(6.0)]).unwrap();
        assert_eq!(a.verdict(), PresetVerdict::Dark);
    }

    #[test]
    fn a_bright_moving_render_is_fine_and_busier_means_more_energy() {
        let calm = analyze_frames(&[flat(100.0), flat(104.0), flat(100.0)]).unwrap();
        let busy = analyze_frames(&[flat(60.0), flat(180.0), flat(60.0)]).unwrap();
        assert_eq!(calm.verdict(), PresetVerdict::Fine);
        assert_eq!(busy.verdict(), PresetVerdict::Fine);
        assert!(!busy.verdict().is_flagged());
        assert!(busy.energy() > calm.energy());
        assert_eq!(busy.energy(), 1.0, "the score saturates");
    }

    #[test]
    fn fewer_than_two_usable_frames_cannot_be_judged() {
        assert!(analyze_frames(&[]).is_none());
        assert!(analyze_frames(&[flat(50.0)]).is_none());
        assert!(analyze_frames(&[flat(50.0), vec![1.0, 2.0]]).is_none(), "a malformed frame is ignored");
    }

    #[test]
    fn a_result_round_trips_through_its_line_and_garbage_is_rejected() {
        let original = PresetAnalysis { mean_luma: 87.5, motion: 4.25 };
        assert_eq!(PresetAnalysis::from_line(&original.to_line()), Some(original));
        assert_eq!(PresetAnalysis::from_line("1.0"), None);
        assert_eq!(PresetAnalysis::from_line("1.0 2.0 3.0"), None);
        assert_eq!(PresetAnalysis::from_line("abc 2.0"), None);
        assert_eq!(PresetAnalysis::from_line("NaN 2.0"), None);
    }

    #[test]
    fn the_scan_table_flags_bad_and_failed_presets_but_not_unscanned_ones() {
        let mut table = ScanTable::default();
        table.record("good", analyze_frames(&[flat(60.0), flat(180.0), flat(60.0)]));
        table.record("black", analyze_frames(&[flat(0.0), flat(0.0)]));
        table.record("rejected", None);

        assert!(!table.is_flagged("good"));
        assert!(table.is_flagged("black"));
        assert!(table.is_flagged("rejected"));
        assert!(!table.is_flagged("never scanned"));
        assert_eq!(table.flagged_count(), 2);
        assert_eq!(table.len(), 3);
        assert_eq!(table.energy("good"), Some(1.0));
        assert_eq!(table.energy("rejected"), None);
        assert_eq!(table.energy("never scanned"), None);
    }

    #[test]
    fn downsampling_averages_blocks_and_rejects_bad_buffers() {
        let frame = rgba_frame(192, 108, 200);
        let small = downsample_luma(&frame, 192, 108);
        assert_eq!(small.len(), SAMPLE_W * SAMPLE_H);
        assert!(small.iter().all(|v| (v - 200.0).abs() < 0.5));

        assert!(downsample_luma(&frame, 10, 10).is_empty());
        assert!(downsample_luma(&frame[..100], 192, 108).is_empty());
    }
}
