//! Native PipeWire audio capture
//!
//! Captures audio directly from PipeWire without subprocess overhead.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use pipewire as pw;
use pw::context::Context;
use pw::main_loop::MainLoop;
use pw::properties::properties;
use pw::stream::{Stream, StreamFlags};

use tracing::{debug, error, info, warn};

use super::AudioError;

/// PipeWire capture configuration
#[derive(Debug, Clone)]
pub struct PipeWireConfig {
    /// Target node name (e.g., "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor")
    pub target: Option<String>,
    /// Sample rate (default: 44100)
    pub sample_rate: u32,
    /// Channels (default: 2)
    pub channels: u32,
}

impl Default for PipeWireConfig {
    fn default() -> Self {
        Self {
            target: None,
            sample_rate: 44100,
            channels: 2,
        }
    }
}

/// PipeWire audio source information
#[derive(Debug, Clone)]
pub struct PipeWireSource {
    /// Node ID
    pub id: u32,
    /// Node name (used for connection)
    pub name: String,
    /// Description (human-readable)
    pub description: String,
    /// Media class (e.g., "Audio/Source", "Audio/Sink")
    pub media_class: String,
    /// Whether this is a monitor (captures output audio)
    pub is_monitor: bool,
}

/// Commands for the PipeWire thread
enum PipeWireCommand {
    Stop,
}

/// User data for stream callbacks
struct StreamData {
    sample_tx: Sender<Vec<f32>>,
    #[allow(dead_code)]
    channels: u32, // Reserved for future format negotiation
}

/// PipeWire audio capture handle
pub struct PipeWireCapture {
    command_tx: Option<Sender<PipeWireCommand>>,
    thread_handle: Option<JoinHandle<()>>,
    running: bool,
}

impl PipeWireCapture {
    /// Create a new PipeWire capture (not started)
    pub fn new() -> Self {
        Self {
            command_tx: None,
            thread_handle: None,
            running: false,
        }
    }

    /// Check if PipeWire is available on this system
    pub fn is_available() -> bool {
        // Try to init PipeWire
        pw::init();
        true
    }

    /// List available PipeWire audio sources
    pub fn list_sources() -> Vec<PipeWireSource> {
        let mut sources = Vec::new();

        pw::init();

        // Use pw-cli or pw-dump to list sources
        // This is a workaround since the registry API is complex
        if let Ok(output) = std::process::Command::new("pw-cli")
            .args(["list-objects"])
            .output()
        {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                // Parse pw-cli output to find audio nodes
                let mut current_id: Option<u32> = None;
                let mut current_name: Option<String> = None;
                let mut current_desc: Option<String> = None;
                let mut current_class: Option<String> = None;

                for line in stdout.lines() {
                    let line = line.trim();

                    // New object starts with "id X, type PipeWire:Interface:Node"
                    if line.starts_with("id ") && line.contains("type PipeWire:Interface:Node") {
                        // Save previous if it was audio
                        if let (Some(id), Some(name), Some(class)) =
                            (current_id, current_name.take(), current_class.take())
                        {
                            if class.starts_with("Audio/") {
                                let is_monitor = name.contains(".monitor") ||
                                                 class.contains("Monitor");
                                sources.push(PipeWireSource {
                                    id,
                                    name: name.clone(),
                                    description: current_desc.take().unwrap_or(name),
                                    media_class: class,
                                    is_monitor,
                                });
                            }
                        }

                        // Parse new ID
                        if let Some(id_str) = line.split(',').next() {
                            if let Some(id_num) = id_str.strip_prefix("id ") {
                                current_id = id_num.trim().parse().ok();
                            }
                        }
                        current_name = None;
                        current_desc = None;
                        current_class = None;
                    }

                    // Parse properties
                    if line.contains("node.name") {
                        if let Some(val) = extract_property_value(line) {
                            current_name = Some(val);
                        }
                    }
                    if line.contains("node.description") {
                        if let Some(val) = extract_property_value(line) {
                            current_desc = Some(val);
                        }
                    }
                    if line.contains("media.class") {
                        if let Some(val) = extract_property_value(line) {
                            current_class = Some(val);
                        }
                    }
                }

                // Don't forget last one
                if let (Some(id), Some(name), Some(class)) =
                    (current_id, current_name.take(), current_class.take())
                {
                    if class.starts_with("Audio/") {
                        let is_monitor = name.contains(".monitor") ||
                                         class.contains("Monitor");
                        sources.push(PipeWireSource {
                            id,
                            name: name.clone(),
                            description: current_desc.take().unwrap_or(name),
                            media_class: class,
                            is_monitor,
                        });
                    }
                }
            }
        }

        // Fallback: try wpctl
        if sources.is_empty() {
            if let Ok(output) = std::process::Command::new("wpctl")
                .args(["status"])
                .output()
            {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    parse_wpctl_status(&stdout, &mut sources);
                }
            }
        }

        sources
    }

    /// Start capturing audio
    pub fn start(
        &mut self,
        config: PipeWireConfig,
        sample_tx: Sender<Vec<f32>>,
    ) -> Result<(), AudioError> {
        if self.running {
            warn!("PipeWire capture already running");
            return Ok(());
        }

        let (command_tx, command_rx) = mpsc::channel();

        let thread_handle = thread::spawn(move || {
            if let Err(e) = run_pipewire_capture(config, command_rx, sample_tx) {
                error!("PipeWire capture error: {}", e);
            }
        });

        self.command_tx = Some(command_tx);
        self.thread_handle = Some(thread_handle);
        self.running = true;

        info!("PipeWire capture started");
        Ok(())
    }

    /// Stop capturing
    pub fn stop(&mut self) {
        if let Some(tx) = self.command_tx.take() {
            let _ = tx.send(PipeWireCommand::Stop);
        }

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        self.running = false;
        info!("PipeWire capture stopped");
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl Default for PipeWireCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PipeWireCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run the PipeWire capture loop
fn run_pipewire_capture(
    config: PipeWireConfig,
    command_rx: Receiver<PipeWireCommand>,
    sample_tx: Sender<Vec<f32>>,
) -> Result<(), AudioError> {
    pw::init();

    let mainloop = MainLoop::new(None)
        .map_err(|e| AudioError::StreamError(format!("Failed to create PipeWire main loop: {}", e)))?;

    let context = Context::new(&mainloop)
        .map_err(|e| AudioError::StreamError(format!("Failed to create PipeWire context: {}", e)))?;

    let core = context
        .connect(None)
        .map_err(|e| AudioError::StreamError(format!("Failed to connect to PipeWire: {}", e)))?;

    // Build stream properties
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
    };

    // Set target node if specified (use raw property name)
    if let Some(ref target) = config.target {
        props.insert("node.target", target.as_str());
    }

    let stream = Stream::new(&core, "opendrop-capture", props)
        .map_err(|e| AudioError::StreamError(format!("Failed to create stream: {}", e)))?;

    // Stream user data
    let data = Arc::new(Mutex::new(StreamData {
        sample_tx: sample_tx.clone(),
        channels: config.channels,
    }));

    let data_clone = Arc::clone(&data);

    // Register stream listener
    let _listener = stream
        .add_local_listener_with_user_data(data_clone)
        .state_changed(|_, _, old, new| {
            debug!("PipeWire stream state: {:?} -> {:?}", old, new);
        })
        .process(|stream_ref, data| {
            // Process audio buffer inline to avoid type issues
            if let Some(mut buffer) = stream_ref.dequeue_buffer() {
                let datas = buffer.datas_mut();
                if datas.is_empty() {
                    return;
                }

                let data_guard = match data.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };

                // Get the chunk info (always present, use offset/size from it)
                let chunk = datas[0].chunk();
                let offset = chunk.offset() as usize;
                let size = chunk.size() as usize;

                if let Some(slice) = datas[0].data() {
                    if size > 0 && offset + size <= slice.len() {
                        // Convert bytes to f32 samples (assuming F32LE)
                        let samples: Vec<f32> = slice[offset..offset + size]
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect();

                        // Send samples
                        let _ = data_guard.sample_tx.send(samples);
                    }
                }
            }
        })
        .register()
        .map_err(|e| AudioError::StreamError(format!("Failed to register listener: {}", e)))?;

    // Connect without format params (let PipeWire negotiate)
    let mut params: Vec<&libspa::pod::Pod> = Vec::new();

    // Connect the stream
    stream
        .connect(
            libspa::utils::Direction::Input,
            None,
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
            &mut params[..],
        )
        .map_err(|e| AudioError::StreamError(format!("Failed to connect stream: {}", e)))?;

    info!("PipeWire stream connected, running main loop");

    // Create a PipeWire channel to signal quit from the stop check thread
    let (pw_sender, pw_receiver) = pw::channel::channel::<()>();

    // Attach the receiver to the main loop to wake it when we send a quit signal
    let mainloop_clone = mainloop.clone();
    let _channel_listener = pw_receiver.attach(mainloop.loop_(), move |_| {
        mainloop_clone.quit();
    });

    // Spawn a thread to check for stop command and signal the main loop
    let stop_check = thread::spawn(move || {
        loop {
            match command_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(PipeWireCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    info!("Signaling PipeWire main loop to quit");
                    let _ = pw_sender.send(());
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
    });

    mainloop.run();

    let _ = stop_check.join();

    Ok(())
}

/// Extract property value from pw-cli output line
fn extract_property_value(line: &str) -> Option<String> {
    // Format: "  *key = value" or "  key = value"
    let parts: Vec<&str> = line.splitn(2, '=').collect();
    if parts.len() == 2 {
        let value = parts[1].trim().trim_matches('"');
        Some(value.to_string())
    } else {
        None
    }
}

/// Parse wpctl status output
fn parse_wpctl_status(output: &str, sources: &mut Vec<PipeWireSource>) {
    let mut in_audio_section = false;
    let mut in_sources = false;
    let mut in_sinks = false;

    for line in output.lines() {
        if line.contains("Audio") {
            in_audio_section = true;
        }
        if line.contains("Video") || line.contains("Settings") {
            in_audio_section = false;
        }

        if in_audio_section {
            if line.contains("Sources:") || line.contains("Capture:") {
                in_sources = true;
                in_sinks = false;
            } else if line.contains("Sinks:") || line.contains("Playback:") {
                in_sinks = true;
                in_sources = false;
            }

            // Parse device lines: "  123. device_name [vol: X.XX]"
            if (in_sources || in_sinks) && line.contains('.') {
                let trimmed = line.trim();
                if let Some(dot_pos) = trimmed.find('.') {
                    if let Ok(id) = trimmed[..dot_pos].trim().trim_start_matches('*').parse::<u32>() {
                        let rest = &trimmed[dot_pos + 1..];
                        let name = rest.split('[').next().unwrap_or(rest).trim();
                        if !name.is_empty() {
                            let is_monitor = in_sinks; // Sinks can be monitored
                            sources.push(PipeWireSource {
                                id,
                                name: name.to_string(),
                                description: name.to_string(),
                                media_class: if in_sources {
                                    "Audio/Source".to_string()
                                } else {
                                    "Audio/Sink".to_string()
                                },
                                is_monitor,
                            });
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipewire_capture_new() {
        let capture = PipeWireCapture::new();
        assert!(!capture.is_running());
    }

    #[test]
    fn test_pipewire_config_default() {
        let config = PipeWireConfig::default();
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.channels, 2);
        assert!(config.target.is_none());
    }

    #[test]
    fn test_extract_property() {
        assert_eq!(
            extract_property_value("  node.name = \"test\""),
            Some("test".to_string())
        );
        assert_eq!(
            extract_property_value("media.class = Audio/Source"),
            Some("Audio/Source".to_string())
        );
    }
}
