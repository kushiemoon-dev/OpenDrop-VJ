//! Audio capture using CPAL (and PipeWire on Linux)
//!
//! Captures audio from system input devices and distributes it to visualization decks.
//! On Linux, can use native PipeWire for monitor devices instead of parec subprocess.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

// CPAL is used on Windows/macOS, but not on Linux (we use parec)
#[cfg(not(target_os = "linux"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(not(target_os = "linux"))]
use cpal::{Device, SampleFormat, Stream, StreamConfig};
#[cfg(target_os = "linux")]
use cpal::SampleFormat;

// WASAPI host ID for explicit Windows audio handling
#[cfg(target_os = "windows")]
use cpal::HostId;
use thiserror::Error;
use tracing::{debug, error, info, warn};

// PipeWire native capture is available but we use parec for stability
#[cfg(target_os = "linux")]
#[allow(unused_imports)]
use super::pipewire::{PipeWireCapture, PipeWireConfig};

#[derive(Error, Debug)]
pub enum AudioError {
    #[error("No input device available")]
    NoInputDevice,
    #[error("Failed to get default config: {0}")]
    ConfigError(String),
    #[error("Stream error: {0}")]
    StreamError(String),
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Unsupported sample format: {0:?}")]
    UnsupportedFormat(SampleFormat),
    #[error("Channel error: {0}")]
    ChannelError(String),
}

/// Audio capture configuration
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Target sample rate (will resample if needed)
    pub sample_rate: u32,
    /// Number of channels (1 or 2)
    pub channels: u16,
    /// Buffer size in samples
    pub buffer_size: usize,
    /// Device name (None for default)
    pub device_name: Option<String>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            buffer_size: 1024,
            device_name: None,
        }
    }
}

/// Device direction/type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    /// Input device (microphone)
    Input,
    /// Output device (speakers) - can be used for loopback capture
    Output,
    /// Monitor device (Linux PulseAudio/PipeWire monitor)
    Monitor,
}

/// Information about an audio device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device name/identifier
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Whether this is the default device
    pub is_default: bool,
    /// Whether this is a monitor (captures output audio)
    pub is_monitor: bool,
    /// Device type (input, output, monitor)
    pub device_type: DeviceType,
    /// Capture backend to use
    pub backend: AudioBackend,
}

/// Audio capture backend
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioBackend {
    /// CPAL (cross-platform)
    Cpal,
    /// Native PipeWire (Linux only)
    #[cfg(target_os = "linux")]
    PipeWire,
    /// PulseAudio parec (Linux fallback)
    #[cfg(target_os = "linux")]
    PulseAudio,
}

/// Commands sent to the audio thread
#[derive(Debug)]
enum AudioCommand {
    Stop,
}

/// Audio engine handle - manages audio capture in a separate thread
///
/// This struct is Send + Sync safe because it only holds channels.
pub struct AudioEngine {
    /// Channel to send commands to the audio thread
    command_tx: Option<Sender<AudioCommand>>,
    /// Channel to receive audio samples from the audio thread
    sample_rx: Option<Receiver<Vec<f32>>>,
    /// Thread handle
    thread_handle: Option<JoinHandle<()>>,
    /// Whether the engine is running
    running: bool,
}

impl AudioEngine {
    /// Create a new audio engine (not started yet)
    pub fn new() -> Self {
        Self {
            command_tx: None,
            sample_rx: None,
            thread_handle: None,
            running: false,
        }
    }

    /// List available audio input devices
    pub fn list_devices() -> Vec<DeviceInfo> {
        let mut devices: Vec<DeviceInfo> = Vec::new();

        // On Linux, use pactl to list monitors (captures system audio)
        // Don't use CPAL ALSA devices - they cause panics and block system audio
        #[cfg(target_os = "linux")]
        {
            // Add "System Audio (Auto)" as first option - will auto-detect monitor
            devices.push(DeviceInfo {
                name: "auto".to_string(),
                description: "System Audio (Auto-detect)".to_string(),
                is_default: true,
                is_monitor: true,
                device_type: DeviceType::Monitor,
                backend: AudioBackend::PulseAudio,
            });

            // List monitors from pactl (works with both PipeWire and PulseAudio)
            if let Ok(output) = std::process::Command::new("pactl")
                .args(["list", "sources", "short"])
                .output()
            {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    debug!("pactl sources output: {}", stdout);
                    for line in stdout.lines() {
                        let parts: Vec<&str> = line.split('\t').collect();
                        if parts.len() >= 2 {
                            let name = parts[1].to_string();
                            // Only add monitor devices (they capture what you hear)
                            if name.contains(".monitor") {
                                // Get a cleaner description
                                let description = name
                                    .replace("alsa_output.", "")
                                    .replace(".monitor", "")
                                    .replace("_", " ")
                                    .replace("-", " ")
                                    .replace(".analog stereo", " (Stereo)");

                                devices.push(DeviceInfo {
                                    name: name.clone(),
                                    description: format!("{} (Monitor)", description),
                                    is_default: false,
                                    is_monitor: true,
                                    device_type: DeviceType::Monitor,
                                    backend: AudioBackend::PulseAudio,
                                });
                            }
                        }
                    }
                }
            }

            info!("Found {} audio devices on Linux", devices.len());
        }

        // On Windows/macOS, use CPAL - list both input and output devices
        #[cfg(not(target_os = "linux"))]
        {
            let host = cpal::default_host();

            // Get default device names for marking
            let default_input_name = host
                .default_input_device()
                .and_then(|d| d.name().ok());
            let default_output_name = host
                .default_output_device()
                .and_then(|d| d.name().ok());

            // List input devices (microphones)
            if let Ok(input_devices) = host.input_devices() {
                for device in input_devices {
                    if let Ok(name) = device.name() {
                        devices.push(DeviceInfo {
                            description: format!("{} (Input)", name),
                            is_default: Some(&name) == default_input_name.as_ref(),
                            is_monitor: false,
                            device_type: DeviceType::Input,
                            backend: AudioBackend::Cpal,
                            name,
                        });
                    }
                }
            }

            // List output devices (speakers/headphones) for loopback capture
            // On Windows, WASAPI allows capturing from output devices (loopback)
            #[cfg(target_os = "windows")]
            {
                if let Ok(output_devices) = host.output_devices() {
                    for device in output_devices {
                        if let Ok(name) = device.name() {
                            // Mark as loopback device
                            devices.push(DeviceInfo {
                                description: format!("{} (Loopback)", name),
                                is_default: Some(&name) == default_output_name.as_ref(),
                                is_monitor: true,  // Loopback acts like a monitor
                                device_type: DeviceType::Output,
                                backend: AudioBackend::Cpal,
                                name: format!("loopback:{}", name), // Prefix to identify loopback
                            });
                        }
                    }
                }
            }

            // On macOS, loopback requires virtual audio devices (BlackHole, Loopback app)
            // We still list output devices but they won't work without virtual device software
            #[cfg(target_os = "macos")]
            {
                if let Ok(output_devices) = host.output_devices() {
                    for device in output_devices {
                        if let Ok(name) = device.name() {
                            // Check if this looks like a virtual loopback device
                            let is_virtual = name.to_lowercase().contains("blackhole")
                                || name.to_lowercase().contains("loopback")
                                || name.to_lowercase().contains("soundflower");

                            if is_virtual {
                                devices.push(DeviceInfo {
                                    description: format!("{} (Virtual)", name),
                                    is_default: false,
                                    is_monitor: true,
                                    device_type: DeviceType::Output,
                                    backend: AudioBackend::Cpal,
                                    name,
                                });
                            }
                        }
                    }
                }
            }

            info!("Found {} audio devices", devices.len());
        }

        devices
    }

    /// Find the default monitor device (Linux only)
    #[cfg(target_os = "linux")]
    pub fn find_default_monitor() -> Option<String> {
        if let Ok(output) = std::process::Command::new("pactl")
            .args(["list", "sources", "short"])
            .output()
        {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 2 {
                        let name = parts[1].to_string();
                        if name.contains(".monitor") {
                            debug!("Auto-detected monitor: {}", name);
                            return Some(name);
                        }
                    }
                }
            }
        }
        None
    }

    /// Start the audio engine
    pub fn start(&mut self, config: AudioConfig) -> Result<(), AudioError> {
        if self.running {
            warn!("Audio engine already running");
            return Ok(());
        }

        let (command_tx, command_rx) = mpsc::channel();
        let (sample_tx, sample_rx) = mpsc::channel();

        // Spawn the audio thread
        let thread_handle = thread::spawn(move || {
            if let Err(e) = run_audio_thread(config, command_rx, sample_tx) {
                error!("Audio thread error: {}", e);
            }
        });

        self.command_tx = Some(command_tx);
        self.sample_rx = Some(sample_rx);
        self.thread_handle = Some(thread_handle);
        self.running = true;

        info!("Audio engine started");
        Ok(())
    }

    /// Stop the audio engine
    pub fn stop(&mut self) {
        if let Some(tx) = self.command_tx.take() {
            let _ = tx.send(AudioCommand::Stop);
        }

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        self.sample_rx = None;
        self.running = false;

        info!("Audio engine stopped");
    }

    /// Try to receive audio samples (non-blocking)
    pub fn try_recv(&self) -> Option<Vec<f32>> {
        self.sample_rx.as_ref()?.try_recv().ok()
    }

    /// Check if the engine is running
    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run the audio capture in a separate thread
fn run_audio_thread(
    config: AudioConfig,
    command_rx: Receiver<AudioCommand>,
    sample_tx: Sender<Vec<f32>>,
) -> Result<(), AudioError> {
    // On Linux, ALWAYS use parec to avoid CPAL/ALSA panics and system audio blocking
    #[cfg(target_os = "linux")]
    {
        let device_name = config.device_name.clone().unwrap_or_else(|| "auto".to_string());

        // If "auto" or empty, find the default monitor
        let actual_device = if device_name == "auto" || device_name.is_empty() {
            AudioEngine::find_default_monitor()
                .ok_or_else(|| AudioError::NoInputDevice)?
        } else {
            device_name
        };

        info!("Linux audio capture using parec with device: {}", actual_device);
        return run_parec_capture(actual_device, command_rx, sample_tx);
    }

    // On Windows/macOS, use CPAL
    #[cfg(not(target_os = "linux"))]
    {
        // On Windows, explicitly use WASAPI host for proper loopback support
        #[cfg(target_os = "windows")]
        let host = cpal::host_from_id(HostId::Wasapi)
            .map_err(|e| AudioError::StreamError(format!("Failed to get WASAPI host: {}", e)))?;

        #[cfg(not(target_os = "windows"))]
        let host = cpal::default_host();

        info!("Using audio host: {:?}", host.id());

        // Check if this is a loopback device (Windows only)
        let (device, is_loopback) = if let Some(ref name) = config.device_name {
            if name.starts_with("loopback:") {
                // Extract the actual device name after "loopback:" prefix
                let actual_name = name.strip_prefix("loopback:").unwrap_or(name);
                info!("Looking for loopback device: {}", actual_name);

                // Find the output device for loopback capture
                let output_device = host.output_devices()
                    .map_err(|e| AudioError::StreamError(e.to_string()))?
                    .find(|d| d.name().ok().as_deref() == Some(actual_name))
                    .ok_or_else(|| AudioError::DeviceNotFound(actual_name.to_string()))?;

                (output_device, true)
            } else {
                // Regular input device
                let input_device = host.input_devices()
                    .map_err(|e| AudioError::StreamError(e.to_string()))?
                    .find(|d| d.name().ok().as_ref() == Some(name))
                    .ok_or_else(|| AudioError::DeviceNotFound(name.clone()))?;

                (input_device, false)
            }
        } else {
            // Windows: Try loopback on default output first, fallback to input
            // Most users want to visualize what they're listening to, not microphone input
            #[cfg(target_os = "windows")]
            {
                if let Some(output_device) = host.default_output_device() {
                    info!("Windows: Using default output device for loopback capture");
                    (output_device, true)
                } else if let Some(input_device) = host.default_input_device() {
                    info!("Windows: Falling back to default input device");
                    (input_device, false)
                } else {
                    return Err(AudioError::NoInputDevice);
                }
            }

            // macOS: Default to input device (loopback requires virtual audio software)
            #[cfg(not(target_os = "windows"))]
            {
                let default_device = host.default_input_device()
                    .ok_or(AudioError::NoInputDevice)?;
                (default_device, false)
            }
        };

        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        info!(
            "Using audio device: {} (loopback: {})",
            device_name, is_loopback
        );

        // Get supported config
        // For loopback, we use the output device's default output config
        let supported_config = if is_loopback {
            device
                .default_output_config()
                .map_err(|e| AudioError::ConfigError(format!("Loopback config error: {}", e)))?
        } else {
            device
                .default_input_config()
                .map_err(|e| AudioError::ConfigError(e.to_string()))?
        };

        info!(
            "Device config: {} Hz, {} channels, {:?}",
            supported_config.sample_rate(),
            supported_config.channels(),
            supported_config.sample_format()
        );

        let sample_format = supported_config.sample_format();
        let stream_config: StreamConfig = supported_config.into();

        // Build the stream based on sample format
        // Note: For loopback on Windows WASAPI, we use build_input_stream on an output device.
        // WASAPI handles loopback capture internally. Important: loopback only produces audio
        // when something is actually playing through that device.
        info!(
            "Building audio stream: format={:?}, rate={}, channels={}, loopback={}",
            sample_format,
            stream_config.sample_rate,
            stream_config.channels,
            is_loopback
        );

        let stream = match sample_format {
            SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, sample_tx, is_loopback)?,
            SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, sample_tx, is_loopback)?,
            SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, sample_tx, is_loopback)?,
            format => return Err(AudioError::UnsupportedFormat(format)),
        };

        stream.play().map_err(|e| AudioError::StreamError(e.to_string()))?;
        info!("Audio stream started successfully (loopback: {})", is_loopback);

        if is_loopback {
            info!("Note: WASAPI loopback only captures audio when something is playing through the device");
        }

        // Wait for stop command (blocks until Stop received or channel closed)
        let _ = command_rx.recv();
        info!("Audio thread stopping");

        // Stream is dropped here, stopping capture
        Ok(())
    }
}

/// Capture audio from a PulseAudio/PipeWire monitor device using parec
#[cfg(target_os = "linux")]
fn run_parec_capture(
    device_name: String,
    command_rx: Receiver<AudioCommand>,
    sample_tx: Sender<Vec<f32>>,
) -> Result<(), AudioError> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    info!("Using PulseAudio monitor device: {}", device_name);

    // Start parec to capture audio
    // Format: 32-bit float, stereo, 44100 Hz
    let mut child = Command::new("parec")
        .args([
            "--device", &device_name,
            "--format=float32le",
            "--channels=2",
            "--rate=44100",
            "--latency-msec=50",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AudioError::StreamError(format!("Failed to start parec: {}", e)))?;

    let mut stdout = child.stdout.take()
        .ok_or_else(|| AudioError::StreamError("Failed to get parec stdout".to_string()))?;

    info!("PulseAudio capture started");

    // Read audio data in chunks
    let chunk_size = 4096; // samples (2 channels * 2048 frames)
    let mut buffer = vec![0u8; chunk_size * 4]; // 4 bytes per f32

    loop {
        // Check for stop command
        match command_rx.try_recv() {
            Ok(AudioCommand::Stop) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                info!("PulseAudio capture stopping");
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        // Read audio data
        match stdout.read(&mut buffer) {
            Ok(0) => {
                // EOF - parec exited
                warn!("parec exited unexpectedly");
                break;
            }
            Ok(n) => {
                // Convert bytes to f32 samples
                let samples: Vec<f32> = buffer[..n]
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                let _ = sample_tx.send(samples);
            }
            Err(e) => {
                error!("Error reading from parec: {}", e);
                break;
            }
        }
    }

    // Proper cleanup: kill then wait to avoid zombie processes
    // drop stdout first to close the pipe, which helps parec exit cleanly
    drop(stdout);

    // Try graceful termination first, then force kill if needed
    match child.try_wait() {
        Ok(Some(_)) => {
            // Already exited
            info!("parec already exited");
        }
        Ok(None) => {
            // Still running, kill it
            info!("Killing parec process");
            let _ = child.kill();
            let _ = child.wait(); // Reap the zombie
        }
        Err(e) => {
            warn!("Error checking parec status: {}", e);
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    Ok(())
}

/// Capture audio using native PipeWire (no subprocess)
/// Note: Currently unused - using parec for better stability
#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn run_pipewire_native_capture(
    device_name: String,
    command_rx: Receiver<AudioCommand>,
    sample_tx: Sender<Vec<f32>>,
) -> Result<(), AudioError> {
    info!("Starting PipeWire native capture for: {}", device_name);

    let mut capture = PipeWireCapture::new();

    let config = PipeWireConfig {
        target: Some(device_name.clone()),
        sample_rate: 44100,
        channels: 2,
    };

    capture.start(config, sample_tx.clone())?;

    // Wait for stop command (blocks until Stop received or channel closed)
    let _ = command_rx.recv();
    info!("PipeWire native capture stopping");

    capture.stop();
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn build_stream<T>(
    device: &Device,
    config: &StreamConfig,
    tx: Sender<Vec<f32>>,
    is_loopback: bool,
) -> Result<Stream, AudioError>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    // Track callback activity for debugging
    let callback_count = Arc::new(AtomicU64::new(0));
    let callback_count_clone = callback_count.clone();
    let has_logged_first_callback = Arc::new(AtomicBool::new(false));
    let has_logged_first_clone = has_logged_first_callback.clone();

    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let count = callback_count_clone.fetch_add(1, Ordering::Relaxed);

                // Log first callback to confirm stream is working
                if !has_logged_first_clone.swap(true, Ordering::Relaxed) {
                    info!(
                        "Audio callback received first data: {} samples (loopback: {})",
                        data.len(),
                        is_loopback
                    );
                }

                // Log periodically in debug mode
                if count % 1000 == 0 && count > 0 {
                    debug!("Audio callback count: {}", count);
                }

                // Convert samples to f32
                let samples: Vec<f32> = data
                    .iter()
                    .map(|s| cpal::Sample::from_sample(*s))
                    .collect();

                // Send samples (non-blocking, drop if channel is full)
                let _ = tx.send(samples);
            },
            move |err| {
                error!("Audio stream error: {}", err);
            },
            None,
        )
        .map_err(|e| AudioError::StreamError(e.to_string()))?;

    Ok(stream)
}

// ============ Legacy types for compatibility ============

/// Legacy AudioCapture - now wraps AudioEngine
pub struct AudioCapture {
    engine: AudioEngine,
    config: AudioConfig,
}

impl AudioCapture {
    pub fn new(config: AudioConfig) -> Self {
        Self {
            engine: AudioEngine::new(),
            config,
        }
    }

    pub fn list_devices() -> Vec<DeviceInfo> {
        AudioEngine::list_devices()
    }

    pub fn config(&self) -> &AudioConfig {
        &self.config
    }

    pub fn start(&mut self) -> Result<(), AudioError> {
        self.engine.start(self.config.clone())
    }

    pub fn stop(&mut self) {
        self.engine.stop()
    }

    pub fn try_recv(&self) -> Option<Vec<f32>> {
        self.engine.try_recv()
    }

    pub fn is_running(&self) -> bool {
        self.engine.is_running()
    }
}
