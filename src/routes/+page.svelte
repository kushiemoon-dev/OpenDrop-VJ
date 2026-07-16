<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { AudioEngine } from '$lib/engine/audio.js';
	import { MainSync, type ColorParams, DEFAULT_COLOR_PARAMS, colorParamsToFilter, type SlotComposite, DEFAULT_SLOT_COMPOSITE } from '$lib/engine/sync.js';
	import { Compositor, migrateBlendModeString, blendModeFromValue01, blendModeToValue01 } from '$lib/engine/compositor.js';
	import { compositingState, updateComposite } from '$lib/engine/compositing-store.svelte.js';
	import { colorState } from '$lib/engine/color-store.svelte.js';
	import { snapshotsState, setSnapshotValues, renameSnapshot, clearSnapshot } from '$lib/engine/snapshots-store.svelte.js';
	import {
		timelineState, toggleTimelinePlay, addTimelineKeyframe, removeTimelineKeyframe, updateTimelineKeyframe,
	} from '$lib/engine/timeline-store.svelte.js';
	import {
		midiMappingState, setMidiMapping, clearMidiMapping, removeKeyBinding, resetMidiKeymap,
	} from '$lib/engine/midi-mapping-store.svelte.js';
	import { strobeState } from '$lib/engine/strobe-store.svelte.js';
	import { perfState } from '$lib/engine/perf-store.svelte.js';
	import { electronFeaturesState } from '$lib/engine/electron-features-store.svelte.js';
	import { midiConnectionState } from '$lib/engine/midi-connection-store.svelte.js';
	import { audioSourceState } from '$lib/engine/audio-source-store.svelte.js';
	import { runStatusState } from '$lib/engine/run-status-store.svelte.js';
	import {
		toggleNdi, toggleV4l2, toggleSpout, toggleOsc, toggleRemote, toggleLink as toggleLinkAction,
	} from '$lib/engine/electron-features-actions.js';
	import {
		stopLoopbackIpc, captureSystemAudio as captureSystemAudioAction, connectMic as connectMicAction,
		openDevicePicker as openDevicePickerAction, connectDevice as connectDeviceAction,
		connectLoopback as connectLoopbackAction, connectFile as connectFileAction,
	} from '$lib/engine/audio-source-actions.js';
	import { toggleMidi as toggleMidiAction } from '$lib/engine/midi-connection-actions.js';
	import { deckState } from '$lib/engine/deck-store.svelte.js';
	import { beatSyncState, updateBeatTriggerA, updateBeatTriggerB } from '$lib/engine/beat-sync-store.svelte.js';
	import { selectPreset as selectPresetAction, loadImportedMilkPreset as loadImportedMilkPresetAction } from '$lib/engine/deck-preset-actions.js';
	import {
		selectPresetForDeck as selectPresetForDeckAction, buildCurrentSharedSet as buildCurrentSharedSetAction,
		copyShareLink as copyShareLinkAction, applyPendingSharedSet as applyPendingSharedSetAction,
		cancelPendingSharedSet as cancelPendingSharedSetAction,
	} from '$lib/engine/share-set-actions.js';
	import { shareSetState } from '$lib/engine/share-set-store.svelte.js';
	import { startVisualizer as startVisualizerAction } from '$lib/engine/visualizer-startup.js';
	import {
		openOutputFullscreen as openOutputFullscreenAction, onResize as onResizeAction,
	} from '$lib/engine/output-window-actions.js';
	import {
		onBeat as onBeatAction, toggleBeatSync as toggleBeatSyncAction, tapTempo as tapTempoAction, clearManualBpm as clearManualBpmAction,
		resetAutoXfadeCount,
	} from '$lib/engine/beat-tempo-actions.js';
	import { SnapshotEngine, type Snapshot } from '$lib/engine/snapshot.js';
	import { TimelineEngine, timelineLoopDuration, type TimelineKeyframe } from '$lib/engine/timeline.js';
	import { type SharedSet, decodeSharedSet } from '$lib/engine/share-set.js';
	import { type DeckTimeParams, defaultTimeParams, getGlobalTimeParams } from '$lib/engine/time-params.js';
	import { defaultQVarParams, getGlobalQVarParams } from '$lib/engine/q-vars.js';
	import { timeParamsState, updateTimeParams } from '$lib/engine/time-params-store.svelte.js';
	import { qvarState, updateQVarValue, addQVarWatch, removeQVarWatch } from '$lib/engine/q-vars-store.svelte.js';
	import {
		playlistState, destroyPlaylistEngines, addToPlaylist, removeFromPlaylist,
		togglePlaylist, playlistNext, playlistPrev, exportPlaylists, importPlaylists,
	} from '$lib/engine/playlist-store.svelte.js';
	import {
		defaultBeatTriggerConfig,
		type VolumePeakState, defaultVolumePeakState, detectVolumePeak,
		clampBeatsPerChange, clampOffset,
	} from '$lib/engine/beat-trigger.js';
	import { visibleOverlayIds } from '$lib/engine/overlay-queue.js';
	import { initPresets, buildPresetList, loadPresetData, type PresetMeta } from '$lib/presets/index.js';
	import { isMilkPresetFilename } from '$lib/presets/milk-import.js';
	import PresetBrowser from '$lib/components/PresetBrowser.svelte';
	import { MidiEngine } from '$lib/engine/midi.js';
	import { createDefaultRegistry, type CommandId, type CommandContext } from '$lib/engine/commands.js';
	import { loadKeymap, saveKeymap, DEFAULT_KEYMAP } from '$lib/engine/keymap.js';
	import { Clock } from '$lib/engine/clock.js';
	import { LfoEngine, defaultSlot } from '$lib/engine/lfo.js';
	import { BeatDetector } from '$lib/engine/bpm.js';
	import { getQualitySettings, DEFAULT_TIER, DEFAULT_PERF, type QualityTier, type InvisibleMode } from '$lib/engine/quality.js';
	import {
		overlayState, onOverlayFilePick, addTextOverlay, addOverlayAtPosition, onVisualizerDragOver,
		removeOverlay, updateOverlay, toggleOverlayQueue, setOverlayQueueMode,
		updateOverlayQueueTrigger, advanceOverlayQueue,
	} from '$lib/engine/overlay-store.svelte.js';
	import OverlayLayer from '$lib/components/OverlayLayer.svelte';
	import VideoLayer from '$lib/components/VideoLayer.svelte';
	import SidebarAudio from '$lib/components/SidebarAudio.svelte';
	import SidebarPlaylist from '$lib/components/SidebarPlaylist.svelte';
	import SidebarOverlays from '$lib/components/SidebarOverlays.svelte';
	import {
		cloudPresetsState, initCloudPresets, onCloudPresetFilePick, copyCloudToken,
		linkCloudDevice, renameCloudPresetEntry, deleteCloudPresetEntry,
	} from '$lib/engine/cloud-presets-store.svelte.js';
	import SidebarCloudPresets from '$lib/components/SidebarCloudPresets.svelte';
	import SidebarVideo from '$lib/components/SidebarVideo.svelte';
	import SidebarQuality from '$lib/components/SidebarQuality.svelte';
	import SidebarOutput from '$lib/components/SidebarOutput.svelte';
	import SidebarMidi from '$lib/components/SidebarMidi.svelte';
	import SidebarKeymap from '$lib/components/SidebarKeymap.svelte';
	import SidebarStrobe from '$lib/components/SidebarStrobe.svelte';
	import SidebarLfo from '$lib/components/SidebarLfo.svelte';
	import SidebarColor from '$lib/components/SidebarColor.svelte';
	import SidebarElectron from '$lib/components/SidebarElectron.svelte';
	import SidebarComposite from '$lib/components/SidebarComposite.svelte';
	import SidebarSnapshot from '$lib/components/SidebarSnapshot.svelte';
	import SidebarTimeline from '$lib/components/SidebarTimeline.svelte';
	import SidebarShare from '$lib/components/SidebarShare.svelte';
	import SidebarTime from '$lib/components/SidebarTime.svelte';
	import SidebarQvar from '$lib/components/SidebarQvar.svelte';
	import DeckCard from '$lib/components/DeckCard.svelte';
	import LayoutToggle from '$lib/components/LayoutToggle.svelte';
	import MixerLayout from '$lib/components/MixerLayout.svelte';
	import { DeckManager } from '$lib/engine/deck-manager.js';
	import { initVideoLoops, builtinClips } from '$lib/video-loops/index.js';
	import { type ClipRef } from '$lib/engine/video-store.js';
	import {
		videoState, addVideoFromFile, onVideoFilePick, removeVideoClip, onVideoAudioTick,
		setLiveCamera, clearLiveCamera, setNdiSource, clearNdiSource,
	} from '$lib/video-loops/playback-store.svelte.js';

	// — State —————————————————————————————————————————————
	let canvases = $state<(HTMLCanvasElement | undefined)[]>([undefined, undefined, undefined, undefined]);
	let compositorCanvas: HTMLCanvasElement | undefined = $state();
	let compositor: Compositor | null = null;
	const manager = new DeckManager();
	let audio: AudioEngine | null = null;

	let presetList: PresetMeta[] = $state([]);

	// activeSlot/presetA/presetB/preset2/preset3/deckBus/crossfader/transitionTime/slotEpoch
	// — state extracted into deck-store.svelte.ts

	// sourceLabel/currentDeviceId/currentLoopbackDeviceId/audioDevices — state extracted into audio-source-store.svelte.ts
	// status/errorMsg/sourceError — state extracted into run-status-store.svelte.ts
	let audioEl: HTMLAudioElement | undefined = $state();
	// outputDevices/showDevicePicker — state extracted into audio-source-store.svelte.ts
	// loopbackUnlisten — moved into audio-source-actions.ts (private to it)
	let vuLevel = $state(0);
	let outputOpen = $state(false);
	let outputWinRef: Window | null = null;
	// Non-reactive: gates the one-shot full-state send in onOutputReady (see there).
	let outputReadyOnce = false;
	let outputCloseTimer: ReturnType<typeof setInterval> | null = null;
	let sync: MainSync | null = null;
	// Screen targeting (Electron)
	type DisplayInfo = { id: number; label: string; isPrimary: boolean; bounds: { x: number; y: number; width: number; height: number } };
	let displays = $state<DisplayInfo[]>([]);
	let selectedDisplayId = $state<number | null>(null);
	let outputWindowClosedUnlisten: (() => void) | null = null;

	// — Playlist state — extracted into playlist-store.svelte.ts

	// — MIDI ——————————————————————————————————————————————
	const registry = createDefaultRegistry();

	const midiSupported = typeof navigator !== 'undefined' && 'requestMIDIAccess' in navigator;
	let midi: MidiEngine | null = null;
	// midiConnected/midiDeviceNames/midiClockBpm — state extracted into midi-connection-store.svelte.ts
	// midiMappings/keymap/learningAction — state extracted into midi-mapping-store.svelte.ts
	let learningKey = $state<CommandId | null>(null);

	// — Clock + LFO + Strobe ———————————————————————————————
	const clock = new Clock();
	const lfoEngine = new LfoEngine();
	const lfoSlots = $state(lfoEngine.slots);
	// strobeOn/Rate/Intensity/Color/Flash — state extracted into strobe-store.svelte.ts
	// _lastStrobeVal — moved into visualizer-startup.ts (module-private)

	// — Color controls per deck (M3) — state extracted into color-store.svelte.ts
	type ColorCmd = [sfx: string, field: keyof ColorParams, lbl: string];
	const COLOR_CMDS: ColorCmd[] = [
		['hue','hueRotate','Hue'],['sat','saturate','Saturation'],
		['bright','brightness','Brightness'],['contrast','contrast','Contrast'],['invert','invert','Invert'],
	];
	const colorFilterA = $derived(colorParamsToFilter(colorState.a));
	const colorFilterB = $derived(colorParamsToFilter(colorState.b));

	// Inverted index: commandId → assigned key string
	const keyById = $derived(
		new Map<CommandId, string>(
			(Object.entries(midiMappingState.keymap) as [string, CommandId][]).map(([k, v]) => [v, k])
		)
	);

	// — Electron ——————————————————————————————————————————
	const isElectron = typeof window !== 'undefined' && !!window.electronAPI?.isElectron;
	let platform = $state('');
	// showSystemAudioHelp — state extracted into audio-source-store.svelte.ts
	let showPresetBrowser = $state(false);
	let showStreamPanel = $state(false);
	// ndiActive/Error, oscActive/Port/Error, remoteActive/Url/Error, linkActive/Peers/Error,
	// v4l2Active/Error, spoutActive/Error — state extracted into electron-features-store.svelte.ts
	let oscUnlisten: (() => void) | null = null;
	let remoteUnlisten: (() => void) | null = null;
	let linkUnlisten: (() => void) | null = null;
	let layout = $state<'stage' | 'mixer'>('stage');
	let mixerSelectedSlot = $state(0);

	/** Detect OS in web builds (navigator.userAgent) — used for help text only. */
	function detectWebOS(): string {
		if (typeof navigator === 'undefined') return '';
		const ua = navigator.userAgent;
		if (ua.includes('Windows')) return 'windows';
		if (ua.includes('Macintosh') || ua.includes('Mac OS')) return 'darwin';
		if (ua.includes('Linux')) return 'linux';
		return '';
	}
	/** Effective OS: Electron gives us the real value; web falls back to UA detection. */
	const effectiveOS = $derived(platform || detectWebOS());
	const loopbackSupported = $derived(isElectron && platform === 'win32' && !!window.electronAPI?.listOutputDevices);

	// — Video loops — state/actions extracted into playback-store.svelte.ts

	// — Beat detection ————————————————————————————————————
	let beatDetector: BeatDetector | null = null;
	// detectedBpm/manualBpm — state extracted into audio-source-store.svelte.ts
	// beatSyncA/B/beatsPerChange/beatTriggerA/B/autoXfade/lockA/B — state extracted into beat-sync-store.svelte.ts
	let volumePeakStateA: VolumePeakState = defaultVolumePeakState();
	let volumePeakStateB: VolumePeakState = defaultVolumePeakState();
	// autoXfadeCount/tapTimes — moved into beat-tempo-actions.ts (module-private)

	// — Render quality — state extracted into perf-store.svelte.ts
	let fps = $state(0);

	// — Compositing par slot (blend + lumaKey + colorKey) — state/actions extracted into compositing-store.svelte.ts

	// — Time param sliders per deck (1.4) — state/actions extracted into time-params-store.svelte.ts
	// — Q-var live editing per deck (Track 2) — state/actions extracted into q-vars-store.svelte.ts

	// — Snapshots / macros (1.3) — state extracted into snapshots-store.svelte.ts
	const snapshotEngine = new SnapshotEngine();
	function saveSnapshot(slot: number) {
		const values: Partial<Record<CommandId, number>> = {};
		for (const id of LOOK_COMMAND_IDS) {
			const v = getCommandCurrentValue(id);
			if (v !== null) values[id] = v;
		}
		setSnapshotValues(slot, values);
	}

	// — Timeline (Track 2 — keyframe playback) — state/actions extracted into timeline-store.svelte.ts
	const timelineEngine = new TimelineEngine();

	// — Performance decks — state extracted into perf-store.svelte.ts
	let pausedSlots = new Set<number>();   // non-reactive: cross-run memory for the eco $effect
	let lastFps = [0, 0, 0, 0];            // non-reactive: cross-run anti-churn for the eco $effect

	// — Overlays ——————————————————————————————————————————
	// beat — state extracted into beat-sync-store.svelte.ts
	// overlays + overlay auto-cycle queue state/actions extracted into overlay-store.svelte.ts
	let overlayQueueVolumeState: VolumePeakState = defaultVolumePeakState();
	const overlayVisibleIds = $derived(visibleOverlayIds(overlayState.overlays, overlayState.queueIndex));
	const nonShareableOverlayCount = $derived(overlayState.overlays.filter((o) => o.kind !== 'text').length);

	// — Cloud presets (Track 2) — state + actions extracted into cloud-presets-store.svelte.ts
	const activeDeck = $derived<'A' | 'B'>(deckState.deckBus[deckState.activeSlot] === 'B' ? 'B' : 'A');
	let activePreset = $derived(activeDeck === 'A' ? deckState.presetA : deckState.presetB);
	const presets4 = $derived([deckState.presetA, deckState.presetB, deckState.preset2, deckState.preset3]);

	function busGain(bus: 'A' | 'B' | 'off', x: number): number {
		if (bus === 'A') return 1 - x;
		if (bus === 'B') return x;
		return 0;
	}
	const opacities = $derived(deckState.deckBus.map((bus) => busGain(bus, deckState.crossfader)));
	const opacityA = $derived(opacities[0]);
	const opacityB = $derived(opacities[1]);
	let presetIdxA = $derived(presetList.findIndex((p) => p.name === deckState.presetA));
	let presetIdxB = $derived(presetList.findIndex((p) => p.name === deckState.presetB));
	const allClips = $derived([...builtinClips, ...videoState.userClips]);

	/** Returns the preset of the first running deck on a given bus, or the last known preset if none running. */
	function primaryPreset(bus: 'A' | 'B'): string {
		void deckState.slotEpoch; // force reactive tracking
		for (let i = 0; i < 4; i++) {
			if (deckState.deckBus[i] === bus && isRunning(i)) return presets4[i];
		}
		return bus === 'A' ? deckState.presetA : deckState.presetB;
	}

	const busPresetA = $derived(primaryPreset('A'));
	const busPresetB = $derived(primaryPreset('B'));
	const runningCount = $derived([0, 1, 2, 3].filter(i => manager.isRunning(i)).length);

	// Order is load-bearing: `liveDeviceId` must short-circuit BEFORE `allClips`/
	// `currentClipIndex` are read, so those two aren't tracked as dependencies while
	// live — otherwise VideoLayer's live effect would re-run (re-acquire the camera,
	// visible flicker) on every unrelated clip-index change.
	const currentClip = $derived<ClipRef | null>(
		!videoState.enabled ? null :
		videoState.liveDeviceId ? { kind: 'live', deviceId: videoState.liveDeviceId, label: videoState.liveLabel } :
		videoState.ndiSourceName ? { kind: 'ndi', sourceName: videoState.ndiSourceName, urlAddress: videoState.ndiUrlAddress } :
		allClips.length > 0 ? allClips[videoState.currentClipIndex % allClips.length].ref : null
	);
	// Rounded to 1/20 steps so the sync $effect doesn't fire at 60fps
	const videoPlaybackRateStep = $derived(Math.round(videoState.playbackRate * 20) / 20);

	/** Toggle a live camera as the video layer source. Probes the default camera once
	 * to resolve a stable deviceId/label, then immediately releases that probe stream —
	 * VideoLayer re-acquires the same physical device by deviceId in whichever window
	 * needs it (mirrors the mic 'source' sync pattern in output/+page.svelte). */
	async function onToggleLiveCamera(): Promise<void> {
		if (videoState.liveDeviceId) { clearLiveCamera(); return; }
		// setLiveCamera() clears ndiSourceName store-side, but the store can't reach the
		// Electron IPC bridge — stop the still-running main-process receiver here too,
		// or it would keep broadcasting frames nobody's listening to.
		if (videoState.ndiSourceName) await window.electronAPI?.ndiReceiveStop();
		try {
			const stream = await navigator.mediaDevices.getUserMedia({ video: true });
			const track = stream.getVideoTracks()[0];
			const deviceId = track?.getSettings().deviceId ?? '';
			const label = track?.label || 'Camera';
			stream.getTracks().forEach((t) => t.stop());
			if (deviceId) setLiveCamera(deviceId, label);
		} catch { /* permission denied or no camera available */ }
	}

	// — NDI receive (Electron only) — unlike a camera, there's a single shared
	// receiver in the main process, so ONLY this window drives start/stop;
	// VideoLayer in both windows just listens for the broadcasted frames.
	let ndiSources = $state<Array<{ name: string; urlAddress: string }>>([]);

	async function onFindNdiSources(): Promise<void> {
		const res = await window.electronAPI?.ndiFind();
		ndiSources = res?.ok ? res.sources : [];
	}

	async function onSelectNdiSource(name: string, urlAddress: string): Promise<void> {
		const res = await window.electronAPI?.ndiReceiveStart(name, urlAddress);
		if (res?.ok) { setNdiSource(name, urlAddress); ndiSources = []; }
	}

	async function onClearNdiSource(): Promise<void> {
		await window.electronAPI?.ndiReceiveStop();
		clearNdiSource();
	}

	// — Wiring M2 commands (strobe/LFO) into the registry ——
	registry.register({
		id: 'strobe-toggle', label: 'Strobe ON/OFF', kind: 'trigger',
		run() { strobeState.on = !strobeState.on; },
	});
	registry.register({
		id: 'lfo-rate-up', label: 'LFO Rate +', kind: 'trigger',
		run() { strobeState.rate = Math.min(4, strobeState.rate * 2); },
	});
	registry.register({
		id: 'lfo-rate-down', label: 'LFO Rate −', kind: 'trigger',
		run() { strobeState.rate = Math.max(0.25, strobeState.rate / 2); },
	});

	// — Wiring M3 commands (color controls) ——————————————
	for (const [sfx, field, lbl] of COLOR_CMDS)
		for (const deck of ['a', 'b'] as const)
			registry.register({ id: `color-${sfx}-${deck}` as CommandId, label: `${lbl} ${deck.toUpperCase()}`, kind: 'range',
				run(v) { if (deck === 'a') colorState.a = {...colorState.a, [field]: v}; else colorState.b = {...colorState.b, [field]: v}; },
			});

	// — Wiring 1.1 commands (compositing: blend + lumaKey + colorKey, 4 slots) —
	type CompositeCmd = [
		prefix: string, label: string,
		apply: (cfg: SlotComposite, v: number) => SlotComposite,
		read: (cfg: SlotComposite) => number,
	];
	const COMPOSITE_CMDS: CompositeCmd[] = [
		['composite-blend', 'Blend', (c, v) => ({ ...c, blend: blendModeFromValue01(v) }), (c) => blendModeToValue01(c.blend)],
		['lumakey-black', 'Luma Black', (c, v) => ({ ...c, lumaBlack: v }), (c) => c.lumaBlack],
		['lumakey-white', 'Luma White', (c, v) => ({ ...c, lumaWhite: v }), (c) => c.lumaWhite],
		['colorkey-hue', 'Key Hue', (c, v) => ({ ...c, colorHue: v }), (c) => c.colorHue],
		['colorkey-tolerance', 'Key Tolerance', (c, v) => ({ ...c, colorTol: v }), (c) => c.colorTol],
	];
	// The 30 "look" commands that a snapshot captures/recalls — derived from COLOR_CMDS/
	// COMPOSITE_CMDS to stay in sync if those arrays change. The crossfader
	// doesn't appear in either one, so it's excluded by construction.
	const LOOK_COMMAND_IDS: CommandId[] = [
		...COLOR_CMDS.flatMap(([sfx]) => (['a', 'b'] as const).map((d) => `color-${sfx}-${d}` as CommandId)),
		...COMPOSITE_CMDS.flatMap(([prefix]) => ([0, 1, 2, 3] as const).map((s) => `${prefix}-${s}` as CommandId)),
	];
	for (const [prefix, lbl, apply] of COMPOSITE_CMDS)
		for (const slot of [0, 1, 2, 3] as const)
			registry.register({ id: `${prefix}-${slot}` as CommandId, label: `${lbl} ${slot}`, kind: 'range',
				run(v) { updateComposite(slot, apply(compositingState.slotComposites[slot], v)); },
			});

	// — Wiring 1.4 commands (time param sliders) ——————————
	// v (0..1) is mapped to the sliders' 0..2 display range (v*2).
	type TimeCmd = [prefix: string, label: string, field: keyof DeckTimeParams];
	const TIME_CMDS: TimeCmd[] = [
		['time-speed', 'Speed', 'speedMult'],
		['time-zoom', 'Zoom', 'zoomMult'],
		['time-rot', 'Rotation', 'rotMult'],
		['time-warp', 'Wrap', 'warpMult'],
		['time-dx', 'Horizontal', 'dxMult'],
		['time-dy', 'Vertical', 'dyMult'],
		['time-stretch', 'Stretch', 'stretchMult'],
		['time-wave', 'Wave', 'waveMult'],
	];
	for (const [prefix, lbl, field] of TIME_CMDS)
		for (const slot of [0, 1, 2, 3] as const)
			registry.register({ id: `${prefix}-${slot}` as CommandId, label: `${lbl} ${slot}`, kind: 'range',
				run(v) { updateTimeParams(slot, { [field]: v * 2 } as Partial<DeckTimeParams>); },
			});

	// — Wiring Q-var live editing commands (Track 2) ——————————
	// v (0..1) mapped to the sliders' [-2, 2] display range (v*4-2).
	// Never touches `enabled` — only the watchlist (UI) activates a q-var.
	for (let n = 1; n <= 32; n++)
		for (const slot of [0, 1, 2, 3] as const)
			registry.register({ id: `qvar-${n}-${slot}` as CommandId, label: `Q${n} — Deck ${slot}`, kind: 'range',
				run(v) { updateQVarValue(slot, n, v * 4 - 2); },
			});

	// — Command context (injected into registry.dispatch) ——
	const commandCtx: CommandContext = {
		getCrossfader: () => deckState.crossfader,
		setCrossfader: (v) => { deckState.crossfader = v; },
		getActiveDeck: () => activeDeck,
		switchActiveDeck: () => { deckState.activeSlot = deckState.activeSlot === 0 ? 1 : 0; },
		navigatePreset(deck, direction) {
			if (presetList.length === 0) return;
			if (deck === 'A') {
				const idx = direction === 1
					? (presetIdxA + 1) % presetList.length
					: ((presetIdxA <= 0 ? presetList.length : presetIdxA) - 1) % presetList.length;
				selectPresetForDeck('A', presetList[idx].name);
			} else {
				const idx = direction === 1
					? (presetIdxB + 1) % presetList.length
					: ((presetIdxB <= 0 ? presetList.length : presetIdxB) - 1) % presetList.length;
				selectPresetForDeck('B', presetList[idx].name);
			}
		},
		togglePlaylist,
		playlistNext,
		playlistPrev,
		advanceOverlayQueue,
	};

	// — Wiring 1.3 commands (recall snapshots) ——————————————
	for (const slot of [0, 1, 2, 3, 4, 5, 6, 7] as const) {
		registry.register({
			id: `recall-snapshot-${slot}` as CommandId,
			label: `Recall Snapshot ${slot}`,
			kind: 'trigger',
			run() {
				const snap = snapshotsState.snapshots[slot];
				if (!snap) return; // slot vide → inerte, pas de rappel
				// Re-read live (never cached): a restart mid-recall must
				// always start from the actual visual state at that moment, not a stale value.
				const start: Partial<Record<CommandId, number>> = {};
				for (const id of LOOK_COMMAND_IDS) {
					const v = getCommandCurrentValue(id);
					if (v !== null) start[id] = v;
				}
				snapshotEngine.recall(start, snap.values, snapshotsState.recallDuration * 1000, (values) => {
					for (const key in values)
						registry.dispatch(key as CommandId, values[key as CommandId]!, commandCtx);
				});
			},
		});
	}
	function recallSnapshot(slot: number) {
		registry.dispatch(`recall-snapshot-${slot}` as CommandId, 1, commandCtx);
	}
	registry.register({
		id: 'timeline-toggle',
		label: 'Timeline Play/Pause',
		kind: 'trigger',
		run: toggleTimelinePlay,
	});

	// — Sync crossfader to output window ——————————————————
	// Read crossfader unconditionally before sync?. so Svelte 5 tracks it as a
	// dependency even when sync is still null on the first $effect run (onMount
	// is async → sync is assigned late). Without this, ?. short-circuits the
	// argument evaluation → crossfader is never tracked → effect never re-runs.
	$effect(() => {
		const x = deckState.crossfader;
		sync?.sendCrossfader(x);
	});

	// — Sync overlay queue index vers output ——————————————
	$effect(() => {
		const idx = overlayState.queueIndex;
		sync?.sendOverlayQueueIndex(idx);
	});

	// — Pilote TimelineEngine.play()/.pause() ————————————————
	// Read timelineState.keyframes and timelineState.playing unconditionally before any logic: both
	// are $state, so any change (editing a keyframe mid-playback,
	// toggling play/pause) re-triggers this effect — never an orphaned RAF loop on a stale
	// array after an edit/remove during playback (see design note above).
	$effect(() => {
		const kfs = timelineState.keyframes;
		const playing = timelineState.playing;
		if (!playing) { timelineEngine.pause(); return; }
		if (timelineLoopDuration(kfs) <= 0) { timelineState.playing = false; return; } // guard (also covers the degenerate case: all keyframes at the same instant)
		timelineEngine.play(kfs, snapshotsState.snapshots, (values) => {
			for (const key in values)
				registry.dispatch(key as CommandId, values[key as CommandId]!, commandCtx);
		});
	});

	// — Re-fit canvases on layout switch —————————————————
	// The visualizer-wrap stays permanently mounted (never destroyed, see .mixer-hidden) but
	// changes box (flex:1 in stage vs position:absolute;inset:0 in mixer) — without this re-fit,
	// coming back to stage leaves the composite canvas at the old resolution until the next
	// window resize. Read layout/status unconditionally before the early-return inside
	// onResize (Svelte 5 gotcha: a $state read only after a non-reactive guard breaks
	// tracking if the 1st run happens while the guard is true).
	$effect(() => {
		const l = layout;
		const s = runStatusState.status;
		if (s === 'running') requestAnimationFrame(onResize);
	});

	// — VU meter polling + FPS counter + video speed warp —
	$effect(() => {
		if (runStatusState.status !== 'running' || !audio) return;
		let rafId: number;
		let fpsLast = performance.now();
		let lastRenderCount = 0;
		const tick = (t: number) => {
			// VU meter + video warp
			const lv = audio!.getLevels();
			vuLevel = lv.rms;
			if (beatSyncState.beatSyncA && !beatSyncState.lockA && beatSyncState.beatTriggerA.mode === 'volume-peak') {
				const { triggered, next } = detectVolumePeak(lv.rms, volumePeakStateA, beatSyncState.beatTriggerA.sensitivity, t);
				volumePeakStateA = next;
				if (triggered) {
					if (playlistState.aItems.length > 0) playlistNext('A');
					else applyMidiAction('preset-next-a', 127);
				}
			}
			if (beatSyncState.beatSyncB && !beatSyncState.lockB && beatSyncState.beatTriggerB.mode === 'volume-peak') {
				const { triggered, next } = detectVolumePeak(lv.rms, volumePeakStateB, beatSyncState.beatTriggerB.sensitivity, t);
				volumePeakStateB = next;
				if (triggered) {
					if (playlistState.bItems.length > 0) playlistNext('B');
					else applyMidiAction('preset-next-b', 127);
				}
			}
			if (overlayState.queueEnabled && overlayState.queueTrigger.mode === 'volume-peak') {
				const { triggered, next } = detectVolumePeak(lv.rms, overlayQueueVolumeState, overlayState.queueTrigger.sensitivity, t);
				overlayQueueVolumeState = next;
				if (triggered) advanceOverlayQueue(1);
			}
			onVideoAudioTick(lv.bass);
			// FPS counter — measures actual Butterchurn renders (not RAF ticks)
			if (t - fpsLast >= 500) {
				const activeSlot = ([0, 1, 2, 3] as const).find(i => manager.isRunning(i)) ?? 0;
				const current = manager.getRenderCount(activeSlot);
				fps = Math.round((current - lastRenderCount) * 1000 / (t - fpsLast));
				lastRenderCount = current;
				fpsLast = t;
			}
			rafId = requestAnimationFrame(tick);
		};
		rafId = requestAnimationFrame(tick);
		return () => { cancelAnimationFrame(rafId); fps = 0; };
	});

	// — Persistance localStorage ——————————————————————————
	// _ready prevents the $effects from overwriting localStorage before onMount has read it
	let _ready = $state(false);
	$effect(() => {
		if (!_ready) return;
		localStorage.setItem('od-pl-a', JSON.stringify(playlistState.aItems));
		localStorage.setItem('od-pl-b', JSON.stringify(playlistState.bItems));
		localStorage.setItem('od-pl-interval', String(playlistState.intervalSec));
		localStorage.setItem('od-pl-mode', playlistState.mode);
		localStorage.setItem('od-beat-trigger-a', JSON.stringify(beatSyncState.beatTriggerA));
		localStorage.setItem('od-beat-trigger-b', JSON.stringify(beatSyncState.beatTriggerB));
		localStorage.setItem('od-overlay-queue', JSON.stringify({
			enabled: overlayState.queueEnabled, trigger: overlayState.queueTrigger, mode: overlayState.queueMode,
		}));
		localStorage.setItem('od-midi-mappings', JSON.stringify(midiMappingState.midiMappings));
		localStorage.setItem('od-keymap', JSON.stringify(midiMappingState.keymap));
		localStorage.setItem('od-quality', perfState.quality);
		localStorage.setItem('od-target-fps', String(perfState.targetFps));
		localStorage.setItem('od-invisible-mode', perfState.invisibleMode);
		localStorage.setItem('od-overlays', JSON.stringify(overlayState.overlays));
		localStorage.setItem('od-deck-bus', JSON.stringify(deckState.deckBus));
		localStorage.setItem('od-layout', layout);
		localStorage.setItem('od-transition', String(deckState.transitionTime));
		localStorage.setItem('od-composite', JSON.stringify(compositingState.slotComposites));
		localStorage.setItem('od-snapshots', JSON.stringify(snapshotsState.snapshots));
		localStorage.setItem('od-snapshot-duration', String(snapshotsState.recallDuration));
		localStorage.setItem('od-time-params', JSON.stringify(timeParamsState.params));
		localStorage.setItem('od-qvars', JSON.stringify(qvarState.params));
		localStorage.setItem('od-timeline', JSON.stringify(timelineState.keyframes));
	});

	// — Video localStorage persistence ———————————————————
	$effect(() => {
		if (!_ready) return;
		localStorage.setItem('od-video-enabled', String(videoState.enabled));
		localStorage.setItem('od-video-opacity', String(videoState.opacity));
		localStorage.setItem('od-video-advance', videoState.advance);
		localStorage.setItem('od-video-beats', String(videoState.beatsPerCut));
		localStorage.setItem('od-video-reactions', JSON.stringify({ cut: videoState.reactCut, flash: videoState.reactFlash, warp: videoState.reactWarp, hue: videoState.reactHue }));
		localStorage.setItem('od-video-userclips', JSON.stringify(videoState.userClips));
	});

	// — Sync overlays vers output ——————————————————————————
	$effect(() => {
		const list = overlayState.overlays; // force tracking (same pattern as crossfader above)
		sync?.sendOverlays(list);
	});

	// — Video sync to output ————————————————————————————
	$effect(() => {
		const payload = { // force tracking of all fields before sync?.
			enabled: videoState.enabled,
			clip: currentClip,
			opacity: videoState.opacity,
			playbackRate: videoPlaybackRateStep,
			flashOn: videoState.reactFlash,
			hueOn: videoState.reactHue,
		};
		sync?.sendVideo(payload);
	});

	// — Sync compositing (blend + lumaKey + colorKey) vers output, par slot —
	$effect(() => {
		const composites = compositingState.slotComposites;
		if (!sync) return;
		for (let i = 0; i < 4; i++) sync.sendComposite(i, composites[i]);
	});

	// — Sync time params vers output, par slot —
	$effect(() => {
		const params = timeParamsState.params;
		if (!sync) return;
		for (let i = 0; i < 4; i++) sync.sendTime(i, params[i]);
	});

	// — Sync Q-vars vers output, par slot —
	$effect(() => {
		const params = qvarState.params;
		if (!sync) return;
		for (let i = 0; i < 4; i++) sync.sendQVars(i, params[i]);
	});

	// Pushes opacity + compositing config to the local Compositor (Stage).
	$effect(() => {
		const ops = opacities;
		const composites = compositingState.slotComposites;
		if (!compositor) return;
		for (let i = 0; i < 4; i++) compositor.setLayer(i, ops[i], composites[i]);
	});

	// Pushes color params to the Compositor — by assigned bus (same
	// mapping as the old per-canvas style:filter: off → neutral).
	$effect(() => {
		const bus = deckState.deckBus;
		const paramsA = colorState.a;
		const paramsB = colorState.b;
		if (!compositor) return;
		for (let i = 0; i < 4; i++) {
			const color = bus[i] === 'A' ? paramsA : bus[i] === 'B' ? paramsB : DEFAULT_COLOR_PARAMS;
			compositor.setColor(i, color);
		}
	});

	// — Sync strobe vers output ———————————————————————————
	$effect(() => {
		const on = strobeState.on;
		const rate = strobeState.rate;
		const intensity = strobeState.intensity;
		const color = strobeState.color;
		sync?.sendStrobe(on, rate, intensity, color);
	});

	// — MIDI LED feedback (persistent states) ——————————————
	$effect(() => {
		pushLedStates();
	});

	// — Sync color params vers output ————————————————————
	$effect(() => {
		const paramsA = colorState.a;
		sync?.sendColor('A', paramsA);
	});
	$effect(() => {
		const paramsB = colorState.b;
		sync?.sendColor('B', paramsB);
	});

	// — Apply quality to decks + sync output ———————
	$effect(() => {
		if (runStatusState.status !== 'running') return;
		const settings = getQualitySettings(perfState.quality);
		manager.applyQuality(settings);
		sync?.sendQuality(perfState.quality);
	});

	// — Apply target FPS to decks + sync output ————
	$effect(() => {
		if (runStatusState.status !== 'running') return;
		const fps = perfState.targetFps;    // read before sync?. to force tracking
		const mode = perfState.invisibleMode;
		const eco = perfState.invisibleFps;
		manager.setTargetFps(fps);
		sync?.sendPerf({ targetFps: fps, invisibleMode: mode, invisibleFps: eco });
	});

	// — Throttle invisible decks (eco) ——————————————
	$effect(() => {
		if (runStatusState.status !== 'running') return;
		const ops = opacities;          // read first → tracked
		const mode = perfState.invisibleMode;
		const target = perfState.targetFps;
		const eco = perfState.invisibleFps;
		for (let i = 0; i < 4; i++) {
			if (!manager.isRunning(i)) continue;
			const visible = ops[i] > 0.001;
			const wantedFps = (visible || mode === 'off') ? target : (mode === 'eco' ? eco : 0);
			if (mode === 'pause' && !visible) {
				if (!pausedSlots.has(i)) {
					manager.pause(i);
					pausedSlots = new Set([...pausedSlots, i]);
				}
			} else {
				if (pausedSlots.has(i)) {
					manager.resume(i);
					pausedSlots = new Set([...pausedSlots].filter(s => s !== i));
				}
				if (wantedFps !== lastFps[i]) {
					manager.setSlotTargetFps(i, wantedFps);
					lastFps = [...lastFps.slice(0, i), wantedFps, ...lastFps.slice(i + 1)];
				}
			}
		}
	});

	// — Lifecycle ——————————————————————————————————————————
	onMount(async () => {
		if (location.hash.startsWith('#share=')) {
			const encoded = location.hash.slice('#share='.length);
			history.replaceState(null, '', location.pathname + location.search);
			const decoded = await decodeSharedSet(encoded);
			if (decoded) shareSetState.pending = decoded;
		}
		if (isElectron) {
			platform = await window.electronAPI!.getPlatform();
		}
		// Restore saved playlists
		try {
			const savedA = localStorage.getItem('od-pl-a');
			if (savedA) playlistState.aItems = JSON.parse(savedA);
			const savedB = localStorage.getItem('od-pl-b');
			if (savedB) playlistState.bItems = JSON.parse(savedB);
			const savedInterval = localStorage.getItem('od-pl-interval');
			if (savedInterval) playlistState.intervalSec = Number(savedInterval);
			const savedMode = localStorage.getItem('od-pl-mode');
			if (savedMode === 'sequential' || savedMode === 'shuffle') playlistState.mode = savedMode;
			const savedTriggerA = localStorage.getItem('od-beat-trigger-a');
			if (savedTriggerA) {
				try {
					const raw = { ...defaultBeatTriggerConfig(), ...JSON.parse(savedTriggerA) };
					raw.beatsPerChange = clampBeatsPerChange(raw.beatsPerChange);
					raw.offset = clampOffset(raw.offset, raw.beatsPerChange);
					beatSyncState.beatTriggerA = raw;
				} catch { /* ignore corrupt od-beat-trigger-a */ }
			}
			const savedTriggerB = localStorage.getItem('od-beat-trigger-b');
			if (savedTriggerB) {
				try {
					const raw = { ...defaultBeatTriggerConfig(), ...JSON.parse(savedTriggerB) };
					raw.beatsPerChange = clampBeatsPerChange(raw.beatsPerChange);
					raw.offset = clampOffset(raw.offset, raw.beatsPerChange);
					beatSyncState.beatTriggerB = raw;
				} catch { /* ignore corrupt od-beat-trigger-b */ }
			}
			const savedMidi = localStorage.getItem('od-midi-mappings');
			if (savedMidi) midiMappingState.midiMappings = JSON.parse(savedMidi);
			const savedKeymap = localStorage.getItem('od-keymap');
			if (savedKeymap) try { midiMappingState.keymap = { ...DEFAULT_KEYMAP, ...JSON.parse(savedKeymap) }; } catch {}
			const savedQuality = localStorage.getItem('od-quality');
			if (savedQuality === 'low' || savedQuality === 'medium' || savedQuality === 'high') perfState.quality = savedQuality;
			const savedTransition = localStorage.getItem('od-transition');
			if (savedTransition) deckState.transitionTime = Number(savedTransition);
			const savedComposite = localStorage.getItem('od-composite');
			if (savedComposite) {
				try {
					const parsed = JSON.parse(savedComposite);
					if (Array.isArray(parsed) && parsed.length === 4) {
						compositingState.slotComposites = parsed.map((c) => ({ ...DEFAULT_SLOT_COMPOSITE, ...c })) as typeof compositingState.slotComposites;
					}
				} catch { /* ignore corrupt od-composite */ }
			} else {
				// Migration one-shot depuis l'ancien mode global CSS (od-blendmode).
				const savedBlendMode = localStorage.getItem('od-blendmode');
				if (savedBlendMode) {
					const migrated = migrateBlendModeString(savedBlendMode);
					compositingState.slotComposites = compositingState.slotComposites.map((c) => ({ ...c, blend: migrated })) as typeof compositingState.slotComposites;
				}
			}
			const savedSnapshots = localStorage.getItem('od-snapshots');
			if (savedSnapshots) {
				try {
					const parsed = JSON.parse(savedSnapshots);
					if (Array.isArray(parsed)) {
						const arr: (Snapshot | null)[] = new Array(8).fill(null);
						for (let i = 0; i < 8; i++) {
							const s = parsed[i];
							if (s && typeof s.name === 'string' && s.values && typeof s.values === 'object')
								arr[i] = { name: s.name, values: s.values };
						}
						snapshotsState.snapshots = arr;
					}
				} catch { /* ignore corrupt od-snapshots */ }
			}
			const savedSnapDuration = localStorage.getItem('od-snapshot-duration');
			if (savedSnapDuration) {
				const v = Number(savedSnapDuration);
				if (v >= 0.1 && v <= 10) snapshotsState.recallDuration = v;
			}
			const savedTimeParams = localStorage.getItem('od-time-params');
			if (savedTimeParams) {
				try {
					const parsed = JSON.parse(savedTimeParams);
					if (Array.isArray(parsed) && parsed.length === 4) {
						timeParamsState.params = parsed.map((p) => ({ ...defaultTimeParams(), ...p })) as typeof timeParamsState.params;
						for (let slot = 0; slot < 4; slot++) Object.assign(getGlobalTimeParams()[slot], timeParamsState.params[slot]);
					}
				} catch { /* ignore corrupt od-time-params */ }
			}
			const savedQVars = localStorage.getItem('od-qvars');
			if (savedQVars) {
				try {
					const parsed = JSON.parse(savedQVars);
					if (Array.isArray(parsed) && parsed.length === 4) {
						qvarState.params = parsed.map((p) => ({ ...defaultQVarParams(), ...p })) as typeof qvarState.params;
						for (let slot = 0; slot < 4; slot++) Object.assign(getGlobalQVarParams()[slot], { enabled: [...qvarState.params[slot].enabled], value: [...qvarState.params[slot].value] });
					}
				} catch { /* ignore corrupt od-qvars */ }
			}
			const savedTimeline = localStorage.getItem('od-timeline');
			if (savedTimeline) {
				try {
					const parsed = JSON.parse(savedTimeline);
					if (Array.isArray(parsed)) {
						timelineState.keyframes = parsed
							.filter((kf): kf is TimelineKeyframe => kf && typeof kf.slot === 'number' && typeof kf.timeSec === 'number')
							.sort((a, b) => a.timeSec - b.timeSec);
					}
				} catch { /* ignore corrupt od-timeline */ }
			}
			const savedFps = localStorage.getItem('od-target-fps');
			if (savedFps) {
				const v = Number(savedFps);
				if (v === 30 || v === 45 || v === 60) perfState.targetFps = v;
			}
			const savedInvisibleMode = localStorage.getItem('od-invisible-mode');
			if (savedInvisibleMode === 'eco' || savedInvisibleMode === 'pause' || savedInvisibleMode === 'off') {
				perfState.invisibleMode = savedInvisibleMode;
			}
			const savedOverlays = localStorage.getItem('od-overlays');
			if (savedOverlays) overlayState.overlays = JSON.parse(savedOverlays);
			const savedOverlayQueue = localStorage.getItem('od-overlay-queue');
			if (savedOverlayQueue) {
				try {
					const parsed = JSON.parse(savedOverlayQueue);
					overlayState.queueEnabled = !!parsed.enabled;
					const rawTrigger = { ...defaultBeatTriggerConfig(), ...parsed.trigger };
					rawTrigger.beatsPerChange = clampBeatsPerChange(rawTrigger.beatsPerChange);
					rawTrigger.offset = clampOffset(rawTrigger.offset, rawTrigger.beatsPerChange);
					overlayState.queueTrigger = rawTrigger;
					if (parsed.mode === 'sequential' || parsed.mode === 'shuffle') overlayState.queueMode = parsed.mode;
				} catch { /* ignore corrupt od-overlay-queue */ }
			}
			const savedVideoEnabled = localStorage.getItem('od-video-enabled');
			if (savedVideoEnabled) videoState.enabled = savedVideoEnabled === 'true';
			const savedVideoOpacity = localStorage.getItem('od-video-opacity');
			if (savedVideoOpacity) videoState.opacity = Number(savedVideoOpacity);
			const savedVideoAdvance = localStorage.getItem('od-video-advance');
			if (savedVideoAdvance === 'shuffle' || savedVideoAdvance === 'sequential' || savedVideoAdvance === 'manual') videoState.advance = savedVideoAdvance;
			const savedVideoBeats = localStorage.getItem('od-video-beats');
			if (savedVideoBeats) videoState.beatsPerCut = Number(savedVideoBeats);
			const savedVideoReactions = localStorage.getItem('od-video-reactions');
			if (savedVideoReactions) { try { const r = JSON.parse(savedVideoReactions); videoState.reactCut = !!r.cut; videoState.reactFlash = !!r.flash; videoState.reactWarp = !!r.warp; videoState.reactHue = !!r.hue; } catch {} }
			const savedVideoClips = localStorage.getItem('od-video-userclips');
			if (savedVideoClips) { try { videoState.userClips = JSON.parse(savedVideoClips); } catch {} }
			const savedDeckBus = localStorage.getItem('od-deck-bus');
			if (savedDeckBus) {
				try { deckState.deckBus = JSON.parse(savedDeckBus); } catch {}
			}
			const savedLayout = localStorage.getItem('od-layout');
			if (savedLayout === 'stage' || savedLayout === 'mixer') layout = savedLayout;
		} catch {}
		_ready = true; // autorise les $effect de sauvegarde

		// Listeners OSC + remote + Link + screen (Electron-only)
		if (isElectron) {
			oscUnlisten = window.electronAPI?.onOscMsg?.((cmdId, value01) => {
				registry.dispatch(cmdId as CommandId, value01, commandCtx);
			}) ?? null;
			remoteUnlisten = window.electronAPI?.onRemoteCmd?.((cmd, value) => {
				registry.dispatch(cmd as CommandId, value, commandCtx);
			}) ?? null;
			linkUnlisten = window.electronAPI?.onLinkState?.((state) => {
				// Link's phase is on quantum=4 → normalize to 0..1
				clock.syncExternal(state.tempo, state.phase / 4.0);
				electronFeaturesState.link.peers = state.peers;
			}) ?? null;
			outputWindowClosedUnlisten = window.electronAPI?.onOutputWindowClosed?.(() => {
				outputOpen = false;
				outputReadyOnce = false;
				audio?.stopPcmCapture();
			}) ?? null;
			// Load the list of screens
			try {
				const list = await window.electronAPI!.listScreens();
				displays = list;
				const secondary = list.find(d => !d.isPrimary);
				selectedDisplayId = secondary?.id ?? list[0]?.id ?? null;
			} catch {}
		}

		await initPresets();
		await initVideoLoops();
		presetList = buildPresetList();
		if (presetList.length > 0) deckState.presetA = presetList[0].name;
		if (presetList.length > 1) deckState.presetB = presetList[1].name;

		await initCloudPresets();
	});

	onDestroy(() => {
		stopLoopbackIpc();
		if (outputCloseTimer !== null) clearInterval(outputCloseTimer);
		destroyPlaylistEngines();
		manager.destroyAll();
		compositor?.destroy();
		snapshotEngine.cancel();
		timelineEngine.destroy();
		audio?.destroy(); // also calls stopPcmCapture()
		sync?.destroy();
		midi?.destroy();
		beatDetector?.destroy();
		clock.stop();
		oscUnlisten?.();
		remoteUnlisten?.();
		linkUnlisten?.();
		outputWindowClosedUnlisten?.();
		if (electronFeaturesState.osc.active) window.electronAPI?.stopOsc?.();
		if (electronFeaturesState.remote.active) window.electronAPI?.stopRemote?.();
		if (electronFeaturesState.link.active) window.electronAPI?.stopLink?.();
	});

	// — Actions ————————————————————————————————————————————
	// The startup sequence (audio/compositor/sync/beatDetector creation + wiring)
	// moved into visualizer-startup.ts. It returns the 4 new instances rather than
	// assigning them itself, since +page.svelte reads/reassigns these elsewhere too.
	async function startVisualizer() {
		const result = await startVisualizerAction({
			canvases, compositorCanvas, manager, clock, lfoEngine, registry, commandCtx,
			opacities, isElectron,
			getBusPresetA: () => busPresetA,
			getBusPresetB: () => busPresetB,
			getCurrentClip: () => currentClip,
			getVideoPlaybackRateStep: () => videoPlaybackRateStep,
			onBeat,
			getOutputReadyOnce: () => outputReadyOnce,
			setOutputReadyOnce: (v) => { outputReadyOnce = v; },
		});
		if (!result) return;
		audio = result.audio;
		compositor = result.compositor;
		sync = result.sync;
		beatDetector = result.beatDetector;
		// Set status only after the assignments above — the VU-meter $effect
		// gates on `status === 'running' && audio`, and audio isn't reactive,
		// so it must already be non-null by the time status flips.
		runStatusState.status = 'running';
	}

	// captureSystemAudio/connectMic/openDevicePicker/connectDevice/connectLoopback/connectFile
	// — moved into audio-source-actions.ts. Thin wrappers below supply this page's local
	// instances/derived flags (audio-source-actions.ts has no access to +page.svelte locals).
	function captureSystemAudio(): Promise<void> {
		return captureSystemAudioAction(audio, sync, isElectron, platform, effectiveOS);
	}
	function connectMic(): Promise<void> {
		return connectMicAction(audio);
	}
	function openDevicePicker(): Promise<void> {
		return openDevicePickerAction(loopbackSupported);
	}
	function connectDevice(device: MediaDeviceInfo): Promise<void> {
		return connectDeviceAction(device, audio, sync);
	}
	function connectLoopback(device: {id: number; name: string; maxInputChannels: number; maxOutputChannels: number; defaultSampleRate: number}): Promise<void> {
		return connectLoopbackAction(device, audio, sync, manager);
	}
	function connectFile(): Promise<void> {
		return connectFileAction(audio, audioEl);
	}

	function onFileChange(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0];
		if (!file || !audioEl) return;
		audioEl.src = URL.createObjectURL(file);
		if (runStatusState.status === 'running') connectFile();
	}

	// selectPreset/loadImportedMilkPreset moved into deck-preset-actions.ts.
	function selectPreset(name: string): Promise<void> {
		return selectPresetAction(name, manager, sync, primaryPreset);
	}

	// onBeat/toggleBeatSync/tapTempo/clearManualBpm moved into beat-tempo-actions.ts.
	function onBeat() {
		onBeatAction(sync, clock, applyMidiAction);
	}

	// — Overlay helpers — extracted into overlay-store.svelte.ts (addOverlayFromFile,
	// onOverlayFilePick, addTextOverlay, onVisualizerDragOver, removeOverlay, updateOverlay)
	async function onVisualizerDrop(e: DragEvent) {
		e.preventDefault();
		overlayState.dragOver = false;
		if (!e.dataTransfer?.files.length) return;
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const x = (e.clientX - rect.left) / rect.width;
		const y = (e.clientY - rect.top) / rect.height;
		for (const f of Array.from(e.dataTransfer.files)) {
			// .milk/.prjm have no reliable browser MIME type (f.type is ''),
			// so this is checked by extension rather than f.type like the
			// video/image branches below.
			if (isMilkPresetFilename(f.name)) {
				await loadImportedMilkPreset(f);
				continue;
			}
			if (f.type.startsWith('video/')) {
				await addVideoFromFile(f);
				continue;
			}
			if (!f.type.startsWith('image/')) continue;
			await new Promise<void>((res) => {
				const reader = new FileReader();
				reader.onload = async () => {
					const dataUrl = reader.result as string;
					await addOverlayAtPosition(f.name.replace(/\.[^.]+$/, ''), dataUrl, x, y);
					res();
				};
				reader.readAsDataURL(f);
			});
		}
	}

	/** Import a dropped .milk/.prjm preset directly into activeSlot (mirrors selectPreset,
	 * minus the loadPresetData lookup — the converted data is already in hand). */
	function loadImportedMilkPreset(file: File): Promise<void> {
		return loadImportedMilkPresetAction(file, manager, sync, primaryPreset);
	}

	function toggleBeatSync(deck: 'A' | 'B') {
		toggleBeatSyncAction(deck);
	}

	function tapTempo() {
		tapTempoAction(clock);
	}

	function clearManualBpm() {
		clearManualBpmAction(clock);
	}

	// toggleMidi's connection/dispatch/clock-IN logic moved into midi-connection-actions.ts.
	async function toggleMidi() {
		midi = await toggleMidiAction(midi, {
			registry, commandCtx, clock, getCommandCurrentValue, getCommandLedState, pushLedStates,
		});
	}

	function startLearn(action: CommandId) {
		midiMappingState.learningAction = midiMappingState.learningAction === action ? null : action;
	}

	function clearMapping(action: CommandId) {
		clearMidiMapping(action);
	}

	function clearKeyBinding(cmdId: CommandId) {
		const key = keyById.get(cmdId);
		if (!key) return;
		removeKeyBinding(key);
	}

	function doResetKeymap() {
		resetMidiKeymap();
	}

	function applyMidiAction(action: CommandId, value: number) {
		if (runStatusState.status !== 'running') return;
		registry.dispatch(action, value / 127, commandCtx);
	}

	/** Read the current value (0..1) of a range command, for soft-takeover. */
	function getCommandCurrentValue(id: CommandId): number | null {
		if (id === 'crossfader') return deckState.crossfader;
		const colorMatch = id.match(/^color-(\w+)-([ab])$/);
		if (colorMatch) {
			const e = COLOR_CMDS.find(([s]) => s === colorMatch[1]);
			return e ? (colorMatch[2] === 'a' ? colorState.a : colorState.b)[e[1]] : null;
		}
		const compositeMatch = id.match(/^(composite-blend|lumakey-black|lumakey-white|colorkey-hue|colorkey-tolerance)-([0-3])$/);
		if (compositeMatch) {
			const e = COMPOSITE_CMDS.find(([prefix]) => prefix === compositeMatch[1]);
			return e ? e[3](compositingState.slotComposites[Number(compositeMatch[2])]) : null;
		}
		const timeMatch = id.match(/^(time-speed|time-zoom|time-rot|time-warp|time-dx|time-dy|time-stretch|time-wave)-([0-3])$/);
		if (timeMatch) {
			const e = TIME_CMDS.find(([prefix]) => prefix === timeMatch[1]);
			return e ? timeParamsState.params[Number(timeMatch[2])][e[2]] / 2 : null;
		}
		const qvarMatch = id.match(/^qvar-(\d+)-([0-3])$/);
		if (qvarMatch) {
			const n = Number(qvarMatch[1]);
			if (n < 1 || n > 32) return null;
			return (qvarState.params[Number(qvarMatch[2])].value[n - 1] + 2) / 4;
		}
		return null;
	}

	function getCommandLedState(id: CommandId): boolean | null {
		if (id === 'strobe-toggle') return strobeState.on;
		if (id === 'playlist-toggle-a') return playlistState.aPlaying;
		if (id === 'playlist-toggle-b') return playlistState.bPlaying;
		if (id === 'playlist-toggle-active') return activeDeck === 'A' ? playlistState.aPlaying : playlistState.bPlaying;
		return null;
	}

	// Reads strobeState.on/playlistState.aPlaying/bPlaying/activeDeck/midiMappingState.midiMappings BEFORE checking
	// `midi` (non-reactive variable) — otherwise an $effect calling this function would never
	// track those $state values if it first ran before the MIDI connection (the same
	// Svelte 5 gotcha documented for optional chaining in an $effect).
	function pushLedStates() {
		const strobe = strobeState.on;
		const plA = playlistState.aPlaying;
		const plB = playlistState.bPlaying;
		const active = activeDeck;
		const kStrobe = midiMappingState.midiMappings['strobe-toggle'];
		const kA = midiMappingState.midiMappings['playlist-toggle-a'];
		const kB = midiMappingState.midiMappings['playlist-toggle-b'];
		const kActive = midiMappingState.midiMappings['playlist-toggle-active'];
		if (!midi) return;
		if (kStrobe) midi.sendFeedback(kStrobe, strobe);
		if (kA) midi.sendFeedback(kA, plA);
		if (kB) midi.sendFeedback(kB, plB);
		if (kActive) midi.sendFeedback(kActive, active === 'A' ? plA : plB);
	}

	// selectPresetForDeck/buildCurrentSharedSet/copyShareLink/applyPendingSharedSet/
	// cancelPendingSharedSet moved into share-set-actions.ts.
	function selectPresetForDeck(deck: 'A' | 'B', name: string): Promise<void> {
		return selectPresetForDeckAction(deck, name, manager, sync);
	}
	function buildCurrentSharedSet(): SharedSet {
		return buildCurrentSharedSetAction();
	}
	function copyShareLink(): Promise<void> {
		return copyShareLinkAction();
	}
	function applyPendingSharedSet(): Promise<void> {
		return applyPendingSharedSetAction(manager, sync);
	}
	function cancelPendingSharedSet(): void {
		cancelPendingSharedSetAction();
	}

	function openOutput() {
		outputWinRef = window.open('/output', 'opendrop-output', 'width=1280,height=720');
		outputOpen = true;
		// Give the window ~800ms to init, then push current state
		setTimeout(() => {
			sync?.sendPreset('A', busPresetA);
			sync?.sendPreset('B', busPresetB);
			sync?.sendCrossfader(deckState.crossfader);
			if (audioSourceState.currentDeviceId) sync?.sendSource(audioSourceState.currentDeviceId);
		}, 800);
		// Poll for output window closure to stop PCM capture and release resources.
		if (outputCloseTimer !== null) clearInterval(outputCloseTimer);
		outputCloseTimer = setInterval(() => {
			if (outputWinRef?.closed) {
				audio?.stopPcmCapture();
				outputOpen = false;
				outputReadyOnce = false;
				outputWinRef = null;
				clearInterval(outputCloseTimer!);
				outputCloseTimer = null;
			}
		}, 1500);
	}

	// openOutputFullscreen/onResize moved into output-window-actions.ts.
	function openOutputFullscreen(): Promise<void> {
		return openOutputFullscreenAction(isElectron, selectedDisplayId, sync, () => busPresetA, () => busPresetB, (v) => { outputOpen = v; });
	}
	function onResize(): void {
		onResizeAction(canvases, compositorCanvas, manager, compositor);
	}

	// toggleNdi/V4l2/Spout/Osc/Remote/Link — moved into electron-features-actions.ts
	function toggleLink(): Promise<void> {
		return toggleLinkAction(clock);
	}

	async function startSlot(slot: number) {
		if (!audio || runStatusState.status !== 'running') return;
		const q = getQualitySettings(perfState.quality);
		const name = presets4[slot];
		const presetData = name ? await loadPresetData(name) : null;
		await manager.start(slot, audio.ctx, audio.gainNode, q, presetData);
		deckState.slotEpoch++;
	}

	function pauseSlot(slot: number) {
		manager.pause(slot);
		deckState.slotEpoch++;
	}

	function cycleBus(slot: number) {
		const order: Array<'A' | 'B' | 'off'> = ['A', 'B', 'off'];
		const next = order[(order.indexOf(deckState.deckBus[slot]) + 1) % order.length];
		deckState.deckBus = deckState.deckBus.map((b, i) => (i === slot ? next : b)) as Array<'A' | 'B' | 'off'>;
	}

	function isRunning(slot: number): boolean {
		return manager.isRunning(slot);
	}

	function onKeydown(e: KeyboardEvent) {
		const tag = (e.target as HTMLElement).tagName;
		if (tag === 'INPUT' || tag === 'TEXTAREA') return;

		if (learningKey !== null) {
			if (e.key === 'Escape') { learningKey = null; e.preventDefault(); return; }
			midiMappingState.keymap = { ...midiMappingState.keymap, [e.key]: learningKey };
			learningKey = null;
			e.preventDefault();
			return;
		}

		const action = midiMappingState.keymap[e.key];
		if (!action) return;
		e.preventDefault();
		if (runStatusState.status !== 'running') return;
		registry.dispatch(action, 1, commandCtx);
	}
</script>

<svelte:window onresize={onResize} onkeydown={onKeydown} />
<audio bind:this={audioEl} style="display:none" crossorigin="anonymous"></audio>

<main>
{#snippet audioSection()}
  <SidebarAudio
    sourceLabel={audioSourceState.sourceLabel}
    status={runStatusState.status}
    {effectiveOS}
    {vuLevel}
    sourceError={runStatusState.sourceError}
    showSystemAudioHelp={audioSourceState.showSystemAudioHelp}
    showDevicePicker={audioSourceState.showDevicePicker}
    audioDevices={audioSourceState.devices}
    outputDevices={audioSourceState.outputDevices}
    {loopbackSupported}
    audioElHasSrc={!!audioEl?.src}
    onConnectMic={connectMic}
    onOpenDevicePicker={openDevicePicker}
    onCaptureSystemAudio={captureSystemAudio}
    onConnectFile={connectFile}
    {onFileChange}
    onConnectDevice={connectDevice}
    onConnectLoopback={connectLoopback}
    onDismissSystemAudioHelp={() => { audioSourceState.showSystemAudioHelp = false }}
    onDismissDevicePicker={() => { audioSourceState.showDevicePicker = false }}
  />
{/snippet}
{#snippet videoSection()}
  <SidebarVideo
    videoEnabled={videoState.enabled}
    videoOpacity={videoState.opacity}
    videoAdvance={videoState.advance}
    videoBeatsPerCut={videoState.beatsPerCut}
    vrCut={videoState.reactCut}
    vrFlash={videoState.reactFlash}
    vrWarp={videoState.reactWarp}
    vrHue={videoState.reactHue}
    currentClipIndex={videoState.currentClipIndex}
    {allClips}
    liveActive={!!videoState.liveDeviceId}
    liveLabel={videoState.liveLabel}
    {onToggleLiveCamera}
    ndiActive={!!videoState.ndiSourceName}
    ndiSourceLabel={videoState.ndiSourceName ?? ''}
    {ndiSources}
    {onFindNdiSources}
    {onSelectNdiSource}
    {onClearNdiSource}
    onToggleVideo={() => { videoState.enabled = !videoState.enabled }}
    onOpacityChange={(v) => { videoState.opacity = v }}
    onAdvanceChange={(v) => { videoState.advance = v }}
    onBeatsPerCutChange={(v) => { videoState.beatsPerCut = v }}
    onToggleVrCut={() => { videoState.reactCut = !videoState.reactCut }}
    onToggleVrFlash={() => { videoState.reactFlash = !videoState.reactFlash }}
    onToggleVrWarp={() => { videoState.reactWarp = !videoState.reactWarp }}
    onToggleVrHue={() => { videoState.reactHue = !videoState.reactHue }}
    onSelectClip={(i) => { videoState.currentClipIndex = i }}
    onRemoveClip={(i) => removeVideoClip(i)}
    onAddVideo={onVideoFilePick}
  />
{/snippet}
{#snippet qualitySection()}
	<SidebarQuality
		quality={perfState.quality}
		targetFps={perfState.targetFps}
		invisibleMode={perfState.invisibleMode}
		status={runStatusState.status}
		{fps}
		onQualityChange={(q) => { perfState.quality = q; }}
		onTargetFpsChange={(n) => { perfState.targetFps = n; }}
		onInvisibleModeChange={(m) => { perfState.invisibleMode = m; }}
	/>
{/snippet}
{#snippet outputSection()}
	<SidebarOutput
		status={runStatusState.status}
		{isElectron}
		{outputOpen}
		{displays}
		{selectedDisplayId}
		{showStreamPanel}
		{platform}
		ndiActive={electronFeaturesState.ndi.active}
		v4l2Active={electronFeaturesState.v4l2.active}
		spoutActive={electronFeaturesState.spout.active}
		ndiError={electronFeaturesState.ndi.error}
		v4l2Error={electronFeaturesState.v4l2.error}
		spoutError={electronFeaturesState.spout.error}
		onOpenOutput={openOutput}
		onOpenOutputFullscreen={openOutputFullscreen}
		onToggleStreamPanel={() => { showStreamPanel = !showStreamPanel; }}
		onSelectDisplay={(id) => { selectedDisplayId = id; }}
		onToggleNdi={toggleNdi}
		onToggleV4l2={toggleV4l2}
		onToggleSpout={toggleSpout}
	/>
{/snippet}
{#snippet midiSection()}
	<SidebarMidi
		{midiSupported}
		midiConnected={midiConnectionState.connected}
		midiDeviceNames={midiConnectionState.deviceNames}
		midiClockBpm={midiConnectionState.clockBpm}
		learningAction={midiMappingState.learningAction}
		midiMappings={midiMappingState.midiMappings}
		{registry}
		onToggleMidi={toggleMidi}
		onStartLearn={startLearn}
		onClearMapping={clearMapping}
	/>
{/snippet}
{#snippet keyboardSection()}
	<SidebarKeymap
		{learningKey}
		{keyById}
		{registry}
		onResetKeymap={doResetKeymap}
		onToggleLearnKey={(id) => { learningKey = learningKey === id ? null : id; }}
		onClearKeyBinding={clearKeyBinding}
	/>
{/snippet}
{#snippet strobeSection()}
	<SidebarStrobe
		strobeOn={strobeState.on}
		strobeRate={strobeState.rate}
		strobeIntensity={strobeState.intensity}
		strobeColor={strobeState.color}
		onToggleStrobe={() => { strobeState.on = !strobeState.on; }}
		onRateChange={(r) => { strobeState.rate = r; }}
		onIntensityChange={(v) => { strobeState.intensity = v; }}
		onColorChange={(c) => { strobeState.color = c; }}
	/>
{/snippet}
{#snippet lfoSection()}
	<SidebarLfo {lfoSlots} {registry} />
{/snippet}
{#snippet colorSection()}
	<SidebarColor
		colorParamsA={colorState.a}
		colorParamsB={colorState.b}
		onUpdateA={(p) => { colorState.a = p; }}
		onUpdateB={(p) => { colorState.b = p; }}
	/>
{/snippet}
{#snippet compositeSection()}
	<SidebarComposite
		{mixerSelectedSlot}
		composite={compositingState.slotComposites[mixerSelectedSlot]}
		onUpdate={(patch) => updateComposite(mixerSelectedSlot, patch)}
	/>
{/snippet}
{#snippet snapshotSection()}
	<SidebarSnapshot
		snapshotRecallDuration={snapshotsState.recallDuration}
		snapshots={snapshotsState.snapshots}
		onDurationChange={(v) => { snapshotsState.recallDuration = v; }}
		onRenameSnapshot={renameSnapshot}
		onSaveSnapshot={saveSnapshot}
		onRecallSnapshot={recallSnapshot}
		onClearSnapshot={clearSnapshot}
	/>
{/snippet}
{#snippet timelineSection()}
	<SidebarTimeline
		timelinePlaying={timelineState.playing}
		timelineKeyframes={timelineState.keyframes}
		snapshots={snapshotsState.snapshots}
		onTogglePlay={toggleTimelinePlay}
		onUpdateKeyframe={updateTimelineKeyframe}
		onRemoveKeyframe={removeTimelineKeyframe}
		onAddKeyframe={addTimelineKeyframe}
	/>
{/snippet}
{#snippet shareSection()}
	<SidebarShare
		shareSetName={shareSetState.name}
		shareCopyLabel={shareSetState.copyLabel}
		{nonShareableOverlayCount}
		onNameChange={(name) => { shareSetState.name = name; }}
		onCopyShareLink={copyShareLink}
	/>
{/snippet}
{#snippet timeSection()}
	<SidebarTime
		{mixerSelectedSlot}
		timeParams={timeParamsState.params[mixerSelectedSlot]}
		onUpdate={(patch) => updateTimeParams(mixerSelectedSlot, patch)}
		onReset={() => updateTimeParams(mixerSelectedSlot, defaultTimeParams())}
	/>
{/snippet}
{#snippet qvarSection()}
	<SidebarQvar
		{mixerSelectedSlot}
		qvar={qvarState.params[mixerSelectedSlot]}
		onAddWatch={(n) => addQVarWatch(mixerSelectedSlot, n)}
		onUpdateValue={(n, value) => updateQVarValue(mixerSelectedSlot, n, value)}
		onRemoveWatch={(n) => removeQVarWatch(mixerSelectedSlot, n)}
	/>
{/snippet}
{#snippet electronSection()}
	<SidebarElectron
		{isElectron}
		oscActive={electronFeaturesState.osc.active}
		oscPort={electronFeaturesState.osc.port}
		oscError={electronFeaturesState.osc.error}
		remoteActive={electronFeaturesState.remote.active}
		remoteUrl={electronFeaturesState.remote.url}
		remoteError={electronFeaturesState.remote.error}
		linkActive={electronFeaturesState.link.active}
		linkPeers={electronFeaturesState.link.peers}
		linkError={electronFeaturesState.link.error}
		onToggleOsc={toggleOsc}
		onOscPortChange={(port) => { electronFeaturesState.osc.port = port; }}
		onToggleRemote={toggleRemote}
		onToggleLink={toggleLink}
	/>
{/snippet}
{#if shareSetState.pending}
	<div class="overlay share-confirm-overlay">
		<p class="tagline">Load the shared set « {shareSetState.pending.name || 'Unnamed'} » ?</p>
		<p style="font-size:11px;color:#aaa;max-width:320px;text-align:center">
			Replaces your current visual state (presets, color, snapshots, timeline...).
		</p>
		<div style="display:flex;gap:8px">
			<button class="btn-primary" onclick={applyPendingSharedSet}>Load</button>
			<button class="btn-secondary" onclick={cancelPendingSharedSet}>Cancel</button>
		</div>
	</div>
{/if}
<div
	class="visualizer-wrap"
	class:drag-over={overlayState.dragOver}
	class:mixer-hidden={layout !== 'stage'}
	ondragover={onVisualizerDragOver}
	ondragleave={() => { overlayState.dragOver = false; }}
	ondrop={onVisualizerDrop}
	role="region"
	aria-label="Visualizer"
>
	<!-- Video loop — first child = behind the decks -->
	<VideoLayer clip={currentClip} opacity={videoState.opacity} beat={beatSyncState.beat} playbackRate={videoState.playbackRate} flashOn={videoState.reactFlash} hueOn={videoState.reactHue} />
	<!-- Deck canvases — 4 slots, texture sources for the Compositor (hidden) -->
	{#each [0, 1, 2, 3] as i}
		<canvas bind:this={canvases[i]} class="deck-src"></canvas>
	{/each}
	<!-- Composited render (blend + lumaKey + colorKey per slot) -->
	<canvas bind:this={compositorCanvas} class="deck-canvas" style:mix-blend-mode={videoState.enabled ? 'screen' : 'normal'}></canvas>
	<!-- Overlay sprites -->
	<OverlayLayer overlays={overlayState.overlays} beat={beatSyncState.beat} visibleIds={overlayVisibleIds} />
	<!-- Strobe flash — top z-index, pointer-events none -->
	{#if strobeState.on && strobeState.flash}
		<div class="strobe-flash" style="background:{strobeState.color};opacity:{strobeState.intensity}"></div>
	{/if}

	{#if runStatusState.status === 'idle' && !shareSetState.pending}
		<div class="overlay">
			<h1 class="logo">OpenDrop</h1>
			<p class="tagline">Milkdrop visualizer — web-first</p>
			<button class="btn-primary" onclick={startVisualizer}>▶ Start</button>
		</div>
	{/if}

	{#if runStatusState.status === 'error'}
		<div class="overlay error">
			<p>⚠ {runStatusState.errorMsg}</p>
			<button class="btn-secondary" onclick={() => { runStatusState.status = 'idle'; runStatusState.errorMsg = ''; }}>Retry</button>
		</div>
	{/if}
</div>

{#if layout === 'stage'}
	<aside class="controls">
		<!-- Layout toggle -->
		<div class="controls-section">
			<LayoutToggle {layout} onToggle={(l) => { layout = l }} />
		</div>

		<!-- Audio source -->
		<SidebarAudio
			sourceLabel={audioSourceState.sourceLabel}
			status={runStatusState.status}
			{effectiveOS}
			{vuLevel}
			sourceError={runStatusState.sourceError}
			showSystemAudioHelp={audioSourceState.showSystemAudioHelp}
			showDevicePicker={audioSourceState.showDevicePicker}
			audioDevices={audioSourceState.devices}
			outputDevices={audioSourceState.outputDevices}
			{loopbackSupported}
			audioElHasSrc={!!audioEl?.src}
			onConnectMic={connectMic}
			onOpenDevicePicker={openDevicePicker}
			onCaptureSystemAudio={captureSystemAudio}
			onConnectFile={connectFile}
			{onFileChange}
			onConnectDevice={connectDevice}
			onConnectLoopback={connectLoopback}
			onDismissSystemAudioHelp={() => { audioSourceState.showSystemAudioHelp = false; }}
			onDismissDevicePicker={() => { audioSourceState.showDevicePicker = false; }}
		/>

		<!-- Mixer -->
		<div class="controls-section">
			<span class="label">Mixer</span>
			<div class="deck-cards-row">
				{#each (['A', 'B', 'C', 'D'] as const) as letter, i}
					<DeckCard
						{letter}
						canvas={canvases[i]}
						presetName={presets4[i]}
						isActive={deckState.activeSlot === i}
						isLive={opacities[i] > 0.5}
						bus={deckState.deckBus[i]}
						running={isRunning(i)}
						onSelect={() => { deckState.activeSlot = i }}
						onCycleBus={() => cycleBus(i)}
						onToggleRun={() => isRunning(i) ? pauseSlot(i) : startSlot(i)}
					/>
				{/each}
			</div>
			<div class="crossfader-row">
				<span class="cf-label" class:bright={deckState.crossfader < 0.2}>A</span>
				<input class="crossfader" type="range" min="0" max="1" step="0.01" bind:value={deckState.crossfader} />
				<span class="cf-label" class:bright={deckState.crossfader > 0.8}>B</span>
			</div>
			<div class="transition-row">
				<span class="transition-label">Fade</span>
				<input class="transition-slider" type="range" min="0" max="5" step="0.1" bind:value={deckState.transitionTime} title="Preset transition duration (s)" />
				<span class="transition-value">{deckState.transitionTime.toFixed(1)}s</span>
				<button class="btn-sm" onclick={() => { deckState.transitionTime = 0 }} title="Hard cut">Hard Cut</button>
			</div>
			<button
				class="btn-sm preset-browser-toggle"
				class:active={showPresetBrowser}
				onclick={() => { showPresetBrowser = !showPresetBrowser }}
				type="button"
			>
				Presets {showPresetBrowser ? '▼' : '▲'}
			</button>
		</div>

		<!-- Playlist -->
		<SidebarPlaylist
			playlistMode={playlistState.mode}
			playlistIntervalSec={playlistState.intervalSec}
			beatSyncA={beatSyncState.beatSyncA}
			beatSyncB={beatSyncState.beatSyncB}
			autoXfade={beatSyncState.autoXfade}
			beatsPerChange={beatSyncState.beatsPerChange}
			beatTriggerA={beatSyncState.beatTriggerA}
			beatTriggerB={beatSyncState.beatTriggerB}
			detectedBpm={audioSourceState.detectedBpm}
			manualBpm={audioSourceState.manualBpm}
			playlistAItems={playlistState.aItems}
			playlistBItems={playlistState.bItems}
			playlistAPlaying={playlistState.aPlaying}
			playlistBPlaying={playlistState.bPlaying}
			audioRunning={runStatusState.status === 'running'}
			presetA={deckState.presetA}
			presetB={deckState.presetB}
			lockA={beatSyncState.lockA}
			lockB={beatSyncState.lockB}
			onModeChange={(m) => { playlistState.mode = m }}
			onIntervalChange={(s) => { playlistState.intervalSec = s }}
			onBeatsPerChangeChange={(n) => { beatSyncState.beatsPerChange = n }}
			onBeatTriggerAChange={updateBeatTriggerA}
			onBeatTriggerBChange={updateBeatTriggerB}
			onTapTempo={tapTempo}
			onClearManualBpm={clearManualBpm}
			onToggleBeatSyncA={() => toggleBeatSync('A')}
			onToggleBeatSyncB={() => toggleBeatSync('B')}
			onToggleAutoXfade={() => { beatSyncState.autoXfade = !beatSyncState.autoXfade; resetAutoXfadeCount(); }}
			onTogglePlaylistA={() => togglePlaylist('A')}
			onTogglePlaylistB={() => togglePlaylist('B')}
			onPlaylistNext={(deck) => playlistNext(deck)}
			onPlaylistPrev={(deck) => playlistPrev(deck)}
			onRemoveFromPlaylistA={(name) => removeFromPlaylist('A', name)}
			onRemoveFromPlaylistB={(name) => removeFromPlaylist('B', name)}
			onToggleLockA={() => { beatSyncState.lockA = !beatSyncState.lockA }}
			onToggleLockB={() => { beatSyncState.lockB = !beatSyncState.lockB }}
			onExportPlaylists={exportPlaylists}
			onImportPlaylists={importPlaylists}
		/>

		<!-- Overlays -->
		<SidebarOverlays
			overlays={overlayState.overlays}
			onAddOverlays={onOverlayFilePick}
			onAddText={addTextOverlay}
			onRemoveOverlay={(id) => removeOverlay(id)}
			onUpdateOverlay={(id, patch) => updateOverlay(id, patch)}
			overlayQueueEnabled={overlayState.queueEnabled}
			overlayQueueMode={overlayState.queueMode}
			overlayQueueTrigger={overlayState.queueTrigger}
			onToggleOverlayQueue={toggleOverlayQueue}
			onOverlayQueueModeChange={(mode) => setOverlayQueueMode(mode)}
			onOverlayQueueTriggerChange={(patch) => updateOverlayQueueTrigger(patch)}
			onOverlayQueueNext={() => advanceOverlayQueue(1)}
			onOverlayQueuePrev={() => advanceOverlayQueue(-1)}
		/>

		<!-- Presets cloud -->
		<SidebarCloudPresets
			presets={cloudPresetsState.presets}
			token={cloudPresetsState.token}
			error={cloudPresetsState.error}
			onUploadFile={onCloudPresetFilePick}
			onCopyToken={copyCloudToken}
			copyLabel={cloudPresetsState.copyLabel}
			onLinkDevice={linkCloudDevice}
			onLoadPreset={selectPreset}
			onRename={renameCloudPresetEntry}
			onDelete={deleteCloudPresetEntry}
		/>

		<!-- Video loops -->
		<SidebarVideo
			videoEnabled={videoState.enabled}
			videoOpacity={videoState.opacity}
			videoAdvance={videoState.advance}
			videoBeatsPerCut={videoState.beatsPerCut}
			vrCut={videoState.reactCut}
			vrFlash={videoState.reactFlash}
			vrWarp={videoState.reactWarp}
			vrHue={videoState.reactHue}
			currentClipIndex={videoState.currentClipIndex}
			{allClips}
			liveActive={!!videoState.liveDeviceId}
			liveLabel={videoState.liveLabel}
			{onToggleLiveCamera}
			ndiActive={!!videoState.ndiSourceName}
			ndiSourceLabel={videoState.ndiSourceName ?? ''}
			{ndiSources}
			{onFindNdiSources}
			{onSelectNdiSource}
			{onClearNdiSource}
			onToggleVideo={() => { videoState.enabled = !videoState.enabled }}
			onOpacityChange={(v) => { videoState.opacity = v }}
			onAdvanceChange={(v) => { videoState.advance = v }}
			onBeatsPerCutChange={(v) => { videoState.beatsPerCut = v }}
			onToggleVrCut={() => { videoState.reactCut = !videoState.reactCut }}
			onToggleVrFlash={() => { videoState.reactFlash = !videoState.reactFlash }}
			onToggleVrWarp={() => { videoState.reactWarp = !videoState.reactWarp }}
			onToggleVrHue={() => { videoState.reactHue = !videoState.reactHue }}
			onSelectClip={(i) => { videoState.currentClipIndex = i }}
			onRemoveClip={(i) => removeVideoClip(i)}
			onAddVideo={onVideoFilePick}
		/>

		{@render qualitySection()}

		{@render outputSection()}

		{@render midiSection()}

		{@render keyboardSection()}

		{@render strobeSection()}

		{@render lfoSection()}

		{@render colorSection()}

		{@render electronSection()}

	</aside>
	<PresetBrowser
		presets={presetList}
		isOpen={showPresetBrowser}
		{activeDeck}
		targetSlot={deckState.activeSlot}
		playlistAItems={playlistState.aItems}
		playlistBItems={playlistState.bItems}
		onClose={() => { showPresetBrowser = false }}
		onLoadPreset={selectPreset}
		onAddToPlaylist={addToPlaylist}
	/>
{:else}
  <MixerLayout
    {canvases}
    {presets4}
    deckBus={deckState.deckBus}
    {runningCount}
    {isRunning}
    selectedSlot={mixerSelectedSlot}
    crossfader={deckState.crossfader}
    transitionTime={deckState.transitionTime}
    {presetList}
    playlistAItems={playlistState.aItems}
    playlistBItems={playlistState.bItems}
    {layout}
    onStartSlot={startSlot}
    onPauseSlot={pauseSlot}
    onSelectSlot={(s) => { mixerSelectedSlot = s }}
    onCycleBus={cycleBus}
    onCrossfaderChange={(v) => { deckState.crossfader = v }}
    onTransitionChange={(v) => { deckState.transitionTime = v }}
    onLoadPreset={selectPreset}
    onAddToPlaylist={addToPlaylist}
    onLayoutToggle={(l) => { layout = l }}
    {audioSection}
    {videoSection}
    {qualitySection}
    {outputSection}
    {midiSection}
    {keyboardSection}
    {strobeSection}
    {lfoSection}
    {colorSection}
    {compositeSection}
    {snapshotSection}
    {timelineSection}
    {shareSection}
    {timeSection}
    {qvarSection}
    {electronSection}
  />
{/if}
</main>

<style>
	:global(*, *::before, *::after) {
		box-sizing: border-box;
		margin: 0;
		padding: 0;
	}

	:global(html, body) {
		width: 100%; height: 100%;
		background: #07071a;
		color: #ddddf5;
		font-family: 'Inter', system-ui, sans-serif;
		font-size: 13px;
		overflow: hidden;
	}

	/* Scrollbars */
	:global(::-webkit-scrollbar) { width: 6px; }
	:global(::-webkit-scrollbar-track) { background: transparent; }
	:global(::-webkit-scrollbar-thumb) { background: #2a2a5a; border-radius: 3px; }
	:global(::-webkit-scrollbar-thumb:hover) { background: var(--accent); }

	main { display: flex; width: 100vw; height: 100vh; overflow: hidden; }

	.visualizer-wrap { flex: 1; position: relative; background: #000; min-width: 0; isolation: isolate; }
	/* In mixer mode, the wrap stays mounted (never destroyed — the canvases keep their link
	   with the engine) but exits the flex flow and hides: visibility, not display:none, to keep
	   clientWidth/Height non nuls (rAF + captureStream restent vivants). */
	.visualizer-wrap.mixer-hidden { position: absolute; inset: 0; visibility: hidden; pointer-events: none; }
	.strobe-flash { position: absolute; inset: 0; z-index: 200; pointer-events: none; }

	.deck-canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
	/* Texture sources for the Compositor — real, sized, DOM-attached (Butterchurn +
	   DeckCard captureStream need them) but not shown; .deck-canvas above is visible. */
	.deck-src { position: absolute; inset: 0; width: 100%; height: 100%; display: block; visibility: hidden; }

	/* Overlay start screen */
	.overlay {
		position: absolute; inset: 0;
		display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 1.2rem;
		background: rgba(7, 7, 26, 0.82);
		backdrop-filter: blur(2px);
		z-index: 10;
	}

	.overlay.error { background: rgba(20, 0, 10, 0.9); color: #ff6090; }
	/* Rendered outside .visualizer-wrap (see template) so it stays visible even when the wrap
	   is visibility:hidden in mixer layout — fixed to the viewport, not the wrap. */
	.share-confirm-overlay { position: fixed; z-index: 300; }

	.logo {
		font-size: 3rem; font-weight: 800; letter-spacing: 0.15em;
		background: linear-gradient(135deg, var(--accent) 0%, var(--cyan) 100%);
		-webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;
		filter: drop-shadow(0 0 24px rgba(255, 45, 120, 0.6));
	}

	.tagline { color: #6666aa; font-size: 12px; letter-spacing: 0.08em; margin-bottom: 0.5rem; }

	/* ── Sidebar ── */
	.controls {
		width: 268px; flex-shrink: 0;
		background: #0b0b20;
		border-left: 1px solid #1a1a42;
		display: flex; flex-direction: column; overflow-y: auto;
		/* subtle scanline texture */
		background-image: repeating-linear-gradient(
			0deg, transparent, transparent 2px,
			rgba(255,255,255,0.012) 2px, rgba(255,255,255,0.012) 4px
		);
	}

	@media (max-width: 1100px) {
		.controls { width: 240px; }
	}

	.controls-section {
		padding: var(--sp-3);
		border-bottom: 1px solid var(--border-subtle);
		display: flex; flex-direction: column; gap: 0.4rem;
	}

	.label {
		font-size: 10px; text-transform: uppercase; letter-spacing: 0.08em;
		color: var(--accent); font-weight: 600;
	}

	/* ── Mixer ── */
	.deck-cards-row { display: flex; gap: 0.4rem; }

	.preset-browser-toggle {
		width: 100%;
		text-align: center;
		margin-top: 0.2rem;
	}

	.crossfader-row { display: flex; align-items: center; gap: 0.4rem; }

	.cf-label {
		font-size: 11px; font-weight: 700; color: var(--text-muted);
		width: 12px; text-align: center; transition: color var(--t-fast);
	}

	.cf-label.bright { color: var(--accent); text-shadow: 0 0 8px var(--accent-glow); }

	.crossfader { flex: 1; accent-color: var(--accent); cursor: pointer; }

	.transition-row { display: flex; align-items: center; gap: 0.4rem; margin-top: 0.3rem; }
	.transition-label { font-size: 10px; color: var(--text-muted); }
	.transition-slider { flex: 1; accent-color: var(--accent); cursor: pointer; }
	.transition-value { font-size: 10px; color: var(--text-muted); width: 28px; text-align: right; }

	/* ── Buttons ── */
	.btn-primary {
		background: linear-gradient(135deg, var(--accent), var(--violet));
		color: #fff; border: none; border-radius: 8px;
		padding: 0.65rem 2.2rem; font-size: 1rem; font-weight: 700;
		cursor: pointer; letter-spacing: 0.05em;
		box-shadow: 0 0 24px rgba(255,45,120,0.5), 0 0 48px rgba(180,79,255,0.2);
		transition: all 0.2s;
	}

	.btn-primary:hover {
		box-shadow: 0 0 32px rgba(255,45,120,0.7), 0 0 64px rgba(180,79,255,0.3);
		transform: translateY(-1px);
	}

	.btn-secondary {
		background: #1a1a3a; color: #aaaacc; border: 1px solid #2a2a5a;
		border-radius: 6px; padding: 0.4rem 1rem; cursor: pointer;
		transition: all 0.1s;
	}

	.btn-secondary:hover { border-color: var(--accent); color: #fff; }

	.btn-sm {
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: border-color var(--t-fast), color var(--t-fast);
	}

	.btn-sm:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }

	.btn-sm.active {
		background: var(--accent-dim); border-color: var(--accent); color: var(--accent);
		box-shadow: 0 0 8px var(--accent-dim);
	}

	.btn-sm:disabled { opacity: 0.3; cursor: not-allowed; }

	/* Visualizer drag-over */
	.visualizer-wrap.drag-over::after {
		content: '';
		position: absolute;
		inset: 0;
		border: 2px dashed var(--violet);
		border-radius: 6px;
		pointer-events: none;
		z-index: 20;
	}

</style>
