//! NDI I/O, split across the sending side ([`out`]) and the discovery/
//! receiving side ([`in_`]): both driven by the same dedicated thread, see
//! [`out`]'s module doc comment.

mod in_;
mod out;

pub use out::{spawn, NdiControl, NdiHandle, NdiSnapshot};
