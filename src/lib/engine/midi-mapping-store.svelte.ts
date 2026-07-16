/**
 * midi-mapping-store.svelte.ts — reactive wrapper around MIDI CC/note
 * mappings and the keyboard-shortcut keymap. Extracted from +page.svelte,
 * same shape as overlay-store.svelte.ts — mutate the exported state object's
 * fields, never reassign the export.
 *
 * The live `MidiEngine` connection, learn-mode flags, and the reversed
 * `keyById` lookup ($derived) stay in +page.svelte — no existing precedent in
 * this codebase for module-level $derived in a .svelte.ts store, and the
 * MIDI connection itself is a browser-API instance (same reasoning as
 * Compositor/DeckManager staying local).
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

import type { CommandId } from './commands.js';
import type { MidiTriggerKey } from './midi.js';
import { type KeyBinding, DEFAULT_KEYMAP, resetKeymap } from './keymap.js';

export const midiMappingState = $state({
	midiMappings: {} as Partial<Record<CommandId, MidiTriggerKey>>,
	keymap: { ...DEFAULT_KEYMAP } as KeyBinding,
});

export function setMidiMapping(action: CommandId, key: MidiTriggerKey): void {
	midiMappingState.midiMappings = { ...midiMappingState.midiMappings, [action]: key };
}

export function clearMidiMapping(action: CommandId): void {
	const { [action]: _, ...rest } = midiMappingState.midiMappings;
	midiMappingState.midiMappings = rest as Partial<Record<CommandId, MidiTriggerKey>>;
}

export function removeKeyBinding(key: string): void {
	const { [key]: _, ...rest } = midiMappingState.keymap;
	midiMappingState.keymap = rest as KeyBinding;
}

export function resetMidiKeymap(): void {
	midiMappingState.keymap = resetKeymap();
}
