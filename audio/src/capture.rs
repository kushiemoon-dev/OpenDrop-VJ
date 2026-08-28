//! Dedicated capture thread: opens the input device, runs the cpal stream,
//! and publishes each captured block's PCM + energy via `arc-swap`.

use crate::analysis::{Analyser, AnalyserConfig};
use crate::AudioSnapshot;
use arc_swap::ArcSwap;
use cpal::Sample;
use std::sync::Arc;

pub(crate) fn spawn(snapshot: Arc<ArcSwap<AudioSnapshot>>) {
    std::thread::spawn(move || run(snapshot));
}

fn run(snapshot: Arc<ArcSwap<AudioSnapshot>>) {
    let host = cpal::default_host();
    let Some(device) = crate::device::select_input_device(&host) else {
        eprintln!("[audio] no input device available: visuals will not react to system audio");
        return;
    };
    match build_stream(&device, snapshot) {
        Ok(stream) => {
            use cpal::traits::StreamTrait;
            if let Err(e) = stream.play() {
                eprintln!("[audio] failed to start capture stream: {e}: visuals will not react to system audio");
                return;
            }
            loop {
                std::thread::park(); // keeps `stream` alive; the callback runs on cpal's own internal thread
            }
        }
        Err(e) => eprintln!("[audio] failed to open input device: {e}: visuals will not react to system audio"),
    }
}

fn build_stream(device: &cpal::Device, snapshot: Arc<ArcSwap<AudioSnapshot>>) -> Result<cpal::Stream, String> {
    use cpal::traits::DeviceTrait;
    use cpal::SampleFormat;
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let analyser = Analyser::new(AnalyserConfig::default());
    let err_fn = |e| eprintln!("[audio] stream error: {e}");
    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(config, on_block::<f32>(channels, analyser, snapshot), err_fn, None),
        SampleFormat::I16 => device.build_input_stream(config, on_block::<i16>(channels, analyser, snapshot), err_fn, None),
        SampleFormat::I32 => device.build_input_stream(config, on_block::<i32>(channels, analyser, snapshot), err_fn, None),
        SampleFormat::I8 => device.build_input_stream(config, on_block::<i8>(channels, analyser, snapshot), err_fn, None),
        other => return Err(format!("unsupported sample format: {other}")),
    };
    stream.map_err(|e| e.to_string())
}

fn on_block<T>(
    channels: usize,
    mut analyser: Analyser,
    snapshot: Arc<ArcSwap<AudioSnapshot>>,
) -> impl FnMut(&[T], &cpal::InputCallbackInfo) + Send + 'static
where
    f32: cpal::FromSample<T>,
    T: cpal::Sample + Copy,
{
    move |data: &[T], _| {
        let as_f32: Vec<f32> = data.iter().map(|&s| f32::from_sample(s)).collect();
        let mono = downmix_to_mono(&as_f32, channels);
        let energy_byte = analyser.process(&mono);
        let pcm = normalize_to_stereo(&as_f32, channels);
        snapshot.store(Arc::new(AudioSnapshot { pcm, energy_byte }));
    }
}

/// Per-frame channel average: a mono signal for the FFT/energy calc only,
/// distinct from the stereo PCM published to the decks.
fn downmix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels == 0 {
        return Vec::new();
    }
    interleaved.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect()
}

/// Normalizes to interleaved stereo for `render_frame()`: mono is duplicated
/// L=R; stereo is unchanged; >2 channels keeps only the first 2 (an
/// implementation choice, not a behavior contract).
fn normalize_to_stereo(interleaved: &[f32], channels: usize) -> Vec<f32> {
    match channels {
        1 => interleaved.iter().flat_map(|&s| [s, s]).collect(),
        2 => interleaved.to_vec(),
        _ => interleaved.chunks_exact(channels).flat_map(|frame| [frame[0], frame[1]]).collect(),
    }
}
