//! egui panel content, one file per panel (Step 16 begins with the decks
//! panel; later steps add more under this module).

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
// file is never parsed. See Task 18's brief and PLAN.md's Risque 5.
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
pub mod shell;
pub mod snapshot;
pub mod streaming;
pub mod strobe;
pub mod time;
pub mod timeline;
pub mod v4l2loopback;
pub mod widgets;
