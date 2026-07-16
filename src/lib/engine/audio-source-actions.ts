/**
 * audio-source-actions.ts — connect/disconnect logic for the audio source
 * picker (system audio / mic / device / loopback / file). Extracted from
 * +page.svelte — pure orchestration calling MediaDevices/AudioEngine (a
 * browser-API boundary never unit tested in this codebase, same precedent
 * as the Electron toggles in electron-features-actions.ts). Mutates
 * audio-source-store.svelte.ts and run-status-store.svelte.ts directly.
 *
 * `audio`/`sync`/`manager`/`audioEl` stay +page.svelte-local instances —
 * passed in as parameters rather than imported, since none of them are
 * shared singletons the way the stores are.
 */

import { AudioEngine } from './audio.js';
import type { MainSync } from './sync.js';
import type { DeckManager } from './deck-manager.js';
import { audioSourceState } from './audio-source-store.svelte.js';
import { runStatusState } from './run-status-store.svelte.js';

let loopbackUnlisten: (() => void) | null = null;

export function stopLoopbackIpc(): void {
	loopbackUnlisten?.();
	loopbackUnlisten = null;
	audioSourceState.currentLoopbackDeviceId = 0;
	window.electronAPI?.stopLoopback();
}

export async function captureSystemAudio(
	audio: AudioEngine | null, sync: MainSync | null, isElectron: boolean, platform: string, effectiveOS: string,
): Promise<void> {
	if (!audio) return;
	runStatusState.sourceError = '';
	stopLoopbackIpc();
	try {
		await audio.resume();
		if (isElectron && platform === 'win32') {
			// Electron Windows: setDisplayMediaRequestHandler → native loopback, no picker
			await audio.connectDisplay();
			audioSourceState.sourceLabel = 'system audio';
		} else if (effectiveOS === 'linux' || effectiveOS === 'darwin') {
			// Linux (Electron ou web) / macOS (Electron) : chercher .monitor ou BlackHole
			const devices = await AudioEngine.listAudioDevices();
			const monitors = devices.filter((d) =>
				/monitor|blackhole|loopback|cable|opendrop/i.test(d.label)
			);
			if (monitors.length === 1) {
				await audio.connectDevice(monitors[0].deviceId);
				audioSourceState.currentDeviceId = monitors[0].deviceId;
				audioSourceState.sourceLabel = monitors[0].label || 'system audio';
				sync?.sendSource(monitors[0].deviceId);
			} else if (monitors.length > 1) {
				audioSourceState.devices = monitors;
				audioSourceState.showDevicePicker = true;
			} else {
				audioSourceState.showSystemAudioHelp = true;
			}
		} else {
			// Web Windows / unknown browser: getDisplayMedia with honest guidance
			await audio.connectDisplay();
			audioSourceState.sourceLabel = 'system audio';
		}
	} catch (e) {
		runStatusState.sourceError = e instanceof Error ? e.message : String(e);
	}
}

export async function connectMic(audio: AudioEngine | null): Promise<void> {
	if (!audio) return;
	runStatusState.sourceError = '';
	stopLoopbackIpc();
	try {
		await audio.resume();
		await audio.connectMic();
		audioSourceState.sourceLabel = 'microphone';
	} catch (e) {
		runStatusState.sourceError = e instanceof Error ? e.message : String(e);
	}
}

export async function openDevicePicker(loopbackSupported: boolean): Promise<void> {
	runStatusState.sourceError = '';
	try {
		audioSourceState.devices = await AudioEngine.listAudioDevices();
		if (loopbackSupported) {
			const res = await window.electronAPI!.listOutputDevices();
			audioSourceState.outputDevices = res.ok ? res.devices : [];
		} else {
			audioSourceState.outputDevices = [];
		}
		audioSourceState.showDevicePicker = true;
	} catch (e) {
		runStatusState.sourceError = e instanceof Error ? e.message : String(e);
	}
}

export async function connectDevice(device: MediaDeviceInfo, audio: AudioEngine | null, sync: MainSync | null): Promise<void> {
	if (!audio) return;
	runStatusState.sourceError = '';
	audioSourceState.showDevicePicker = false;
	stopLoopbackIpc();
	try {
		await audio.resume();
		await audio.connectDevice(device.deviceId);
		audioSourceState.currentDeviceId = device.deviceId;
		audioSourceState.sourceLabel = device.label || device.deviceId;
		sync?.sendSource(device.deviceId);
	} catch (e) {
		runStatusState.sourceError = e instanceof Error ? e.message : String(e);
	}
}

export async function connectLoopback(
	device: { id: number; name: string; maxInputChannels: number; maxOutputChannels: number; defaultSampleRate: number },
	audio: AudioEngine | null, sync: MainSync | null, manager: DeckManager,
): Promise<void> {
	if (!audio) return;
	runStatusState.sourceError = '';
	audioSourceState.showDevicePicker = false;
	stopLoopbackIpc();
	try {
		await audio.resume();
		await audio.connectLoopbackPcm();
		manager.connectAudio(audio.gainNode);
		loopbackUnlisten = window.electronAPI!.onLoopbackData((data) => {
			audio?.pushLoopbackPcm(data);
		});
		const res = await window.electronAPI!.startLoopback(device.id);
		if (!res.ok) throw new Error(res.error ?? 'loopback start failed');
		audioSourceState.currentLoopbackDeviceId = device.id;
		audioSourceState.currentDeviceId = '';
		audioSourceState.sourceLabel = device.name;
		sync?.sendLoopback(device.id);
	} catch (e) {
		runStatusState.sourceError = e instanceof Error ? e.message : String(e);
	}
}

export async function connectFile(audio: AudioEngine | null, audioEl: HTMLAudioElement | undefined): Promise<void> {
	if (!audio || !audioEl) return;
	runStatusState.sourceError = '';
	try {
		await audio.resume();
		audio.connectMediaElement(audioEl);
		audioEl.play();
		audioSourceState.sourceLabel = 'file';
	} catch (e) {
		runStatusState.sourceError = e instanceof Error ? e.message : String(e);
	}
}
