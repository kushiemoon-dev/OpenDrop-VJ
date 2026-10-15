//! Out-of-process preset analysis for the library scan. Re-invoked as
//! `--analyze-preset <preset-path> <out-path>`, it renders the preset for a
//! short while against music-like synthetic audio, samples a few frames and
//! writes the measured brightness and motion (`PresetAnalysis::to_line`) to
//! `out-path`. Same isolation as the thumbnail child: one preset that
//! crashes projectM takes down this process only, which the parent sees as
//! a non-zero exit.

use std::ffi::CString;
use std::path::Path;
use std::sync::atomic::Ordering;

use opendrop_core::preset_analysis::{analyze_frames, downsample_luma, PresetAnalysis};
use opendrop_engine::thumbnail::{synthetic_pcm, THUMB_H, THUMB_W, WARMUP_FRAMES};

use crate::egl_headless;
use crate::thumbnail_child::{on_preset_failed, read_back, write_atomically, PRESET_FAILED};

/// Exit code for "no usable result": the preset was rejected or the result
/// could not be written. The parent treats every non-zero exit alike.
const EXIT_FAILED: i32 = 1;
/// Frames are sampled every `SAMPLE_EVERY` frames from `FIRST_SAMPLE_FRAME`
/// on. Sampling starts early on purpose: some presets are lit only at first
/// and fade once their audio settles, and they must be seen while lit.
const SAMPLE_EVERY: usize = 6;
const FIRST_SAMPLE_FRAME: usize = 12;
/// How long each of the two audio signals runs.
const NOISE_PHASE_FRAMES: usize = WARMUP_FRAMES + 1 + 48;
const MUSIC_PHASE_FRAMES: usize = 60;

/// Sample rate and length of the audio handed to projectM each frame, the
/// same ~10ms stereo chunk the app injects.
const SAMPLE_RATE: f32 = 48_000.0;
const FRAMES_PER_CHUNK: usize = 480;

/// Music-like stereo PCM for frame `frame`: a bass note that thumps on every
/// fifteenth frame, a mid tone sweeping slowly up and down, and a short burst
/// of treble on the off-beat. White noise would give every band the same
/// constant level, which leaves presets that react to bass, mid and treble
/// separately standing still. Deterministic, so a scan is repeatable.
fn synth_music_pcm(frame: usize) -> Vec<f32> {
    let beat_phase = frame % 15;
    let bass_gain = if beat_phase < 4 { 0.8 } else { 0.15 };
    let treble_gain = if (7..9).contains(&beat_phase) { 0.35 } else { 0.03 };
    let sweep_hz = 600.0 + 500.0 * ((frame as f32) * 0.05).sin();
    let mut noise_state: u32 = 0x9E37_79B9 ^ (frame as u32).wrapping_mul(0x85EB_CA6B);
    let mut next_noise = move || {
        noise_state ^= noise_state << 13;
        noise_state ^= noise_state >> 17;
        noise_state ^= noise_state << 5;
        (noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    (0..FRAMES_PER_CHUNK)
        .flat_map(|k| {
            let t = (frame * FRAMES_PER_CHUNK + k) as f32 / SAMPLE_RATE;
            let bass = (std::f32::consts::TAU * 55.0 * t).sin() * bass_gain;
            let mid = (std::f32::consts::TAU * sweep_hz * t).sin() * 0.2;
            let treble = next_noise() * treble_gain;
            let sample = (bass + mid + treble).clamp(-1.0, 1.0);
            [sample, sample]
        })
        .collect()
}

/// White noise with a loud pulse every fifteenth frame over a quiet bed. It
/// excites every frequency band at once, which some presets need before they
/// draw anything.
fn pulsing_noise_pcm(base: &[f32], frame: usize) -> Vec<f32> {
    let gain = if frame % 15 < 4 { 1.0 } else { 0.25 };
    base.iter().map(|s| s * gain).collect()
}

/// Renders `frames` frames of `pcm_for(frame)` and samples the output every
/// `SAMPLE_EVERY` frames once `warmup` frames have passed.
///
/// # Safety
/// `handle` must be a live projectM instance whose GL context is current.
unsafe fn render_phase(
    handle: opendrop_engine::ffi::projectm_handle,
    gl: &glow::Context,
    total_frames: usize,
    pcm_for: &dyn Fn(usize) -> Vec<f32>,
) -> Option<PresetAnalysis> {
    let mut frames: Vec<Vec<f32>> = Vec::new();
    for frame in 0..total_frames {
        let pcm = pcm_for(frame);
        unsafe {
            opendrop_engine::ffi::projectm_pcm_add_float(
                handle,
                pcm.as_ptr(),
                (pcm.len() / 2) as u32,
                opendrop_engine::ffi::projectm_channels_PROJECTM_STEREO,
            );
            let before = opendrop_engine::gl_state::save(gl);
            opendrop_engine::ffi::projectm_opengl_render_frame(handle);
            opendrop_engine::gl_state::restore(gl, &before);
        }
        if frame >= FIRST_SAMPLE_FRAME && (frame - FIRST_SAMPLE_FRAME).is_multiple_of(SAMPLE_EVERY) {
            let pixels = read_back(gl);
            frames.push(downsample_luma(&pixels, THUMB_W as usize, THUMB_H as usize));
        }
    }
    analyze_frames(&frames)
}

/// Analyses `preset_path`, writes the result to `out_path` and exits: 0 once
/// the file is written, non-zero otherwise. Never returns.
///
/// The preset is measured under two different audio signals and judged on the
/// better of the two (see [`PresetAnalysis::best_of`]): presets differ in what
/// they react to, and no single synthetic signal is a fair test for all of them.
pub fn run_analyze_preset(preset_path: &Path, out_path: &Path) -> ! {
    let (egl_inst, display, config) = egl_headless::init_egl();
    let ctx = egl_headless::create_context(&egl_inst, display, config);
    let pb = egl_headless::create_pbuffer(&egl_inst, display, config, THUMB_W as i32, THUMB_H as i32);
    egl_inst.make_current(display, Some(pb), Some(pb), Some(ctx)).expect("eglMakeCurrent failed");
    let gl = egl_headless::make_gl(&egl_inst);

    let handle = unsafe { opendrop_engine::ffi::projectm_create() };
    assert!(!handle.is_null(), "projectm_create() returned NULL");

    let noise = synthetic_pcm();
    let (noisy, musical) = unsafe {
        opendrop_engine::ffi::projectm_set_window_size(handle, THUMB_W as usize, THUMB_H as usize);
        opendrop_engine::ffi::projectm_set_preset_switch_failed_event_callback(handle, Some(on_preset_failed), std::ptr::null_mut());
        let Ok(c_path) = CString::new(preset_path.to_string_lossy().as_bytes()) else { std::process::exit(EXIT_FAILED) };
        opendrop_engine::ffi::projectm_load_preset_file(handle, c_path.as_ptr(), false);

        let noisy = render_phase(handle, &gl, NOISE_PHASE_FRAMES, &|f| pulsing_noise_pcm(&noise, f));
        let musical = render_phase(handle, &gl, MUSIC_PHASE_FRAMES, &synth_music_pcm);
        (noisy, musical)
    };

    if PRESET_FAILED.load(Ordering::SeqCst) {
        std::process::exit(EXIT_FAILED);
    }
    let analysis = match (noisy, musical) {
        (Some(a), Some(b)) => Some(PresetAnalysis::best_of(a, b)),
        (a, b) => a.or(b),
    };
    let Some(analysis) = analysis else { std::process::exit(EXIT_FAILED) };
    match write_atomically(out_path, analysis.to_line().as_bytes()) {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("[analyze-preset] {e}");
            std::process::exit(EXIT_FAILED);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn a_chunk_is_stereo_bounded_and_deterministic() {
        let chunk = synth_music_pcm(3);
        assert_eq!(chunk.len(), FRAMES_PER_CHUNK * 2);
        assert!(chunk.iter().all(|s| (-1.0..=1.0).contains(s)));
        assert_eq!(chunk, synth_music_pcm(3));
        assert!(chunk.chunks(2).all(|lr| lr[0] == lr[1]), "left and right match");
    }

    #[test]
    fn the_noise_signal_pulses_on_the_same_cadence() {
        let base = vec![1.0_f32; 8];
        assert_eq!(pulsing_noise_pcm(&base, 0), base);
        assert_eq!(pulsing_noise_pcm(&base, 10), vec![0.25; 8]);
        assert_eq!(pulsing_noise_pcm(&base, 15), base);
    }

    #[test]
    fn the_beat_frames_are_louder_than_the_gaps_between_them() {
        let on_beat = rms(&synth_music_pcm(15));
        let between = rms(&synth_music_pcm(20));
        assert!(on_beat > between * 1.5, "on beat {on_beat}, between {between}");
    }
}
