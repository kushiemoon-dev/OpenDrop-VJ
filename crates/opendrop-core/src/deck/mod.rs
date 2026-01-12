//! Deck module - manages visualization decks

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DeckError {
    #[error("Failed to initialize deck: {0}")]
    InitError(String),
}

/// A single visualization deck
pub struct Deck {
    id: usize,
    volume: f32,
    active: bool,
}

impl Deck {
    /// Create a new deck with the given ID
    pub fn new(id: usize) -> Self {
        Self {
            id,
            volume: 1.0,
            active: false,
        }
    }

    /// Get the deck ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Set the deck volume (0.0 to 1.0)
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    /// Get the current volume
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Check if deck is active
    pub fn is_active(&self) -> bool {
        self.active
    }
}
