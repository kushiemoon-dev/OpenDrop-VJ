//! Out-of-process preset analysis for the library scan. Re-invoked as
//! `--analyze-preset <preset-path> <out-path>`, it renders the preset for a
//! short while against pulsing synthetic audio, samples a few frames and
//! writes the measured brightness and motion (`PresetAnalysis::to_line`) to
//! `out-path`. Same isolation as the thumbnail child: one preset that
//! crashes projectM takes down this process only, which the parent sees as
//! a non-zero exit.

use std::ffi::CString;
use std::path::Path;
use std::sync::atomic::Ordering;

use opendrop_core::preset_analysis::{analyze_frames, downsample_luma};
use opendrop_engine::thumbnail::{synthetic_pcm, THUMB_H, THUMB_W, WARMUP_FRAMES};

use crate::egl_headless;
use crate::thumbnail_child::{on_preset_failed, read_back, write_atomically, PRESET_FAILED};

/// Exit code for "no usable result": the preset was rejected or the result
/// could not be written. The parent treats every non-zero exit alike.
const EXIT_FAILED: i32 = 1;
/// Frames sampled after the warm-up, one every `SAMPLE_EVERY` frames.
const SAMPLES: usize = 8;
const SAMPLE_EVERY: usize = 6;

/// Audio level for frame `i`: a short loud pulse every 15 frames over a quiet
/// bed, so an audio-reactive preset is shown something beat-like.
fn pulse_gain(frame: usize) -> f32 {
    if frame % 15 < 4 {
        1.0
    } else {
        0.25
    }
}

/// Analyses `preset_path`, writes the result to `out_path` and exits: 0 once
/// the file is written, non-zero otherwise. Never returns.
pub fn run_analyze_preset(preset_path: &Path, out_path: &Path) -> ! {
    let (egl_inst, display, config) = egl_headless::init_egl();
    let ctx = egl_headless::create_context(&egl_inst, display, config);
    let pb = egl_headless::create_pbuffer(&egl_inst, display, config, THUMB_W as i32, THUMB_H as i32);
    egl_inst.make_current(display, Some(pb), Some(pb), Some(ctx)).expect("eglMakeCurrent failed");
    let gl = egl_headless::make_gl(&egl_inst);

    let handle = unsafe { opendrop_engine::ffi::projectm_create() };
    assert!(!handle.is_null(), "projectm_create() returned NULL");

    let base_pcm = synthetic_pcm();
    let mut frames: Vec<Vec<f32>> = Vec::with_capacity(SAMPLES);
    unsafe {
        opendrop_engine::ffi::projectm_set_window_size(handle, THUMB_W as usize, THUMB_H as usize);
        opendrop_engine::ffi::projectm_set_preset_switch_failed_event_callback(handle, Some(on_preset_failed), std::ptr::null_mut());
        let Ok(c_path) = CString::new(preset_path.to_string_lossy().as_bytes()) else { std::process::exit(EXIT_FAILED) };
        opendrop_engine::ffi::projectm_load_preset_file(handle, c_path.as_ptr(), false);

        let total = WARMUP_FRAMES + 1 + SAMPLES * SAMPLE_EVERY;
        for frame in 0..total {
            let gain = pulse_gain(frame);
            let pcm: Vec<f32> = base_pcm.iter().map(|s| s * gain).collect();
            opendrop_engine::ffi::projectm_pcm_add_float(
                handle,
                pcm.as_ptr(),
                (pcm.len() / 2) as u32,
                opendrop_engine::ffi::projectm_channels_PROJECTM_STEREO,
            );
            let before = opendrop_engine::gl_state::save(&gl);
            opendrop_engine::ffi::projectm_opengl_render_frame(handle);
            opendrop_engine::gl_state::restore(&gl, &before);

            let past_warmup = frame > WARMUP_FRAMES;
            if past_warmup && (frame - WARMUP_FRAMES).is_multiple_of(SAMPLE_EVERY) {
                let pixels = read_back(&gl);
                frames.push(downsample_luma(&pixels, THUMB_W as usize, THUMB_H as usize));
            }
        }
    }

    if PRESET_FAILED.load(Ordering::SeqCst) {
        std::process::exit(EXIT_FAILED);
    }
    let Some(analysis) = analyze_frames(&frames) else { std::process::exit(EXIT_FAILED) };
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

    #[test]
    fn the_audio_pulses_every_fifteen_frames() {
        let loud: Vec<usize> = (0..45).filter(|&f| pulse_gain(f) == 1.0).collect();
        assert_eq!(loud, vec![0, 1, 2, 3, 15, 16, 17, 18, 30, 31, 32, 33]);
    }
}
