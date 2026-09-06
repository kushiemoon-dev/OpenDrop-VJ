//! egui panel content, one file per panel.

pub mod about;
pub mod audio;
pub mod cloud_presets;
pub mod color;
pub mod composite;
pub mod ctx;
pub mod decks;
pub mod keymap;
// Feature-gated on the `mod` declaration itself, mirroring `opendrop_io
// ::link`'s own gating: with the `link` feature off (the default), this
// file is never parsed.
#[cfg(feature = "link")]
pub mod link;
pub mod lfo;
pub mod midi;
pub mod ndi;
pub mod osc;
pub mod overlays;
pub mod output;
pub mod playlists;
pub mod preset_browser;
pub mod quality;
pub mod qvar;
pub mod remote;
pub mod rkbx_link;
pub mod share;
pub mod shell;
pub mod snapshot;
pub mod streaming;
pub mod strobe;
pub mod time;
pub mod timeline;
pub mod v4l2loopback;
pub mod video;
pub mod widgets;
