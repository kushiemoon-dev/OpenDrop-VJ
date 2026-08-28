//! Native port of OpenDrop-VJ `src/lib/engine/keymap.ts:5-16` (`DEFAULT_KEYMAP`).
//! No user-remapping/localStorage persistence: that's `loadKeymap`/`STORAGE_KEY`
//! in the TS source; nothing edits the keymap yet in Phase 2.

use opendrop_core::commands::CommandId;
use std::collections::HashMap;
use winit::keyboard::{Key, NamedKey};

pub fn default_keymap() -> HashMap<Key, CommandId> {
    let mut m = HashMap::new();
    m.insert(Key::Named(NamedKey::ArrowLeft), CommandId::CrossfaderLeft);
    m.insert(Key::Named(NamedKey::ArrowRight), CommandId::CrossfaderRight);
    m.insert(Key::Named(NamedKey::Tab), CommandId::DeckSwitch);
    m.insert(Key::Character("[".into()), CommandId::PresetPrevActive);
    m.insert(Key::Character("]".into()), CommandId::PresetNextActive);
    m.insert(Key::Named(NamedKey::Space), CommandId::PlaylistToggleActive);
    m.insert(Key::Character("n".into()), CommandId::PlaylistNextActive);
    m.insert(Key::Character("N".into()), CommandId::PlaylistNextActive);
    m.insert(Key::Character("p".into()), CommandId::PlaylistPrevActive);
    m.insert(Key::Character("P".into()), CommandId::PlaylistPrevActive);
    m
}
