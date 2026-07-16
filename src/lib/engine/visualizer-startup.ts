/**
 * visualizer-startup.ts — the app's main "▶ Start" sequence: acquires audio,
 * wires the compositor/decks/playlists, opens the control→output sync
 * channel, and starts the beat clock (detector, MIDI-clock-driven LFO/
 * strobe tick). Extracted from +page.svelte — pure orchestration touching
 * every browser-facing engine instance, never unit tested in this codebase
 * (same precedent as the other *-actions.ts modules).
 *
 * `audio`/`compositor`/`sync`/`beatDetector` are NOT held module-locally —
 * +page.svelte itself reads them elsewhere (onDestroy, VU-meter effect,
 * other already-extracted action wrappers), so this function takes nothing
 * for them and returns the four new instances; the caller reassigns its own
 * `let audio`/`compositor`/`sync`/`beatDetector`. Same pattern as
 * midi-connection-actions.ts's toggleMidi().
 *
 * `outputReadyOnce` is read/written from two other places in +page.svelte
 * (onOutputWindowClosed, the outputCloseTimer poller) that aren't moving, so
 * it's threaded through as a get/set pair rather than owned here.
 *
 * `opacities`/`busPresetA`/`busPresetB`/`currentClip`/`videoPlaybackRateStep`
 * are $derived values computed in +page.svelte — passed in as snapshots,
 * since $derived can't live in a plain module (same reasoning documented in
 * every store file tonight).
 *
 * `lastStrobeVal` is module-private cross-tick bookkeeping, same category as
 * `autoXfadeCount`/`tapTimes` in beat-tempo-actions.ts.
 */

import { AudioEngine } from './audio.js';
import { Compositor } from './compositor.js';
import { MainSync } from './sync.js';
import { PlaylistEngine } from './playlist.js';
import { BeatDetector } from './bpm.js';
import { getQualitySettings } from './quality.js';
import type { DeckManager } from './deck-manager.js';
import type { Clock } from './clock.js';
import type { LfoEngine } from './lfo.js';
import type { CommandRegistry, CommandContext } from './commands.js';
import type { ClipRef } from './video-store.js';
import { loadPresetData } from '../presets/index.js';
import { deckState } from './deck-store.svelte.js';
import { compositingState } from './compositing-store.svelte.js';
import { colorState } from './color-store.svelte.js';
import { DEFAULT_COLOR_PARAMS } from './sync.js';
import { perfState } from './perf-store.svelte.js';
import { timeParamsState } from './time-params-store.svelte.js';
import { qvarState } from './q-vars-store.svelte.js';
import { overlayState } from './overlay-store.svelte.js';
import { videoState } from '../video-loops/playback-store.svelte.js';
import { audioSourceState } from './audio-source-store.svelte.js';
import { playlistState, setPlaylistEngines } from './playlist-store.svelte.js';
import { strobeState } from './strobe-store.svelte.js';
import { runStatusState } from './run-status-store.svelte.js';

let lastStrobeVal = 0;

export interface StartVisualizerDeps {
	canvases: (HTMLCanvasElement | undefined)[];
	compositorCanvas: HTMLCanvasElement | undefined;
	manager: DeckManager;
	clock: Clock;
	lfoEngine: LfoEngine;
	registry: CommandRegistry;
	commandCtx: CommandContext;
	opacities: number[];
	busPresetA: string;
	busPresetB: string;
	currentClip: ClipRef | null;
	videoPlaybackRateStep: number;
	isElectron: boolean;
	onBeat: () => void;
	getOutputReadyOnce: () => boolean;
	setOutputReadyOnce: (v: boolean) => void;
}

export interface StartVisualizerResult {
	audio: AudioEngine;
	compositor: Compositor;
	sync: MainSync;
	beatDetector: BeatDetector;
}

export async function startVisualizer(deps: StartVisualizerDeps): Promise<StartVisualizerResult | null> {
	const {
		canvases, compositorCanvas, manager, clock, lfoEngine, registry, commandCtx,
		opacities, busPresetA, busPresetB, currentClip, videoPlaybackRateStep, isElectron,
		onBeat, getOutputReadyOnce, setOutputReadyOnce,
	} = deps;
	if (!canvases[0] || !canvases[1]) return null;
	try {
		const testCtx = canvases[0].getContext('webgl2');
		if (!testCtx) {
			throw new Error(
				'WebGL2 unavailable. In LibreWolf/Firefox: go to about:config → set webgl.disabled = false.'
			);
		}

		const audio = new AudioEngine();
		await audio.resume();

		// Attach the 4 canvases to the manager (slots 2-3 may be undefined)
		for (let i = 0; i < 4; i++) {
			const c = canvases[i];
			if (c) manager.attachCanvas(i, c);
		}

		const q = getQualitySettings(perfState.quality);

		const compositor = new Compositor(compositorCanvas!);
		for (let i = 0; i < 4; i++) {
			const c = canvases[i];
			if (c) compositor.attachSource(i, c);
		}
		compositor.resize(compositorCanvas!.clientWidth || window.innerWidth, compositorCanvas!.clientHeight || window.innerHeight, q.pixelRatio);
		// Explicit initial push — the $effect won't re-trigger until one of
		// the $state values it reads changes (compositor isn't one of those).
		for (let i = 0; i < 4; i++) {
			compositor.setLayer(i, opacities[i], compositingState.slotComposites[i]);
			const color = deckState.deckBus[i] === 'A' ? colorState.a : deckState.deckBus[i] === 'B' ? colorState.b : DEFAULT_COLOR_PARAMS;
			compositor.setColor(i, color);
		}
		compositor.start();

		const d0 = deckState.presetA ? await loadPresetData(deckState.presetA) : null;
		const d1 = deckState.presetB ? await loadPresetData(deckState.presetB) : null;
		await manager.start(0, audio.ctx, audio.gainNode, q, d0);
		await manager.start(1, audio.ctx, audio.gainNode, q, d1);

		// Created before the playlist engines below so their advance-callbacks
		// (which fire later, on track change) always see a real sync instance.
		const sync = new MainSync();

		const newPlaylistA = new PlaylistEngine(playlistState.aItems, playlistState.mode, playlistState.intervalSec * 1000, async (name) => {
			deckState.presetA = name;
			const d = await loadPresetData(name); if (d) manager.loadPreset(0, d, deckState.transitionTime);
			sync.sendPreset('A', name, deckState.transitionTime);
			playlistState.aPlaying = newPlaylistA.playing;
		});
		const newPlaylistB = new PlaylistEngine(playlistState.bItems, playlistState.mode, playlistState.intervalSec * 1000, async (name) => {
			deckState.presetB = name;
			const d = await loadPresetData(name); if (d) manager.loadPreset(1, d, deckState.transitionTime);
			sync.sendPreset('B', name, deckState.transitionTime);
			playlistState.bPlaying = newPlaylistB.playing;
		});
		setPlaylistEngines(newPlaylistA, newPlaylistB);

		sync.onOutputReady(async () => {
			// Full state only goes out once per output-window lifetime — output's
			// hello loop pings a few extra times after that (see its comment) purely
			// to retry the PCM kick below, and re-sending preset/etc on every ping
			// would restart their blend-in transition each time (visible flicker).
			if (!getOutputReadyOnce()) {
				setOutputReadyOnce(true);
				sync.sendPreset('A', busPresetA);
				sync.sendPreset('B', busPresetB);
				sync.sendCrossfader(deckState.crossfader);
				sync.sendQuality(perfState.quality);
				for (let i = 0; i < 4; i++) sync.sendComposite(i, compositingState.slotComposites[i]);
				for (let i = 0; i < 4; i++) sync.sendTime(i, timeParamsState.params[i]);
				for (let i = 0; i < 4; i++) sync.sendQVars(i, qvarState.params[i]);
				sync.sendPerf({ targetFps: perfState.targetFps, invisibleMode: perfState.invisibleMode, invisibleFps: perfState.invisibleFps });
				sync.sendOverlays(overlayState.overlays);
				sync.sendOverlayQueueIndex(overlayState.queueIndex);
				sync.sendVideo({ enabled: videoState.enabled, clip: currentClip, opacity: videoState.opacity, playbackRate: videoPlaybackRateStep, flashOn: videoState.reactFlash, hueOn: videoState.reactHue });
				if (audioSourceState.currentDeviceId) sync.sendSource(audioSourceState.currentDeviceId);
				if (audioSourceState.currentLoopbackDeviceId) sync.sendLoopback(audioSourceState.currentLoopbackDeviceId);
			}
			// Stream live PCM to the output window so it becomes audio-reactive
			// regardless of source (device / mic / file). Electron-only: the output
			// window cannot re-capture the same device independently (fragile on Linux).
			// await + catch so a transient worklet failure is visible in the console;
			// retried on every hello (startPcmCapture is idempotent) since it can
			// silently fail to activate on Linux with no other retry signal (README
			// Known Limitations — "audio reactivity in output" / re-pick workaround).
			if (isElectron) {
				try {
					await audio.startPcmCapture((f) => window.electronAPI!.sendAudioFrame(f));
				} catch (e) {
					console.error('[output] startPcmCapture failed', e);
				}
			}
		});

		const beatDetector = new BeatDetector(audio.analyser);
		beatDetector.start(() => {
			audioSourceState.detectedBpm = beatDetector.bpm ?? 0;
			if (!audioSourceState.manualBpm) clock.pulse(audioSourceState.detectedBpm);
		});
		clock.onBeat(onBeat);
		clock.onTick((phase) => {
			// Route LFO values to registry commands
			for (const { target, value01 } of lfoEngine.tick(phase)) {
				if (target) registry.dispatch(target, value01, commandCtx);
			}
			// Strobe: detect rising edge of a square LFO at strobeState.rate
			if (strobeState.on) {
				const p = (phase * strobeState.rate) % 1;
				const val = p < 0.5 ? 1 : 0;
				if (val === 1 && lastStrobeVal === 0) {
					strobeState.flash = true;
					setTimeout(() => { strobeState.flash = false; }, 50);
				}
				lastStrobeVal = val;
			}
		});
		clock.start();

		runStatusState.status = 'running';
		return { audio, compositor, sync, beatDetector };
	} catch (e) {
		runStatusState.status = 'error';
		runStatusState.errorMsg = e instanceof Error ? e.message : String(e);
		return null;
	}
}
