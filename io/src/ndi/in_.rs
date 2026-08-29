//! The NDI input side: source discovery (poll, not event-driven) and
//! receiving frames from a selected source. Both are driven by [`super::out`]'s
//! single NDI thread and its run loop: see that module's doc comment for
//! why sending and receiving share one thread rather than each getting its
//! own.
//!
//! Mirrors OpenDrop-VJ's `ndi:find`/`ndi:receiveStart` handlers
//! (`electron/main.cjs:664-720`): [`find`] mirrors `grandiose.find(filter,
//! timeoutMs)`, and [`ActiveReceive`] mirrors `grandiose.receive({source,
//! colorFormat: COLOR_FORMAT_RGBX_RGBA, bandwidth: BANDWIDTH_LOWEST})`'s read
//! loop, which pushed each captured frame out over Electron IPC: here it
//! pushes each frame (RGBA bytes plus resolution, see [`NdiFrame`]) on an
//! `mpsc::Sender<NdiFrame>` instead, whose `Receiver` end is exposed as
//! `NdiHandle::frame_rx` for `app` (Task 12) to read directly, no IPC
//! involved.

use std::sync::mpsc::Sender;
use std::time::Duration;

use grafton_ndi::{
    Finder, FinderOptions, Receiver as GraftonReceiver, ReceiverBandwidth, ReceiverColorFormat,
    ReceiverOptions, Source, SourceAddress, NDI,
};

/// A discovered NDI source, decoupled from `grafton_ndi::Source` the same
/// way [`super::out::NdiSnapshot`]/[`super::out::NdiControl`] never expose
/// grafton-ndi types directly: insulates `app` from a `grafton-ndi`
/// version bump changing field shapes just to display or select a source.
///
/// `address` collapses `grafton_ndi::SourceAddress`'s `Ip`/`Url`/`None`
/// distinction into a single optional string, mirroring the JS reference
/// (`main.cjs`'s `s.urlAddress`, used uniformly for both IP- and
/// URL-addressed sources). The round trip back to `grafton_ndi::Source`
/// (`impl From<NdiSource> for Source` below) re-derives which variant to
/// use with the same `"://"` heuristic `grafton_ndi::Source::try_from_raw`
/// itself uses internally, so nothing needed to reconnect is lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NdiSource {
    pub name: String,
    pub address: Option<String>,
}

impl From<Source> for NdiSource {
    fn from(source: Source) -> Self {
        let address = match source.address {
            SourceAddress::Ip(addr) | SourceAddress::Url(addr) => Some(addr),
            SourceAddress::None => None,
        };
        NdiSource { name: source.name, address }
    }
}

impl From<NdiSource> for Source {
    fn from(source: NdiSource) -> Self {
        let address = match source.address {
            Some(addr) if addr.contains("://") => SourceAddress::Url(addr),
            Some(addr) => SourceAddress::Ip(addr),
            None => SourceAddress::None,
        };
        Source { name: source.name, address }
    }
}

/// Keeps only sources whose name contains `source_filter` (case-sensitive
/// substring match), or all of them when `source_filter` is `None`. Pure
/// logic split out of [`find`] so it's testable without a real `Finder`.
fn filter_sources(sources: Vec<NdiSource>, source_filter: Option<&str>) -> Vec<NdiSource> {
    match source_filter {
        Some(f) => sources.into_iter().filter(|s| s.name.contains(f)).collect(),
        None => sources,
    }
}

/// Discovers NDI sources known to `finder`, optionally filtered by name.
/// Mirrors OpenDrop-VJ's `find(sourceFilter, timeoutMs)`
/// (`electron/main.cjs:668-676`, `grandiose.find({}, 3000)`: that call
/// site never actually passes a filter either, so [`super::out::run`]'s
/// call below follows suit), adapted to grafton-ndi's
/// `Finder::find_sources`: it honors the full `timeout_ms` window and
/// returns everything observed within it, and `Duration::ZERO` performs
/// exactly one non-blocking snapshot of what the SDK already knows.
///
/// The run loop always calls this with `timeout_ms = 0`: `finder` is kept
/// alive across ticks (so the SDK's own accumulated source list carries
/// over between calls), and this is the single shared NDI thread
/// (composite + 4 decks + discovery + receive): blocking here for seconds
/// would stall output frame draining for just as long.
///
/// Returns `Err` (the SDK's error, stringified) rather than logging and
/// falling back to an empty list itself: whole-branch review Finding 4:
/// this is polled every ~5ms by [`super::out::run`]'s loop, and a
/// persistently failing `Finder` (e.g. no NDI SDK installed) used to
/// `eprintln!` on every single call, roughly 200 lines/second indefinitely.
/// The caller now logs at most once per failure streak: see
/// `super::out::ThreadState::discovery_error_logged`.
pub(super) fn find(finder: &Finder, source_filter: Option<&str>, timeout_ms: u32) -> Result<Vec<NdiSource>, String> {
    let sources = finder.find_sources(Duration::from_millis(u64::from(timeout_ms))).map_err(|e| e.to_string())?;
    Ok(filter_sources(sources.into_iter().map(NdiSource::from).collect(), source_filter))
}

/// Opens a discovery [`Finder`], logging once and returning `None` on
/// failure: never panics, mirrors `out::ensure_ndi`'s failure handling.
pub(super) fn open_finder(ndi: &NDI) -> Option<Finder> {
    match Finder::new(ndi, &FinderOptions::default()) {
        Ok(finder) => Some(finder),
        Err(e) => {
            eprintln!("[ndi] failed to start source discovery: {e}: NDI input unavailable");
            None
        }
    }
}

/// One captured frame from an active receive: RGBA bytes plus the
/// resolution needed to interpret them. Unlike [`super::out`]'s
/// compositor/deck channels (fixed `COMP_W`/`COMP_H`/`DECK_W`/`DECK_H`), an
/// NDI-in source can be any resolution, so `app` (Task 12) needs the
/// dimensions alongside the bytes to size/recreate its GL texture: byte
/// count alone can't disambiguate width×height.
#[derive(Debug, Clone)]
pub struct NdiFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// One active receive: the SDK receiver plus the channel each captured
/// frame is pushed on. Mirrors `out::SlotSender`'s shape (SDK handle +
/// per-connection state), but there is no reusable frame buffer to keep
/// here: `VideoFrame::data()` already hands back an owned, independently
/// sized buffer per call.
pub(super) struct ActiveReceive {
    receiver: GraftonReceiver,
    frame_tx: Sender<NdiFrame>,
}

impl ActiveReceive {
    /// RGBA (`ReceiverColorFormat::RGBX_RGBA`, the Rust equivalent of the
    /// reference's `COLOR_FORMAT_RGBX_RGBA`) at `ReceiverBandwidth::Lowest`
    ///: the same `BANDWIDTH_LOWEST` default the reference used
    /// (`electron/main.cjs:687-689`). That reason (keeping an Electron
    /// IPC/relay path affordable) doesn't apply to this native app, but
    /// nothing argues for a different default either, so it's kept as-is.
    pub(super) fn start(ndi: &NDI, source: NdiSource, frame_tx: Sender<NdiFrame>) -> Option<Self> {
        let options = ReceiverOptions::builder(source.into())
            .color(ReceiverColorFormat::RGBX_RGBA)
            .bandwidth(ReceiverBandwidth::Lowest)
            .build();
        match GraftonReceiver::new(ndi, &options) {
            Ok(receiver) => Some(ActiveReceive { receiver, frame_tx }),
            Err(e) => {
                eprintln!("[ndi] failed to start receiver: {e}");
                None
            }
        }
    }

    /// One non-blocking poll: forwards a frame's RGBA bytes on `frame_tx`
    /// if one is ready. Called every run-loop tick alongside the output
    /// side's `drain_slot`, same reasoning: never block this shared
    /// thread waiting on a specific source.
    pub(super) fn poll(&self) {
        match self.receiver.video().try_capture(Duration::ZERO) {
            Ok(Some(frame)) => {
                // frame_tx is unbounded; `app` (Task 12) is expected to
                // drain NdiHandle::frame_rx continuously. A disconnected
                // receiver (app dropped that Receiver) just means these
                // sends fail silently from here on, until StopReceive
                // tears this down.
                let _ = self.frame_tx.send(NdiFrame {
                    width: frame.width() as u32,
                    height: frame.height() as u32,
                    data: frame.data().to_vec(),
                });
            }
            Ok(None) => {}
            Err(e) => eprintln!("[ndi] receive error: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_sources_matches_substring() {
        let sources = vec![
            NdiSource { name: "CAMERA1 (Studio)".into(), address: Some("192.168.1.10:5960".into()) },
            NdiSource { name: "CAMERA2 (Booth)".into(), address: None },
        ];
        let filtered = filter_sources(sources, Some("Studio"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "CAMERA1 (Studio)");
    }

    #[test]
    fn filter_sources_none_returns_all() {
        let sources =
            vec![NdiSource { name: "A".into(), address: None }, NdiSource { name: "B".into(), address: None }];
        assert_eq!(filter_sources(sources, None).len(), 2);
    }

    #[test]
    fn filter_sources_no_match_returns_empty() {
        let sources = vec![NdiSource { name: "CAMERA1".into(), address: None }];
        assert!(filter_sources(sources, Some("nonexistent")).is_empty());
    }

    #[test]
    fn ndi_source_round_trips_ip_address() {
        let source = Source { name: "CAM".into(), address: SourceAddress::Ip("192.168.1.10:5960".into()) };
        let ndi_source = NdiSource::from(source);
        assert_eq!(ndi_source.address.as_deref(), Some("192.168.1.10:5960"));
        let back: Source = ndi_source.into();
        assert!(matches!(back.address, SourceAddress::Ip(ip) if ip == "192.168.1.10:5960"));
    }

    #[test]
    fn ndi_source_round_trips_url_address() {
        let source =
            Source { name: "CAM-HX".into(), address: SourceAddress::Url("http://camera.local:8080".into()) };
        let ndi_source = NdiSource::from(source);
        let back: Source = ndi_source.into();
        assert!(matches!(back.address, SourceAddress::Url(url) if url == "http://camera.local:8080"));
    }

    #[test]
    fn ndi_source_round_trips_none_address() {
        let source = Source { name: "CAM".into(), address: SourceAddress::None };
        let ndi_source = NdiSource::from(source);
        assert_eq!(ndi_source.address, None);
        let back: Source = ndi_source.into();
        assert!(matches!(back.address, SourceAddress::None));
    }
}
