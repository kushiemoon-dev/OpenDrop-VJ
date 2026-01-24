//! OpenDrop Renderer - Standalone visualization window
//!
//! This is a separate process to work around winit's EventLoop limitations.
//! Communication with the main app is done via stdin/stdout JSON messages.

use std::ffi::CString;
use std::io::{self, BufRead, Write};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

use projectm_rs::ProjectM;

// Video output support
#[cfg(target_os = "linux")]
use opendrop_core::video::{V4l2Config, V4l2Output, VideoOutput};

#[cfg(target_os = "windows")]
use opendrop_core::video::{SpoutConfig, SpoutOutput, VideoOutput};

// NDI output (cross-platform)
use opendrop_core::video::{NdiConfig, NdiOutput};

/// Commands received from the parent process via stdin
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Command {
    #[serde(rename = "load_preset")]
    LoadPreset { path: String },
    #[serde(rename = "audio")]
    Audio { samples: Vec<f32> },
    #[serde(rename = "toggle_fullscreen")]
    ToggleFullscreen,
    #[serde(rename = "set_beat_sensitivity")]
    SetBeatSensitivity { value: f32 },
    #[serde(rename = "set_video_output")]
    SetVideoOutput {
        enabled: bool,
        #[serde(default)]
        device_path: Option<String>,
    },
    #[serde(rename = "set_ndi_output")]
    SetNdiOutput {
        enabled: bool,
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(rename = "set_texture_paths")]
    SetTexturePaths { paths: Vec<String> },
    #[serde(rename = "stop")]
    Stop,
}

/// Events sent to the parent process via stdout
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum Event {
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "preset_loaded")]
    PresetLoaded { path: String },
}

/// Configuration passed via command line
#[derive(Debug, Deserialize)]
struct Config {
    width: u32,
    height: u32,
    preset_path: Option<String>,
    fullscreen: bool,
    #[serde(default)]
    deck_id: u8,
    /// Monitor index for fullscreen (0 = primary)
    #[serde(default)]
    monitor_index: Option<usize>,
    /// Texture search paths for presets that reference external textures
    #[serde(default)]
    texture_paths: Vec<String>,
}

fn send_event(event: Event) {
    if let Ok(json) = serde_json::to_string(&event) {
        let mut stdout = io::stdout().lock();
        let _ = writeln!(stdout, "{}", json);
        let _ = stdout.flush();
    }
}

/// Render application state
struct RenderApp {
    config: Config,
    command_rx: Receiver<Command>,
    gl_context: Option<PossiblyCurrentContext>,
    gl_surface: Option<Surface<WindowSurface>>,
    window: Option<Window>,
    projectm: Option<ProjectM>,
    should_exit: bool,
    // Video output state (platform-specific)
    #[cfg(target_os = "linux")]
    video_output: Option<V4l2Output>,
    #[cfg(target_os = "windows")]
    video_output: Option<SpoutOutput>,
    // NDI output (cross-platform)
    ndi_output: Option<NdiOutput>,
    /// Pixel buffer for frame capture (RGBA)
    pixel_buffer: Vec<u8>,
    /// Current framebuffer dimensions for capture
    capture_width: u32,
    capture_height: u32,
}

impl RenderApp {
    fn new(config: Config, command_rx: Receiver<Command>) -> Self {
        Self {
            config,
            command_rx,
            gl_context: None,
            gl_surface: None,
            window: None,
            projectm: None,
            should_exit: false,
            #[cfg(target_os = "linux")]
            video_output: None,
            #[cfg(target_os = "windows")]
            video_output: None,
            ndi_output: None,
            pixel_buffer: Vec::new(),
            capture_width: 0,
            capture_height: 0,
        }
    }

    /// Enable or disable video output to v4l2loopback
    #[cfg(target_os = "linux")]
    fn set_video_output(&mut self, enabled: bool, device_path: Option<String>) {
        if enabled {
            let path = device_path
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/dev/video10"));

            // Get current window size
            let (width, height) = if let Some(ref window) = self.window {
                let size = window.inner_size();
                (size.width, size.height)
            } else {
                (self.config.width, self.config.height)
            };

            let config = V4l2Config {
                device_path: path.clone(),
                width,
                height,
            };

            match V4l2Output::new(config) {
                Ok(output) => {
                    info!("Video output enabled: {:?} ({}x{})", path, width, height);
                    self.video_output = Some(output);
                    self.capture_width = width;
                    self.capture_height = height;
                    // Allocate pixel buffer (RGBA, 4 bytes per pixel)
                    self.pixel_buffer = vec![0u8; (width * height * 4) as usize];
                }
                Err(e) => {
                    error!("Failed to enable video output: {}", e);
                    send_event(Event::Error {
                        message: format!("Video output error: {}", e),
                    });
                }
            }
        } else {
            info!("Video output disabled");
            self.video_output = None;
            self.pixel_buffer.clear();
        }
    }

    /// Enable or disable video output to Spout (Windows)
    #[cfg(target_os = "windows")]
    fn set_video_output(&mut self, enabled: bool, device_path: Option<String>) {
        if enabled {
            // device_path is ignored for Spout, but we can use it as sender name
            let sender_name = device_path
                .map(|p| p.replace("Spout:", ""))
                .unwrap_or_else(|| format!("OpenDrop Deck {}", self.config.deck_id + 1));

            // Get current window size
            let (width, height) = if let Some(ref window) = self.window {
                let size = window.inner_size();
                (size.width, size.height)
            } else {
                (self.config.width, self.config.height)
            };

            let config = SpoutConfig {
                sender_name: sender_name.clone(),
                width,
                height,
            };

            match SpoutOutput::new(config) {
                Ok(output) => {
                    info!("Spout output enabled: {} ({}x{})", sender_name, width, height);
                    self.video_output = Some(output);
                    self.capture_width = width;
                    self.capture_height = height;
                    // Allocate pixel buffer (RGBA, 4 bytes per pixel)
                    self.pixel_buffer = vec![0u8; (width * height * 4) as usize];
                }
                Err(e) => {
                    error!("Failed to enable Spout output: {}", e);
                    send_event(Event::Error {
                        message: format!("Spout output error: {}", e),
                    });
                }
            }
        } else {
            info!("Spout output disabled");
            self.video_output = None;
            self.pixel_buffer.clear();
        }
    }

    /// Stub for other platforms (macOS, etc.)
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn set_video_output(&mut self, enabled: bool, _device_path: Option<String>) {
        if enabled {
            warn!("Video output not supported on this platform");
            send_event(Event::Error {
                message: "Video output not supported on this platform".to_string(),
            });
        }
    }

    /// Enable or disable NDI output (cross-platform)
    fn set_ndi_output(&mut self, enabled: bool, name: Option<String>) {
        if enabled {
            // Check if NDI is available
            if !NdiOutput::is_available() {
                warn!("NDI runtime not found. Install NDI Tools from https://ndi.video/tools/");
                send_event(Event::Error {
                    message: "NDI runtime not installed. Get it from https://ndi.video/tools/".to_string(),
                });
                return;
            }

            let sender_name = name.unwrap_or_else(|| format!("OpenDrop Deck {}", self.config.deck_id + 1));

            // Get current window size
            let (width, height) = if let Some(ref window) = self.window {
                let size = window.inner_size();
                (size.width, size.height)
            } else {
                (self.config.width, self.config.height)
            };

            let config = NdiConfig {
                name: sender_name.clone(),
                groups: None,
                clock_video: true,
            };

            match NdiOutput::with_config(config) {
                Ok(mut output) => {
                    output.set_active(true);
                    info!("NDI output enabled: {} ({}x{})", sender_name, width, height);
                    self.ndi_output = Some(output);
                    // Ensure pixel buffer is allocated
                    if self.pixel_buffer.is_empty() {
                        self.capture_width = width;
                        self.capture_height = height;
                        self.pixel_buffer = vec![0u8; (width * height * 4) as usize];
                    }
                }
                Err(e) => {
                    error!("Failed to enable NDI output: {}", e);
                    send_event(Event::Error {
                        message: format!("NDI output error: {}", e),
                    });
                }
            }
        } else {
            info!("NDI output disabled");
            if let Some(mut output) = self.ndi_output.take() {
                output.set_active(false);
            }
        }
    }

    /// Capture current framebuffer to pixel buffer
    fn capture_frame(&mut self) {
        // Early exit if no video output configured
        #[cfg(target_os = "linux")]
        let has_platform_output = self.video_output.is_some();
        #[cfg(target_os = "windows")]
        let has_platform_output = self.video_output.is_some();
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        let has_platform_output = false;

        let has_ndi_output = self.ndi_output.is_some();
        let has_output = has_platform_output || has_ndi_output;

        if !has_output || self.pixel_buffer.is_empty() {
            return;
        }

        // Read pixels from framebuffer
        unsafe {
            gl::ReadPixels(
                0,
                0,
                self.capture_width as i32,
                self.capture_height as i32,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                self.pixel_buffer.as_mut_ptr() as *mut _,
            );
        }

        // Flip vertically (OpenGL has origin at bottom-left)
        let row_size = (self.capture_width * 4) as usize;
        let half_height = self.capture_height as usize / 2;
        for y in 0..half_height {
            let top_start = y * row_size;
            let bottom_start = (self.capture_height as usize - 1 - y) * row_size;
            for x in 0..row_size {
                self.pixel_buffer.swap(top_start + x, bottom_start + x);
            }
        }

        // Send to video output (Linux - v4l2loopback)
        #[cfg(target_os = "linux")]
        if let Some(ref mut output) = self.video_output {
            if let Err(e) = output.send_frame_rgba(
                &self.pixel_buffer,
                self.capture_width,
                self.capture_height,
            ) {
                // Don't spam errors, just log occasionally
                debug!("Video output frame error: {}", e);
            }
        }

        // Send to video output (Windows - Spout)
        #[cfg(target_os = "windows")]
        if let Some(ref mut output) = self.video_output {
            if let Err(e) = output.send_frame_rgba(
                &self.pixel_buffer,
                self.capture_width,
                self.capture_height,
            ) {
                // Don't spam errors, just log occasionally
                debug!("Spout output frame error: {}", e);
            }
        }

        // Send to NDI output (cross-platform)
        if let Some(ref mut output) = self.ndi_output {
            if let Err(e) = output.send_frame_rgba(
                &self.pixel_buffer,
                self.capture_width,
                self.capture_height,
            ) {
                // Don't spam errors, just log occasionally
                debug!("NDI output frame error: {}", e);
            }
        }
    }

    fn process_commands(&mut self, event_loop: &ActiveEventLoop) {
        loop {
            match self.command_rx.try_recv() {
                Ok(cmd) => match cmd {
                    Command::LoadPreset { path } => {
                        if let Some(ref mut pm) = self.projectm {
                            match pm.load_preset(&path, true) {
                                Ok(()) => {
                                    info!("Loaded preset: {}", path);
                                    send_event(Event::PresetLoaded { path });
                                }
                                Err(e) => {
                                    error!("Failed to load preset: {}", e);
                                    send_event(Event::Error {
                                        message: e.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    Command::Audio { samples } => {
                        if let Some(ref mut pm) = self.projectm {
                            pm.add_pcm_stereo(&samples);
                        }
                    }
                    Command::ToggleFullscreen => {
                        if let Some(ref window) = self.window {
                            let is_fullscreen = window.fullscreen().is_some();
                            if is_fullscreen {
                                window.set_fullscreen(None);
                            } else {
                                window.set_fullscreen(Some(
                                    winit::window::Fullscreen::Borderless(None),
                                ));
                            }
                        }
                    }
                    Command::SetBeatSensitivity { value } => {
                        if let Some(ref mut pm) = self.projectm {
                            pm.set_beat_sensitivity(value);
                        }
                    }
                    Command::SetVideoOutput { enabled, device_path } => {
                        self.set_video_output(enabled, device_path);
                    }
                    Command::SetNdiOutput { enabled, name } => {
                        self.set_ndi_output(enabled, name);
                    }
                    Command::SetTexturePaths { paths } => {
                        if let Some(ref mut pm) = self.projectm {
                            let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
                            pm.set_texture_search_paths(&path_refs);
                            info!("Set {} texture search paths", paths.len());
                        }
                    }
                    Command::Stop => {
                        self.should_exit = true;
                        event_loop.exit();
                        return;
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // Parent process closed, exit
                    self.should_exit = true;
                    event_loop.exit();
                    return;
                }
            }
        }
    }

    fn render(&mut self) {
        // Render projectM frame
        if let Some(ref mut pm) = self.projectm {
            pm.render_frame();
        }

        // Capture frame for video output (before swap)
        self.capture_frame();

        // Swap buffers
        if let (Some(ref surface), Some(ref context)) = (&self.gl_surface, &self.gl_context) {
            if let Err(e) = surface.swap_buffers(context) {
                error!("Failed to swap buffers: {}", e);
            }
        }
    }

    fn handle_resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        // Check both surface AND context are initialized - prevents crash on Windows
        let (Some(ref surface), Some(ref context)) = (&self.gl_surface, &self.gl_context) else {
            return; // Not ready yet, skip resize
        };

        if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
            surface.resize(context, w, h);
        }

        if let Some(ref mut pm) = self.projectm {
            pm.resize(size.width, size.height);
        }

        unsafe {
            gl::Viewport(0, 0, size.width as i32, size.height as i32);
        }
    }
}

impl ApplicationHandler for RenderApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        info!(
            "Creating render window {}x{}",
            self.config.width, self.config.height
        );

        let window_title = format!("OpenDrop - Deck {}", self.config.deck_id + 1);
        let window_attrs = WindowAttributes::default()
            .with_title(window_title)
            .with_inner_size(LogicalSize::new(self.config.width, self.config.height));

        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_depth_size(24)
            .with_stencil_size(8);

        let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));

        let (window, gl_config) = match display_builder.build(event_loop, template, |configs| {
            configs
                .reduce(|accum, config| {
                    if config.num_samples() > accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        }) {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to create window: {}", e);
                send_event(Event::Error {
                    message: e.to_string(),
                });
                event_loop.exit();
                return;
            }
        };

        let window = window.expect("Window should be created");
        let raw_window_handle = window.window_handle().ok().map(|h| h.as_raw());

        let gl_display = gl_config.display();
        let context_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(raw_window_handle);

        let not_current_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attrs)
                .expect("Failed to create OpenGL context")
        };

        let attrs = window
            .build_surface_attributes(SurfaceAttributesBuilder::new())
            .expect("Failed to build surface attributes");

        let surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &attrs)
                .expect("Failed to create window surface")
        };

        let context = not_current_context
            .make_current(&surface)
            .expect("Failed to make context current");

        gl::load_with(|s| {
            let c_str = CString::new(s).unwrap();
            gl_display.get_proc_address(&c_str) as *const _
        });

        // Vsync
        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        unsafe {
            let version_ptr = gl::GetString(gl::VERSION);
            let renderer_ptr = gl::GetString(gl::RENDERER);

            if !version_ptr.is_null() && !renderer_ptr.is_null() {
                let version = std::ffi::CStr::from_ptr(version_ptr as *const _);
                let renderer = std::ffi::CStr::from_ptr(renderer_ptr as *const _);
                info!("OpenGL version: {:?}", version);
                info!("OpenGL renderer: {:?}", renderer);
            } else {
                warn!("Failed to get OpenGL version/renderer strings (null pointer)");
            }
        }

        // Create projectM
        let size = window.inner_size();
        match ProjectM::new(size.width, size.height) {
            Ok(mut pm) => {
                info!("ProjectM {} initialized", ProjectM::version());

                // Set texture search paths before loading preset
                if !self.config.texture_paths.is_empty() {
                    let path_refs: Vec<&str> =
                        self.config.texture_paths.iter().map(|s| s.as_str()).collect();
                    pm.set_texture_search_paths(&path_refs);
                    info!("Set {} texture search paths from config", self.config.texture_paths.len());
                }

                if let Some(ref preset_path) = self.config.preset_path {
                    if let Err(e) = pm.load_preset(preset_path, false) {
                        warn!("Failed to load initial preset: {}", e);
                    }
                }

                self.projectm = Some(pm);
            }
            Err(e) => {
                error!("Failed to create ProjectM instance: {}", e);
                send_event(Event::Error {
                    message: e.to_string(),
                });
            }
        }

        // Set fullscreen if requested
        if self.config.fullscreen {
            // Get the target monitor
            let monitor = if let Some(index) = self.config.monitor_index {
                event_loop
                    .available_monitors()
                    .nth(index)
            } else {
                None // Use primary (Borderless(None) means primary)
            };

            window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(monitor)));
        }

        self.gl_context = Some(context);
        self.gl_surface = Some(surface);
        self.window = Some(window);

        send_event(Event::Ready);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Window close requested");
                send_event(Event::Closed);
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                debug!("Window resized to {:?}", size);
                self.handle_resize(size);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: key,
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match key {
                Key::Named(NamedKey::Escape) => {
                    info!("Escape pressed, closing window");
                    send_event(Event::Closed);
                    event_loop.exit();
                }
                Key::Named(NamedKey::F11) => {
                    if let Some(ref window) = self.window {
                        let is_fullscreen = window.fullscreen().is_some();
                        if is_fullscreen {
                            window.set_fullscreen(None);
                        } else {
                            window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                        }
                    }
                }
                Key::Character(ref c) if c == "f" => {
                    if let Some(ref window) = self.window {
                        let is_fullscreen = window.fullscreen().is_some();
                        if is_fullscreen {
                            window.set_fullscreen(None);
                        } else {
                            window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                        }
                    }
                }
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                self.render();
                if let Some(ref window) = self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.process_commands(event_loop);
        if !self.should_exit {
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
        }
    }
}

/// Read commands from stdin in a separate thread
fn spawn_stdin_reader(tx: Sender<Command>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) if !line.is_empty() => {
                    match serde_json::from_str::<Command>(&line) {
                        Ok(cmd) => {
                            if tx.send(cmd).is_err() {
                                break; // Channel closed
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to parse command: {}", e);
                        }
                    }
                }
                Ok(_) => {} // Empty line
                Err(_) => break, // stdin closed
            }
        }
    });
}

fn main() {
    // Initialize logging to stderr (stdout is for IPC)
    tracing_subscriber::fmt()
        .with_env_filter("opendrop_renderer=debug,projectm_rs=debug")
        .with_writer(io::stderr)
        .init();

    // Parse config from command line argument
    let args: Vec<String> = std::env::args().collect();
    let config: Config = if args.len() > 1 {
        serde_json::from_str(&args[1]).unwrap_or_else(|e| {
            eprintln!("Failed to parse config: {}", e);
            Config {
                width: 1280,
                height: 720,
                preset_path: None,
                fullscreen: false,
                deck_id: 0,
                monitor_index: None,
                texture_paths: Vec::new(),
            }
        })
    } else {
        Config {
            width: 1280,
            height: 720,
            preset_path: None,
            fullscreen: false,
            deck_id: 0,
            monitor_index: None,
            texture_paths: Vec::new(),
        }
    };

    info!("Starting renderer with config: {:?}", config);

    // Create command channel
    let (command_tx, command_rx) = mpsc::channel();

    // Start stdin reader thread
    spawn_stdin_reader(command_tx);

    // Create and run event loop
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = RenderApp::new(config, command_rx);

    if let Err(e) = event_loop.run_app(&mut app) {
        error!("Event loop error: {}", e);
        send_event(Event::Error {
            message: e.to_string(),
        });
    }

    send_event(Event::Closed);
}
