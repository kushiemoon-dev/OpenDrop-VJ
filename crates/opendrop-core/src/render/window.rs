//! OpenGL window creation and rendering

use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

use projectm_rs::ProjectM;

#[derive(Error, Debug)]
pub enum RenderError {
    #[error("Failed to create window: {0}")]
    WindowCreation(String),
    #[error("Failed to create OpenGL context: {0}")]
    ContextCreation(String),
    #[error("ProjectM error: {0}")]
    ProjectM(#[from] projectm_rs::Error),
    #[error("Event loop error: {0}")]
    EventLoop(String),
}

/// Configuration for the render window
#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub fullscreen: bool,
    pub vsync: bool,
    pub preset_path: Option<String>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            title: "OpenDrop Visualization".to_string(),
            fullscreen: false,
            vsync: true,
            preset_path: None,
        }
    }
}

/// Commands that can be sent to the render thread
#[derive(Debug)]
pub enum RenderCommand {
    /// Load a preset file
    LoadPreset(String),
    /// Add audio samples (stereo interleaved f32)
    AudioData(Vec<f32>),
    /// Resize the window
    Resize(u32, u32),
    /// Toggle fullscreen
    ToggleFullscreen,
    /// Set beat sensitivity
    SetBeatSensitivity(f32),
    /// Stop the render loop
    Stop,
}

/// Events sent from the render thread
#[derive(Debug)]
pub enum RenderEvent {
    /// Window was created
    Ready,
    /// Window was closed
    Closed,
    /// Error occurred
    Error(String),
    /// Preset loaded successfully
    PresetLoaded(String),
}

/// Handle to control the render window from another thread
pub struct RenderWindow {
    command_tx: Sender<RenderCommand>,
    event_rx: Receiver<RenderEvent>,
    thread_handle: Option<JoinHandle<()>>,
}

impl RenderWindow {
    /// Create and start a new render window in a separate thread
    pub fn new(config: RenderConfig) -> Result<Self, RenderError> {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        let thread_handle = thread::spawn(move || {
            if let Err(e) = run_render_loop(config, command_rx, event_tx.clone()) {
                error!("Render loop error: {}", e);
                let _ = event_tx.send(RenderEvent::Error(e.to_string()));
            }
        });

        Ok(Self {
            command_tx,
            event_rx,
            thread_handle: Some(thread_handle),
        })
    }

    /// Send a command to the render window
    pub fn send(&self, cmd: RenderCommand) -> Result<(), RenderError> {
        self.command_tx
            .send(cmd)
            .map_err(|e| RenderError::EventLoop(e.to_string()))
    }

    /// Load a preset file
    pub fn load_preset(&self, path: impl Into<String>) -> Result<(), RenderError> {
        self.send(RenderCommand::LoadPreset(path.into()))
    }

    /// Send audio data to the visualizer
    pub fn send_audio(&self, samples: Vec<f32>) -> Result<(), RenderError> {
        self.send(RenderCommand::AudioData(samples))
    }

    /// Set beat sensitivity (0.0 to 2.0)
    pub fn set_beat_sensitivity(&self, sensitivity: f32) -> Result<(), RenderError> {
        self.send(RenderCommand::SetBeatSensitivity(sensitivity))
    }

    /// Toggle fullscreen mode
    pub fn toggle_fullscreen(&self) -> Result<(), RenderError> {
        self.send(RenderCommand::ToggleFullscreen)
    }

    /// Stop the render window
    pub fn stop(&self) -> Result<(), RenderError> {
        self.send(RenderCommand::Stop)
    }

    /// Try to receive an event without blocking
    pub fn try_recv_event(&self) -> Option<RenderEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Wait for the render thread to finish
    pub fn join(mut self) {
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RenderWindow {
    fn drop(&mut self) {
        let _ = self.command_tx.send(RenderCommand::Stop);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// State for the render application
struct RenderApp {
    config: RenderConfig,
    command_rx: Receiver<RenderCommand>,
    event_tx: Sender<RenderEvent>,
    // OpenGL state (initialized after window creation)
    gl_context: Option<PossiblyCurrentContext>,
    gl_surface: Option<Surface<WindowSurface>>,
    window: Option<Window>,
    projectm: Option<ProjectM>,
}

impl RenderApp {
    fn new(
        config: RenderConfig,
        command_rx: Receiver<RenderCommand>,
        event_tx: Sender<RenderEvent>,
    ) -> Self {
        Self {
            config,
            command_rx,
            event_tx,
            gl_context: None,
            gl_surface: None,
            window: None,
            projectm: None,
        }
    }

    fn process_commands(&mut self, event_loop: &ActiveEventLoop) {
        while let Ok(cmd) = self.command_rx.try_recv() {
            match cmd {
                RenderCommand::LoadPreset(path) => {
                    if let Some(ref mut pm) = self.projectm {
                        match pm.load_preset(&path, true) {
                            Ok(()) => {
                                info!("Loaded preset: {}", path);
                                let _ = self.event_tx.send(RenderEvent::PresetLoaded(path));
                            }
                            Err(e) => {
                                error!("Failed to load preset: {}", e);
                            }
                        }
                    }
                }
                RenderCommand::AudioData(samples) => {
                    if let Some(ref mut pm) = self.projectm {
                        pm.add_pcm_stereo(&samples);
                    }
                }
                RenderCommand::Resize(w, h) => {
                    if let Some(ref window) = self.window {
                        let _ = window.request_inner_size(PhysicalSize::new(w, h));
                    }
                }
                RenderCommand::ToggleFullscreen => {
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
                RenderCommand::SetBeatSensitivity(sens) => {
                    if let Some(ref mut pm) = self.projectm {
                        pm.set_beat_sensitivity(sens);
                    }
                }
                RenderCommand::Stop => {
                    event_loop.exit();
                }
            }
        }
    }

    fn render(&mut self) {
        if let (Some(ref mut pm), Some(ref surface), Some(ref context)) =
            (&mut self.projectm, &self.gl_surface, &self.gl_context)
        {
            // Render projectM frame
            pm.render_frame();

            // Swap buffers
            if let Err(e) = surface.swap_buffers(context) {
                error!("Failed to swap buffers: {}", e);
            }
        }
    }

    fn handle_resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            if let Some(ref surface) = self.gl_surface {
                surface.resize(
                    self.gl_context.as_ref().unwrap(),
                    NonZeroU32::new(size.width).unwrap(),
                    NonZeroU32::new(size.height).unwrap(),
                );
            }
            if let Some(ref mut pm) = self.projectm {
                pm.resize(size.width, size.height);
            }
            // Update viewport
            unsafe {
                gl::Viewport(0, 0, size.width as i32, size.height as i32);
            }
        }
    }
}

impl ApplicationHandler for RenderApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // Already initialized
        }

        info!("Creating render window {}x{}", self.config.width, self.config.height);

        // Window attributes
        let window_attrs = WindowAttributes::default()
            .with_title(&self.config.title)
            .with_inner_size(LogicalSize::new(self.config.width, self.config.height));

        // OpenGL config template
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_depth_size(24)
            .with_stencil_size(8);

        // Create display and window
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
                let _ = self.event_tx.send(RenderEvent::Error(e.to_string()));
                event_loop.exit();
                return;
            }
        };

        let window = window.expect("Window should be created");
        let raw_window_handle = window.window_handle().ok().map(|h| h.as_raw());

        // Create OpenGL context
        let gl_display = gl_config.display();
        let context_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(raw_window_handle);

        let not_current_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attrs)
                .expect("Failed to create OpenGL context")
        };

        // Create surface
        let attrs = window
            .build_surface_attributes(SurfaceAttributesBuilder::new())
            .expect("Failed to build surface attributes");

        let surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &attrs)
                .expect("Failed to create window surface")
        };

        // Make context current
        let context = not_current_context
            .make_current(&surface)
            .expect("Failed to make context current");

        // Load OpenGL functions
        gl::load_with(|s| {
            let c_str = CString::new(s).unwrap();
            gl_display.get_proc_address(&c_str) as *const _
        });

        // Enable vsync if requested
        if self.config.vsync {
            let _ = surface.set_swap_interval(&context, glutin::surface::SwapInterval::Wait(NonZeroU32::new(1).unwrap()));
        }

        // Log OpenGL info
        unsafe {
            let version = std::ffi::CStr::from_ptr(gl::GetString(gl::VERSION) as *const _);
            let renderer = std::ffi::CStr::from_ptr(gl::GetString(gl::RENDERER) as *const _);
            info!("OpenGL version: {:?}", version);
            info!("OpenGL renderer: {:?}", renderer);
        }

        // Create projectM instance
        let size = window.inner_size();
        match ProjectM::new(size.width, size.height) {
            Ok(mut pm) => {
                info!("ProjectM {} initialized", ProjectM::version());

                // Load initial preset if specified
                if let Some(ref preset_path) = self.config.preset_path {
                    if let Err(e) = pm.load_preset(preset_path, false) {
                        warn!("Failed to load initial preset: {}", e);
                    }
                }

                self.projectm = Some(pm);
            }
            Err(e) => {
                error!("Failed to create ProjectM instance: {}", e);
                let _ = self.event_tx.send(RenderEvent::Error(e.to_string()));
            }
        }

        self.gl_context = Some(context);
        self.gl_surface = Some(surface);
        self.window = Some(window);

        let _ = self.event_tx.send(RenderEvent::Ready);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Window close requested");
                let _ = self.event_tx.send(RenderEvent::Closed);
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                debug!("Window resized to {:?}", size);
                self.handle_resize(size);
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    logical_key: key,
                    state: ElementState::Pressed,
                    ..
                },
                ..
            } => {
                match key {
                    Key::Named(NamedKey::Escape) => {
                        info!("Escape pressed, closing window");
                        let _ = self.event_tx.send(RenderEvent::Closed);
                        event_loop.exit();
                    }
                    Key::Named(NamedKey::F11) => {
                        // Toggle fullscreen
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
                    Key::Character(ref c) if c == "f" => {
                        // Toggle fullscreen with 'f' key
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
                    _ => {}
                }
            }
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
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}

/// Run the render loop (called in render thread)
fn run_render_loop(
    config: RenderConfig,
    command_rx: Receiver<RenderCommand>,
    event_tx: Sender<RenderEvent>,
) -> Result<(), RenderError> {
    let event_loop = EventLoop::new().map_err(|e| RenderError::EventLoop(e.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = RenderApp::new(config, command_rx, event_tx);

    event_loop
        .run_app(&mut app)
        .map_err(|e| RenderError::EventLoop(e.to_string()))?;

    Ok(())
}
