//! Shared chat-message type, normalizing incoming messages from both
//! `io::twitch` and `io::kick` into one shape: mirrors `broadcastChatMessage`
//! (`electron/main.cjs:425-429`), which fans a single function out to both
//! platforms' `on('message'|'ChatMessage', ...)` handlers so callers never
//! have to branch on platform to read a chat message.
//!
//! No channel/constructor lives here: `twitch::spawn`/`kick::spawn` each take
//! a `std::sync::mpsc::Sender<ChatMessage>` clone of one channel the caller
//! (`app`'s bootstrap) creates once via `std::sync::mpsc::channel()`: see
//! those modules' doc comments for why a plain shared `Sender` is enough
//! (single-use wiring, no abstraction needed on top of the standard
//! library's own channel constructor).

/// Which platform a `ChatMessage` came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatPlatform {
    Twitch,
    Kick,
}

/// One normalized chat message, regardless of source platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub platform: ChatPlatform,
    /// Platform-native user ID (Twitch: `sender.id` from `twitch-irc`'s
    /// `TwitchUserBasics`; Kick: `sender.id`, stringified: mirrors the JS
    /// reference's `String(message.sender.id)`, `main.cjs:487`).
    pub user_id: String,
    /// Display name (Twitch: `sender.name`, the `display-name` tag; Kick:
    /// `sender.username`).
    pub username: String,
    pub content: String,
}
