/**
 * midi-connection-store.svelte.ts — reactive wrapper around the live MIDI
 * hardware connection status (connected/device names/clock BPM). Extracted
 * from +page.svelte, same shape as color-store.svelte.ts — plain $state,
 * mutated directly by toggleMidi() which stays in +page.svelte (it owns the
 * MidiEngine browser-API instance, never unit tested in this codebase).
 *
 * Separate from midi-mapping-store.svelte.ts, which holds CC/note mappings
 * and the keyboard keymap — that state is user configuration, this state is
 * live connection status.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

export const midiConnectionState = $state({
	connected: false,
	deviceNames: [] as string[],
	clockBpm: 0, // BPM detected via MIDI clock IN (0 = inactive)
});
