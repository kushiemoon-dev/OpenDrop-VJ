/**
 * audio-source-store.svelte.ts — reactive wrapper around the current audio
 * source selection (device/loopback), the listed devices, the source label/
 * picker UI flags, and the detected vs. manually-locked BPM. Extracted from
 * +page.svelte, same shape as color-store.svelte.ts — plain $state, mutated
 * directly by the audio-source connection functions
 * (captureSystemAudio/connectMic/connectDevice/connectLoopback, in
 * audio-source-actions.ts) — MediaDevices is a browser API boundary never
 * unit tested in this codebase.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

export const audioSourceState = $state({
	currentDeviceId: '',
	currentLoopbackDeviceId: 0,
	devices: [] as MediaDeviceInfo[],
	manualBpm: 0,
	detectedBpm: 0,
	sourceLabel: 'none',
	showDevicePicker: false,
	showSystemAudioHelp: false,
	outputDevices: [] as Array<{ id: number; name: string; maxInputChannels: number; maxOutputChannels: number; defaultSampleRate: number }>,
});
