//! Spout input (Windows): receives frames from another application's Spout
//! sender so they can be composited as a layer, the counterpart of NDI input.
//!
//! A dedicated thread polls the receiver and publishes the newest frame over
//! an unbounded channel that the render loop drains to its latest entry every
//! tick (the same arrangement as `ndi::in_`). Platform specifics sit behind
//! [`SpoutSource`], so the thread logic is testable on any OS; off Windows,
//! starting a receive just reports that Spout is unavailable.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;

const POLL_TICK: Duration = Duration::from_millis(8);
/// How often the list of available senders is refreshed.
const SENDER_REFRESH: Duration = Duration::from_secs(1);
/// A receive with no new frame for this long is shown as not connected.
const CONNECTED_TIMEOUT: Duration = Duration::from_secs(1);

pub struct SpoutInFrame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8, top row first.
    pub data: Vec<u8>,
}

/// A running receiver. One per active input, owned by the receive thread.
pub trait SpoutSource {
    /// Fills `buf` and returns the frame size when a new frame arrived,
    /// `None` when there is nothing new (or the sender is not connected).
    fn receive(&mut self, buf: &mut Vec<u8>) -> Result<Option<(u32, u32)>, String>;
    fn sender_names(&self) -> Vec<String>;
}

type SourceFactory = fn(Option<&str>) -> Result<Box<dyn SpoutSource>, String>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpoutInSnapshot {
    /// A receive was started and has not been stopped or failed.
    pub active: bool,
    /// A frame arrived recently.
    pub connected: bool,
    /// Senders currently published on this machine.
    pub senders: Vec<String>,
    pub last_error: Option<String>,
}

pub enum SpoutInControl {
    /// Receive from the named sender, or from the active one when `None`.
    Start(Option<String>),
    Stop,
}

pub struct SpoutInHandle {
    state: Arc<ArcSwap<SpoutInSnapshot>>,
    pub control_tx: Sender<SpoutInControl>,
    pub frame_rx: Receiver<SpoutInFrame>,
}

impl SpoutInHandle {
    pub fn latest(&self) -> Arc<SpoutInSnapshot> {
        self.state.load_full()
    }
}

pub fn spawn() -> SpoutInHandle {
    spawn_with(platform::open)
}

fn spawn_with(factory: SourceFactory) -> SpoutInHandle {
    let state = Arc::new(ArcSwap::from_pointee(SpoutInSnapshot::default()));
    let (control_tx, control_rx) = mpsc::channel();
    let (frame_tx, frame_rx) = mpsc::channel();
    std::thread::spawn({
        let state = state.clone();
        move || run(control_rx, frame_tx, state, factory)
    });
    SpoutInHandle { state, control_tx, frame_rx }
}

fn run(
    control_rx: Receiver<SpoutInControl>,
    frame_tx: Sender<SpoutInFrame>,
    state: Arc<ArcSwap<SpoutInSnapshot>>,
    factory: SourceFactory,
) {
    let mut source: Option<Box<dyn SpoutSource>> = None;
    let mut finder: Option<Box<dyn SpoutSource>> = None;
    let mut snapshot = SpoutInSnapshot::default();
    let mut buf: Vec<u8> = Vec::new();
    let mut last_frame_at: Option<Instant> = None;
    let mut last_refresh: Option<Instant> = None;

    loop {
        loop {
            match control_rx.try_recv() {
                Ok(SpoutInControl::Start(name)) => match factory(name.as_deref()) {
                    Ok(opened) => {
                        source = Some(opened);
                        snapshot.active = true;
                        snapshot.last_error = None;
                    }
                    Err(e) => {
                        eprintln!("[spout-in] failed to start: {e}");
                        source = None;
                        snapshot.active = false;
                        snapshot.last_error = Some(e);
                    }
                },
                Ok(SpoutInControl::Stop) => {
                    source = None;
                    snapshot.active = false;
                    last_frame_at = None;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        if let Some(active) = source.as_mut() {
            match active.receive(&mut buf) {
                Ok(Some((width, height))) => {
                    last_frame_at = Some(Instant::now());
                    let _ = frame_tx.send(SpoutInFrame { width, height, data: buf.clone() });
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("[spout-in] receive failed: {e}");
                    source = None;
                    snapshot.active = false;
                    snapshot.last_error = Some(e);
                }
            }
        }
        snapshot.connected = last_frame_at.is_some_and(|at| at.elapsed() < CONNECTED_TIMEOUT);

        if last_refresh.is_none_or(|at| at.elapsed() >= SENDER_REFRESH) {
            last_refresh = Some(Instant::now());
            if finder.is_none() {
                finder = factory(None).ok();
            }
            if let Some(f) = finder.as_ref() {
                snapshot.senders = f.sender_names();
            }
        }

        state.store(Arc::new(snapshot.clone()));
        std::thread::sleep(POLL_TICK);
    }
}

/// DXGI formats whose bytes are laid out blue, green, red, alpha. Most Spout
/// senders share `B8G8R8A8_UNORM` (87); 88 is the `X` variant and 91 the sRGB one.
#[cfg(any(windows, test))]
fn is_bgra_format(dxgi_format: u32) -> bool {
    matches!(dxgi_format, 87 | 88 | 91)
}

/// Swaps red and blue in tightly packed 4-byte pixels, in place. The Spout
/// SDK hands pixels back in the shared texture's own channel order, and the
/// compositor expects RGBA.
#[cfg(any(windows, test))]
fn swap_red_blue(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
}

#[cfg(windows)]
mod platform {
    use super::SpoutSource;

    struct DxSource {
        receiver: spout2::dx::Receiver,
        width: u32,
        height: u32,
    }

    impl SpoutSource for DxSource {
        fn receive(&mut self, buf: &mut Vec<u8>) -> Result<Option<(u32, u32)>, String> {
            let needed = self.width as usize * self.height as usize * 4;
            if buf.len() < needed {
                buf.resize(needed, 0);
            }
            let connected = self
                .receiver
                .receive_image(buf, self.width, self.height, false, false)
                .map_err(|e| e.to_string())?;
            if !connected {
                return Ok(None);
            }
            if self.receiver.is_updated() {
                // The sender changed size: adopt it, and take the first
                // frame at the new size on the next call.
                let (w, h) = self.receiver.sender_size();
                self.width = w;
                self.height = h;
                buf.resize(w as usize * h as usize * 4, 0);
                return Ok(None);
            }
            if self.receiver.is_frame_new() {
                if super::is_bgra_format(self.receiver.sender_format()) {
                    let len = self.width as usize * self.height as usize * 4;
                    super::swap_red_blue(&mut buf[..len]);
                }
                Ok(Some((self.width, self.height)))
            } else {
                Ok(None)
            }
        }

        fn sender_names(&self) -> Vec<String> {
            self.receiver.sender_list()
        }
    }

    pub(super) fn open(sender: Option<&str>) -> Result<Box<dyn SpoutSource>, String> {
        let receiver = spout2::dx::Receiver::new(sender).map_err(|e| e.to_string())?;
        // Start at 1x1; the first update reports the real sender size.
        Ok(Box::new(DxSource { receiver, width: 1, height: 1 }))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::SpoutSource;

    pub(super) fn open(_sender: Option<&str>) -> Result<Box<dyn SpoutSource>, String> {
        Err("Spout is only available on Windows".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static FRAMES_LEFT: AtomicUsize = AtomicUsize::new(0);

    struct FakeSource;

    impl SpoutSource for FakeSource {
        fn receive(&mut self, buf: &mut Vec<u8>) -> Result<Option<(u32, u32)>, String> {
            if FRAMES_LEFT.load(Ordering::SeqCst) == 0 {
                return Ok(None);
            }
            FRAMES_LEFT.fetch_sub(1, Ordering::SeqCst);
            buf.clear();
            buf.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
            Ok(Some((2, 1)))
        }

        fn sender_names(&self) -> Vec<String> {
            vec!["Resolume Output".to_string()]
        }
    }

    fn ok_factory(_name: Option<&str>) -> Result<Box<dyn SpoutSource>, String> {
        Ok(Box::new(FakeSource))
    }

    fn failing_factory(_name: Option<&str>) -> Result<Box<dyn SpoutSource>, String> {
        Err("no GPU".to_string())
    }

    fn wait_for(mut condition: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn frames_flow_while_started_and_the_senders_are_listed() {
        FRAMES_LEFT.store(3, Ordering::SeqCst);
        let handle = spawn_with(ok_factory);
        handle.control_tx.send(SpoutInControl::Start(Some("Resolume Output".into()))).unwrap();

        let frame = handle.frame_rx.recv_timeout(Duration::from_secs(2)).expect("a frame should arrive");
        assert_eq!((frame.width, frame.height), (2, 1));
        assert_eq!(frame.data, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        assert!(wait_for(|| handle.latest().active && handle.latest().connected));
        assert!(wait_for(|| handle.latest().senders == vec!["Resolume Output".to_string()]));

        handle.control_tx.send(SpoutInControl::Stop).unwrap();
        assert!(wait_for(|| !handle.latest().active));
    }

    #[test]
    fn bgra_senders_are_recognised_and_rgba_ones_are_not() {
        assert!(is_bgra_format(87), "B8G8R8A8_UNORM, Spout's default");
        assert!(is_bgra_format(88));
        assert!(is_bgra_format(91));
        assert!(!is_bgra_format(28), "R8G8B8A8_UNORM");
        assert!(!is_bgra_format(10), "R16G16B16A16_FLOAT");
    }

    #[test]
    fn swapping_red_and_blue_leaves_green_and_alpha_alone() {
        let mut pixels = vec![1, 2, 3, 4, 10, 20, 30, 40];
        swap_red_blue(&mut pixels);
        assert_eq!(pixels, vec![3, 2, 1, 4, 30, 20, 10, 40]);
        let mut tail = vec![1, 2, 3, 4, 9, 9];
        swap_red_blue(&mut tail);
        assert_eq!(tail, vec![3, 2, 1, 4, 9, 9], "a trailing partial pixel is untouched");
    }

    #[test]
    fn a_receive_that_cannot_start_reports_why() {
        let handle = spawn_with(failing_factory);
        handle.control_tx.send(SpoutInControl::Start(None)).unwrap();
        assert!(wait_for(|| handle.latest().last_error.as_deref() == Some("no GPU")));
        assert!(!handle.latest().active);
    }

    #[test]
    fn off_windows_the_real_source_reports_unavailable() {
        #[cfg(not(windows))]
        assert_eq!(platform::open(None).err().as_deref(), Some("Spout is only available on Windows"));
    }
}
