pub mod chat;
pub mod cloud_presets;
pub mod command_names;
pub mod kick;
// Feature-gated on the `mod` declaration itself (not on `#[cfg]` inside
// `link.rs`): with the `link` feature off (the default), this line
// vanishes entirely and `link.rs` is never parsed/compiled: no GPL
// `rusty_link` code, and no GPL symbol, reaches a default build. See
// Task 18's brief and PLAN.md's Risque 5.
#[cfg(feature = "link")]
pub mod link;
pub mod midi;
pub mod ndi;
pub mod obs;
pub mod osc;
pub mod remote_ws;
pub mod secrets;
pub mod share_codec;
pub mod twitch;
pub mod v4l2loopback;
pub mod video_capture;
