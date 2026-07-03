<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { AudioEngine } from '$lib/engine/audio.js';
	import { MainSync, type ColorParams, DEFAULT_COLOR_PARAMS, colorParamsToFilter, type SlotComposite, DEFAULT_SLOT_COMPOSITE } from '$lib/engine/sync.js';
	import { Compositor, migrateBlendModeString } from '$lib/engine/compositor.js';
	import { PlaylistEngine, type PlaylistMode } from '$lib/engine/playlist.js';
	import { initPresets, buildPresetList, loadPresetData, type PresetMeta } from '$lib/presets/index.js';
	import PresetBrowser from '$lib/components/PresetBrowser.svelte';
	import { MidiEngine, triggerKey, formatTrigger, type MidiTriggerKey } from '$lib/engine/midi.js';
	import { createDefaultRegistry, type CommandId, type CommandContext } from '$lib/engine/commands.js';
	import { loadKeymap, saveKeymap, resetKeymap, formatKey, DEFAULT_KEYMAP, type KeyBinding } from '$lib/engine/keymap.js';
	import { Clock } from '$lib/engine/clock.js';
	import { LfoEngine, defaultSlot } from '$lib/engine/lfo.js';
	import { BeatDetector } from '$lib/engine/bpm.js';
	import { getQualitySettings, DEFAULT_TIER, DEFAULT_PERF, type QualityTier, type InvisibleMode } from '$lib/engine/quality.js';
	import { makeOverlay, saveAsset, deleteAsset, type Overlay } from '$lib/engine/overlay.js';
	import OverlayLayer from '$lib/components/OverlayLayer.svelte';
	import VideoLayer from '$lib/components/VideoLayer.svelte';
	import SidebarAudio from '$lib/components/SidebarAudio.svelte';
	import SidebarPlaylist from '$lib/components/SidebarPlaylist.svelte';
	import SidebarOverlays from '$lib/components/SidebarOverlays.svelte';
	import SidebarVideo from '$lib/components/SidebarVideo.svelte';
	import DeckCard from '$lib/components/DeckCard.svelte';
	import LayoutToggle from '$lib/components/LayoutToggle.svelte';
	import MixerLayout from '$lib/components/MixerLayout.svelte';
	import { DeckManager } from '$lib/engine/deck-manager.js';
	import { initVideoLoops, builtinClips } from '$lib/video-loops/index.js';
	import { saveVideo, deleteVideo, type ClipRef, type VideoClipMeta } from '$lib/engine/video-store.js';

	// — State —————————————————————————————————————————————
	let canvases = $state<(HTMLCanvasElement | undefined)[]>([undefined, undefined, undefined, undefined]);
	let compositorCanvas: HTMLCanvasElement | undefined = $state();
	let compositor: Compositor | null = null;
	const manager = new DeckManager();
	let audio: AudioEngine | null = null;

	let presetList: PresetMeta[] = $state([]);

	let activeSlot = $state(0); // 0=A 1=B 2=C 3=D — cible du preset browser
	let presetA = $state('');
	let presetB = $state('');
	let crossfader = $state(0); // 0 = 100% A, 1 = 100% B
	let transitionTime = $state(2.0); // secondes de fondu preset (0 = hard cut)

	let sourceLabel = $state('none');
	let currentDeviceId = $state('');
	let status = $state<'idle' | 'running' | 'error'>('idle');
	let errorMsg = $state('');
	let sourceError = $state('');
	let audioEl: HTMLAudioElement | undefined = $state();
	let audioDevices = $state<MediaDeviceInfo[]>([]);
	let outputDevices = $state<Array<{id: number; name: string; maxInputChannels: number; maxOutputChannels: number; defaultSampleRate: number}>>([]);
	let showDevicePicker = $state(false);
	let loopbackUnlisten: (() => void) | null = null;
	let currentLoopbackDeviceId = $state(0);
	let vuLevel = $state(0);
	let outputOpen = $state(false);
	let outputWinRef: Window | null = null;
	let outputCloseTimer: ReturnType<typeof setInterval> | null = null;
	let sync: MainSync | null = null;
	// Screen targeting (Electron)
	type DisplayInfo = { id: number; label: string; isPrimary: boolean; bounds: { x: number; y: number; width: number; height: number } };
	let displays = $state<DisplayInfo[]>([]);
	let selectedDisplayId = $state<number | null>(null);
	let outputWindowClosedUnlisten: (() => void) | null = null;

	// — Playlist state ————————————————————————————————————
	let playlistIntervalSec = $state(10);
	let playlistMode = $state<PlaylistMode>('sequential');
	let playlistAPlaying = $state(false);
	let playlistBPlaying = $state(false);
	let playlistA: PlaylistEngine | null = null;
	let playlistB: PlaylistEngine | null = null;
	let playlistAItems = $state<string[]>([]);
	let playlistBItems = $state<string[]>([]);

	// — MIDI ——————————————————————————————————————————————
	const registry = createDefaultRegistry();

	const midiSupported = typeof navigator !== 'undefined' && 'requestMIDIAccess' in navigator;
	let midi: MidiEngine | null = null;
	let midiConnected = $state(false);
	let midiDeviceNames = $state<string[]>([]);
	let midiMappings = $state<Partial<Record<CommandId, MidiTriggerKey>>>({});
	let learningAction = $state<CommandId | null>(null);
	let keymap = $state<KeyBinding>({ ...DEFAULT_KEYMAP });
	let learningKey = $state<CommandId | null>(null);

	// — Clock + LFO + Strobe ———————————————————————————————
	const clock = new Clock();
	const lfoEngine = new LfoEngine();
	const lfoSlots = $state(lfoEngine.slots);
	let midiClockBpm = $state(0);   // BPM détecté via MIDI clock IN (0 = inactif)
	let strobeOn = $state(false);
	/** Strobe rate: beats per flash cycle. 0.25=1/4beat, 0.5=half, 1=beat, 2=half-tempo, 4=quarter-tempo */
	let strobeRate = $state(1);
	let strobeIntensity = $state(0.8);
	let strobeColor = $state('#ffffff');
	let strobeFlash = $state(false);
	let _lastStrobeVal = 0;

	// — Color controls per deck (M3) ——————————————————————
	let colorParamsA = $state<ColorParams>({ ...DEFAULT_COLOR_PARAMS });
	let colorParamsB = $state<ColorParams>({ ...DEFAULT_COLOR_PARAMS });

	type ColorCmd = [sfx: string, field: keyof ColorParams, lbl: string];
	const COLOR_CMDS: ColorCmd[] = [
		['hue','hueRotate','Hue'],['sat','saturate','Saturation'],
		['bright','brightness','Brightness'],['contrast','contrast','Contrast'],['invert','invert','Invert'],
	];
	const colorFilterA = $derived(colorParamsToFilter(colorParamsA));
	const colorFilterB = $derived(colorParamsToFilter(colorParamsB));

	// Inverted index: commandId → assigned key string
	const keyById = $derived(
		new Map<CommandId, string>(
			(Object.entries(keymap) as [string, CommandId][]).map(([k, v]) => [v, k])
		)
	);

	// — Electron ——————————————————————————————————————————
	const isElectron = typeof window !== 'undefined' && !!window.electronAPI?.isElectron;
	let platform = $state('');
	let showSystemAudioHelp = $state(false);
	let showPresetBrowser = $state(false);
	let showStreamPanel = $state(false);
	let ndiActive = $state(false);
	let ndiError = $state('');
	// OSC
	let oscActive = $state(false);
	let oscPort = $state(7000);
	let oscError = $state('');
	let oscUnlisten: (() => void) | null = null;
	// Remote control WS
	let remoteActive = $state(false);
	let remoteUrl = $state('');
	let remoteError = $state('');
	let remoteUnlisten: (() => void) | null = null;
	// Ableton Link
	let linkActive = $state(false);
	let linkPeers = $state(0);
	let linkError = $state('');
	let linkUnlisten: (() => void) | null = null;
	let v4l2Active = $state(false);
	let v4l2Error = $state('');
	let spoutActive = $state(false);
	let spoutError = $state('');
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

	// — Video loops ———————————————————————————————————————
	let videoEnabled = $state(false);
	let videoOpacity = $state(0.6);
	let videoAdvance = $state<'shuffle' | 'sequential' | 'manual'>('shuffle');
	let videoBeatsPerCut = $state(8);
	let vrCut = $state(true);
	let vrFlash = $state(true);
	let vrWarp = $state(true);
	let vrHue = $state(false);
	let userClips = $state<VideoClipMeta[]>([]);
	let currentClipIndex = $state(0);
	let videoPlaybackRate = $state(1);
	let videoBeatCount = 0;

	// — Beat detection ————————————————————————————————————
	let beatDetector: BeatDetector | null = null;
	let detectedBpm = $state(0);
	let beatSyncA = $state(false);
	let beatSyncB = $state(false);
	let beatsPerChange = $state(8);
	let beatCountA = 0;
	let beatCountB = 0;
	let autoXfade = $state(false);
	let autoXfadeCount = 0;

	// — Tap tempo ——————————————————————————————————————————
	let tapTimes: number[] = [];
	let manualBpm = $state(0);

	// — Lock deck ——————————————————————————————————————————
	let lockA = $state(false);
	let lockB = $state(false);

	// — Qualité rendu ——————————————————————————————————————
	let quality = $state<QualityTier>(DEFAULT_TIER);
	let fps = $state(0);

	// — Blend mode decks ——————————————————————————————————
	let deckBlendMode = $state('screen');
	// Compositing par slot (blend + lumaKey + colorKey) — remplace deckBlendMode (1.1)
	let slotComposites = $state<[SlotComposite, SlotComposite, SlotComposite, SlotComposite]>([
		{ ...DEFAULT_SLOT_COMPOSITE },
		{ ...DEFAULT_SLOT_COMPOSITE },
		{ ...DEFAULT_SLOT_COMPOSITE },
		{ ...DEFAULT_SLOT_COMPOSITE },
	]);

	// — Performance decks ——————————————————————————————————
	let targetFps = $state(DEFAULT_PERF.targetFps);
	let invisibleMode = $state<InvisibleMode>(DEFAULT_PERF.invisibleMode);
	let invisibleFps = $state(DEFAULT_PERF.invisibleFps);
	let pausedSlots = new Set<number>();   // non-réactif: mémoire inter-runs du $effect éco
	let lastFps = [0, 0, 0, 0];            // non-réactif: anti-churn inter-runs du $effect éco

	// — Overlays ——————————————————————————————————————————
	let overlays = $state<Overlay[]>([]);
	let beat = $state(false);
	let overlayDragOver = $state(false);
	let deckBus = $state<Array<'A' | 'B' | 'off'>>(['A', 'B', 'off', 'off']);
	const activeDeck = $derived<'A' | 'B'>(deckBus[activeSlot] === 'B' ? 'B' : 'A');
	let activePreset = $derived(activeDeck === 'A' ? presetA : presetB);
	let _slotEpoch = $state(0);
	let preset2 = $state('');
	let preset3 = $state('');
	const presets4 = $derived([presetA, presetB, preset2, preset3]);

	function busGain(bus: 'A' | 'B' | 'off', x: number): number {
		if (bus === 'A') return 1 - x;
		if (bus === 'B') return x;
		return 0;
	}
	const opacities = $derived(deckBus.map((bus) => busGain(bus, crossfader)));
	const opacityA = $derived(opacities[0]);
	const opacityB = $derived(opacities[1]);
	let presetIdxA = $derived(presetList.findIndex((p) => p.name === presetA));
	let presetIdxB = $derived(presetList.findIndex((p) => p.name === presetB));
	const allClips = $derived([...builtinClips, ...userClips]);

	/** Returns the preset of the first running deck on a given bus, or the last known preset if none running. */
	function primaryPreset(bus: 'A' | 'B'): string {
		void _slotEpoch; // force reactive tracking
		for (let i = 0; i < 4; i++) {
			if (deckBus[i] === bus && isRunning(i)) return presets4[i];
		}
		return bus === 'A' ? presetA : presetB;
	}

	const busPresetA = $derived(primaryPreset('A'));
	const busPresetB = $derived(primaryPreset('B'));
	const runningCount = $derived([0, 1, 2, 3].filter(i => manager.isRunning(i)).length);

	const currentClip = $derived<ClipRef | null>(
		videoEnabled && allClips.length > 0 ? allClips[currentClipIndex % allClips.length].ref : null
	);
	// Rounded to 1/20 steps so the sync $effect doesn't fire at 60fps
	const videoPlaybackRateStep = $derived(Math.round(videoPlaybackRate * 20) / 20);

	// — Câblage commandes M2 (strobe/LFO) dans le registre ——
	registry.register({
		id: 'strobe-toggle', label: 'Strobe ON/OFF', kind: 'trigger',
		run() { strobeOn = !strobeOn; },
	});
	registry.register({
		id: 'lfo-rate-up', label: 'LFO Rate +', kind: 'trigger',
		run() { strobeRate = Math.min(4, strobeRate * 2); },
	});
	registry.register({
		id: 'lfo-rate-down', label: 'LFO Rate −', kind: 'trigger',
		run() { strobeRate = Math.max(0.25, strobeRate / 2); },
	});

	// — Câblage commandes M3 (color controls) ——————————————
	for (const [sfx, field, lbl] of COLOR_CMDS)
		for (const deck of ['a', 'b'] as const)
			registry.register({ id: `color-${sfx}-${deck}` as CommandId, label: `${lbl} ${deck.toUpperCase()}`, kind: 'range',
				run(v) { if (deck === 'a') colorParamsA = {...colorParamsA, [field]: v}; else colorParamsB = {...colorParamsB, [field]: v}; },
			});

	// — Command context (injected into registry.dispatch) ——
	const commandCtx: CommandContext = {
		getCrossfader: () => crossfader,
		setCrossfader: (v) => { crossfader = v; },
		getActiveDeck: () => activeDeck,
		switchActiveDeck: () => { activeSlot = activeSlot === 0 ? 1 : 0; },
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
	};

	// — Sync crossfader to output window ——————————————————
	// Read crossfader unconditionally before sync?. so Svelte 5 tracks it as a
	// dependency even when sync is still null on the first $effect run (onMount
	// is async → sync is assigned late). Without this, ?. short-circuits the
	// argument evaluation → crossfader is never tracked → effect never re-runs.
	$effect(() => {
		const x = crossfader;
		sync?.sendCrossfader(x);
	});

	// — VU meter polling + FPS counter + video speed warp —
	$effect(() => {
		if (status !== 'running' || !audio) return;
		let rafId: number;
		let fpsLast = performance.now();
		let lastRenderCount = 0;
		const tick = (t: number) => {
			// VU meter + video warp
			const lv = audio!.getLevels();
			vuLevel = lv.rms;
			if (videoEnabled && vrWarp) {
				const target = 0.6 + lv.bass * 1.4;
				videoPlaybackRate += (target - videoPlaybackRate) * 0.15;
			} else {
				videoPlaybackRate = 1;
			}
			// FPS counter — mesure les renders Butterchurn réels (pas les ticks RAF)
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
	// _ready évite que les $effect écrasent le localStorage avant qu'onMount l'ait lu
	let _ready = $state(false);
	$effect(() => {
		if (!_ready) return;
		localStorage.setItem('od-pl-a', JSON.stringify(playlistAItems));
		localStorage.setItem('od-pl-b', JSON.stringify(playlistBItems));
		localStorage.setItem('od-pl-interval', String(playlistIntervalSec));
		localStorage.setItem('od-pl-mode', playlistMode);
		localStorage.setItem('od-midi-mappings', JSON.stringify(midiMappings));
		localStorage.setItem('od-keymap', JSON.stringify(keymap));
		localStorage.setItem('od-quality', quality);
		localStorage.setItem('od-target-fps', String(targetFps));
		localStorage.setItem('od-invisible-mode', invisibleMode);
		localStorage.setItem('od-overlays', JSON.stringify(overlays));
		localStorage.setItem('od-deck-bus', JSON.stringify(deckBus));
		localStorage.setItem('od-layout', layout);
		localStorage.setItem('od-transition', String(transitionTime));
		localStorage.setItem('od-blendmode', deckBlendMode);
		localStorage.setItem('od-composite', JSON.stringify(slotComposites));
	});

	// — Persistance localStorage vidéo ———————————————————
	$effect(() => {
		if (!_ready) return;
		localStorage.setItem('od-video-enabled', String(videoEnabled));
		localStorage.setItem('od-video-opacity', String(videoOpacity));
		localStorage.setItem('od-video-advance', videoAdvance);
		localStorage.setItem('od-video-beats', String(videoBeatsPerCut));
		localStorage.setItem('od-video-reactions', JSON.stringify({ cut: vrCut, flash: vrFlash, warp: vrWarp, hue: vrHue }));
		localStorage.setItem('od-video-userclips', JSON.stringify(userClips));
	});

	// — Sync overlays vers output ——————————————————————————
	$effect(() => {
		const list = overlays; // force tracking (same pattern as crossfader above)
		sync?.sendOverlays(list);
	});

	// — Sync vidéo vers output ————————————————————————————
	$effect(() => {
		const payload = { // force tracking of all fields before sync?.
			enabled: videoEnabled,
			clip: currentClip,
			opacity: videoOpacity,
			playbackRate: videoPlaybackRateStep,
			flashOn: vrFlash,
			hueOn: vrHue,
		};
		sync?.sendVideo(payload);
	});

	// — Sync blend mode vers output ————————————————————————
	$effect(() => {
		const mode = deckBlendMode;
		sync?.sendBlendMode(mode);
	});

	// — Sync compositing (blend + lumaKey + colorKey) vers output, par slot —
	$effect(() => {
		const composites = slotComposites;
		if (!sync) return;
		for (let i = 0; i < 4; i++) sync.sendComposite(i, composites[i]);
	});

	// Pousse opacité + config de compositing vers le Compositor local (Stage).
	$effect(() => {
		const ops = opacities;
		const composites = slotComposites;
		if (!compositor) return;
		for (let i = 0; i < 4; i++) compositor.setLayer(i, ops[i], composites[i]);
	});

	// — Sync strobe vers output ———————————————————————————
	$effect(() => {
		const on = strobeOn;
		const rate = strobeRate;
		const intensity = strobeIntensity;
		const color = strobeColor;
		sync?.sendStrobe(on, rate, intensity, color);
	});

	// — Sync color params vers output ————————————————————
	$effect(() => {
		const paramsA = colorParamsA;
		sync?.sendColor('A', paramsA);
	});
	$effect(() => {
		const paramsB = colorParamsB;
		sync?.sendColor('B', paramsB);
	});

	// — Appliquer la qualité aux decks + sync output ———————
	$effect(() => {
		if (status !== 'running') return;
		const settings = getQualitySettings(quality);
		manager.applyQuality(settings);
		sync?.sendQuality(quality);
	});

	// — Appliquer le FPS cible aux decks + sync output ————
	$effect(() => {
		if (status !== 'running') return;
		const fps = targetFps;    // lire avant sync?. pour forcer le tracking
		const mode = invisibleMode;
		const eco = invisibleFps;
		manager.setTargetFps(fps);
		sync?.sendPerf({ targetFps: fps, invisibleMode: mode, invisibleFps: eco });
	});

	// — Throttle des decks invisibles (éco) ——————————————
	$effect(() => {
		if (status !== 'running') return;
		const ops = opacities;          // lire en premier → tracké
		const mode = invisibleMode;
		const target = targetFps;
		const eco = invisibleFps;
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
		if (isElectron) {
			platform = await window.electronAPI!.getPlatform();
		}
		// Restaurer les playlists sauvegardées
		try {
			const savedA = localStorage.getItem('od-pl-a');
			if (savedA) playlistAItems = JSON.parse(savedA);
			const savedB = localStorage.getItem('od-pl-b');
			if (savedB) playlistBItems = JSON.parse(savedB);
			const savedInterval = localStorage.getItem('od-pl-interval');
			if (savedInterval) playlistIntervalSec = Number(savedInterval);
			const savedMode = localStorage.getItem('od-pl-mode');
			if (savedMode) playlistMode = savedMode as PlaylistMode;
			const savedMidi = localStorage.getItem('od-midi-mappings');
			if (savedMidi) midiMappings = JSON.parse(savedMidi);
			const savedKeymap = localStorage.getItem('od-keymap');
			if (savedKeymap) try { keymap = { ...DEFAULT_KEYMAP, ...JSON.parse(savedKeymap) }; } catch {}
			const savedQuality = localStorage.getItem('od-quality');
			if (savedQuality === 'low' || savedQuality === 'medium' || savedQuality === 'high') quality = savedQuality;
			const savedTransition = localStorage.getItem('od-transition');
			if (savedTransition) transitionTime = Number(savedTransition);
			const savedBlendMode = localStorage.getItem('od-blendmode');
			if (savedBlendMode) deckBlendMode = savedBlendMode;
			const savedComposite = localStorage.getItem('od-composite');
			if (savedComposite) {
				try {
					const parsed = JSON.parse(savedComposite);
					if (Array.isArray(parsed) && parsed.length === 4) {
						slotComposites = parsed.map((c) => ({ ...DEFAULT_SLOT_COMPOSITE, ...c })) as typeof slotComposites;
					}
				} catch { /* ignore corrupt od-composite */ }
			} else if (savedBlendMode) {
				// Migration one-shot depuis l'ancien mode global CSS.
				const migrated = migrateBlendModeString(savedBlendMode);
				slotComposites = slotComposites.map((c) => ({ ...c, blend: migrated })) as typeof slotComposites;
			}
			const savedFps = localStorage.getItem('od-target-fps');
			if (savedFps) {
				const v = Number(savedFps);
				if (v === 30 || v === 45 || v === 60) targetFps = v;
			}
			const savedInvisibleMode = localStorage.getItem('od-invisible-mode');
			if (savedInvisibleMode === 'eco' || savedInvisibleMode === 'pause' || savedInvisibleMode === 'off') {
				invisibleMode = savedInvisibleMode;
			}
			const savedOverlays = localStorage.getItem('od-overlays');
			if (savedOverlays) overlays = JSON.parse(savedOverlays);
			const savedVideoEnabled = localStorage.getItem('od-video-enabled');
			if (savedVideoEnabled) videoEnabled = savedVideoEnabled === 'true';
			const savedVideoOpacity = localStorage.getItem('od-video-opacity');
			if (savedVideoOpacity) videoOpacity = Number(savedVideoOpacity);
			const savedVideoAdvance = localStorage.getItem('od-video-advance');
			if (savedVideoAdvance === 'shuffle' || savedVideoAdvance === 'sequential' || savedVideoAdvance === 'manual') videoAdvance = savedVideoAdvance;
			const savedVideoBeats = localStorage.getItem('od-video-beats');
			if (savedVideoBeats) videoBeatsPerCut = Number(savedVideoBeats);
			const savedVideoReactions = localStorage.getItem('od-video-reactions');
			if (savedVideoReactions) { try { const r = JSON.parse(savedVideoReactions); vrCut = !!r.cut; vrFlash = !!r.flash; vrWarp = !!r.warp; vrHue = !!r.hue; } catch {} }
			const savedVideoClips = localStorage.getItem('od-video-userclips');
			if (savedVideoClips) { try { userClips = JSON.parse(savedVideoClips); } catch {} }
			const savedDeckBus = localStorage.getItem('od-deck-bus');
			if (savedDeckBus) {
				try { deckBus = JSON.parse(savedDeckBus); } catch {}
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
				// phase de Link est sur quantum=4 → normaliser en 0..1
				clock.syncExternal(state.tempo, state.phase / 4.0);
				linkPeers = state.peers;
			}) ?? null;
			outputWindowClosedUnlisten = window.electronAPI?.onOutputWindowClosed?.(() => {
				outputOpen = false;
				audio?.stopPcmCapture();
			}) ?? null;
			// Charger la liste des écrans
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
		if (presetList.length > 0) presetA = presetList[0].name;
		if (presetList.length > 1) presetB = presetList[1].name;
	});

	onDestroy(() => {
		_stopLoopbackIpc();
		if (outputCloseTimer !== null) clearInterval(outputCloseTimer);
		playlistA?.destroy();
		playlistB?.destroy();
		manager.destroyAll();
		compositor?.destroy();
		audio?.destroy(); // also calls stopPcmCapture()
		sync?.destroy();
		midi?.destroy();
		beatDetector?.destroy();
		clock.stop();
		oscUnlisten?.();
		remoteUnlisten?.();
		linkUnlisten?.();
		outputWindowClosedUnlisten?.();
		if (oscActive) window.electronAPI?.stopOsc?.();
		if (remoteActive) window.electronAPI?.stopRemote?.();
		if (linkActive) window.electronAPI?.stopLink?.();
	});

	// — Actions ————————————————————————————————————————————
	async function startVisualizer() {
		if (!canvases[0] || !canvases[1]) return;
		try {
			const testCtx = canvases[0].getContext('webgl2');
			if (!testCtx) {
				throw new Error(
					'WebGL2 unavailable. In LibreWolf/Firefox: go to about:config → set webgl.disabled = false.'
				);
			}

			audio = new AudioEngine();
			await audio.resume();

			// Attacher les 4 canvases au manager (slots 2-3 peuvent être undefined)
			for (let i = 0; i < 4; i++) {
				const c = canvases[i];
				if (c) manager.attachCanvas(i, c);
			}

			const q = getQualitySettings(quality);

			compositor = new Compositor(compositorCanvas!);
			for (let i = 0; i < 4; i++) {
				const c = canvases[i];
				if (c) compositor.attachSource(i, c);
			}
			compositor.resize(compositorCanvas!.clientWidth || window.innerWidth, compositorCanvas!.clientHeight || window.innerHeight, q.pixelRatio);
			// Poussée initiale explicite — le $effect ne se redéclenche pas tant
			// qu'aucun $state qu'il lit ne change (compositor n'en est pas un).
			for (let i = 0; i < 4; i++) compositor.setLayer(i, opacities[i], slotComposites[i]);
			compositor.start();

			const d0 = presetA ? await loadPresetData(presetA) : null;
			const d1 = presetB ? await loadPresetData(presetB) : null;
			await manager.start(0, audio.ctx, audio.gainNode, q, d0);
			await manager.start(1, audio.ctx, audio.gainNode, q, d1);

			playlistA = new PlaylistEngine(playlistAItems, playlistMode, playlistIntervalSec * 1000, async (name) => {
				presetA = name;
				const d = await loadPresetData(name); if (d) manager.loadPreset(0, d, transitionTime);
				sync?.sendPreset('A', name, transitionTime);
				playlistAPlaying = playlistA?.playing ?? false;
			});
			playlistB = new PlaylistEngine(playlistBItems, playlistMode, playlistIntervalSec * 1000, async (name) => {
				presetB = name;
				const d = await loadPresetData(name); if (d) manager.loadPreset(1, d, transitionTime);
				sync?.sendPreset('B', name, transitionTime);
				playlistBPlaying = playlistB?.playing ?? false;
			});

			sync = new MainSync();
			sync.onOutputReady(async () => {
				sync?.sendPreset('A', busPresetA);
				sync?.sendPreset('B', busPresetB);
				sync?.sendCrossfader(crossfader);
				sync?.sendQuality(quality);
				sync?.sendBlendMode(deckBlendMode);
				for (let i = 0; i < 4; i++) sync?.sendComposite(i, slotComposites[i]);
				sync?.sendPerf({ targetFps, invisibleMode, invisibleFps });
				sync?.sendOverlays(overlays);
				sync?.sendVideo({ enabled: videoEnabled, clip: currentClip, opacity: videoOpacity, playbackRate: videoPlaybackRateStep, flashOn: vrFlash, hueOn: vrHue });
				if (currentDeviceId) sync?.sendSource(currentDeviceId);
				if (currentLoopbackDeviceId) sync?.sendLoopback(currentLoopbackDeviceId);
				// Stream live PCM to the output window so it becomes audio-reactive
				// regardless of source (device / mic / file). Electron-only: the output
				// window cannot re-capture the same device independently (fragile on Linux).
				// await + catch so a transient worklet failure is visible in the console
				// and retried on the next hello (startPcmCapture is idempotent).
				if (isElectron) {
					try {
						await audio?.startPcmCapture((f) => window.electronAPI!.sendAudioFrame(f));
					} catch (e) {
						console.error('[output] startPcmCapture failed', e);
					}
				}
			});

			beatDetector = new BeatDetector(audio.analyser);
			beatDetector.start(() => {
				detectedBpm = beatDetector?.bpm ?? 0;
				if (!manualBpm) clock.pulse(detectedBpm);
			});
			clock.onBeat(onBeat);
			clock.onTick((phase) => {
				// Route LFO values to registry commands
				for (const { target, value01 } of lfoEngine.tick(phase)) {
					if (target) registry.dispatch(target, value01, commandCtx);
				}
				// Strobe: detect rising edge of a square LFO at strobeRate
				if (strobeOn) {
					const p = (phase * strobeRate) % 1;
					const val = p < 0.5 ? 1 : 0;
					if (val === 1 && _lastStrobeVal === 0) {
						strobeFlash = true;
						setTimeout(() => { strobeFlash = false; }, 50);
					}
					_lastStrobeVal = val;
				}
			});
			clock.start();

			status = 'running';
		} catch (e) {
			status = 'error';
			errorMsg = e instanceof Error ? e.message : String(e);
		}
	}

	async function captureSystemAudio() {
		if (!audio) return;
		sourceError = '';
		_stopLoopbackIpc();
		try {
			await audio.resume();
			if (isElectron && platform === 'win32') {
				// Electron Windows: setDisplayMediaRequestHandler → loopback natif, pas de picker
				await audio.connectDisplay();
				sourceLabel = 'system audio';
			} else if (effectiveOS === 'linux' || effectiveOS === 'darwin') {
				// Linux (Electron ou web) / macOS (Electron) : chercher .monitor ou BlackHole
				const devices = await AudioEngine.listAudioDevices();
				const monitors = devices.filter((d) =>
					/monitor|blackhole|loopback|cable|opendrop/i.test(d.label)
				);
				if (monitors.length === 1) {
					await audio.connectDevice(monitors[0].deviceId);
					currentDeviceId = monitors[0].deviceId;
					sourceLabel = monitors[0].label || 'system audio';
					sync?.sendSource(monitors[0].deviceId);
				} else if (monitors.length > 1) {
					audioDevices = monitors;
					showDevicePicker = true;
				} else {
					showSystemAudioHelp = true;
				}
			} else {
				// Web Windows / navigateur inconnu : getDisplayMedia avec guidance honnête
				await audio.connectDisplay();
				sourceLabel = 'system audio';
			}
		} catch (e) {
			sourceError = e instanceof Error ? e.message : String(e);
		}
	}

	async function connectMic() {
		if (!audio) return;
		sourceError = '';
		_stopLoopbackIpc();
		try {
			await audio.resume();
			await audio.connectMic();
			sourceLabel = 'microphone';
		} catch (e) {
			sourceError = e instanceof Error ? e.message : String(e);
		}
	}

	async function openDevicePicker() {
		sourceError = '';
		try {
			audioDevices = await AudioEngine.listAudioDevices();
			if (loopbackSupported) {
				const res = await window.electronAPI!.listOutputDevices();
				outputDevices = res.ok ? res.devices : [];
			} else {
				outputDevices = [];
			}
			showDevicePicker = true;
		} catch (e) {
			sourceError = e instanceof Error ? e.message : String(e);
		}
	}

	function _stopLoopbackIpc() {
		loopbackUnlisten?.();
		loopbackUnlisten = null;
		currentLoopbackDeviceId = 0;
		window.electronAPI?.stopLoopback();
	}

	async function connectDevice(device: MediaDeviceInfo) {
		if (!audio) return;
		sourceError = '';
		showDevicePicker = false;
		_stopLoopbackIpc();
		try {
			await audio.resume();
			await audio.connectDevice(device.deviceId);
			currentDeviceId = device.deviceId;
			sourceLabel = device.label || device.deviceId;
			sync?.sendSource(device.deviceId);
		} catch (e) {
			sourceError = e instanceof Error ? e.message : String(e);
		}
	}

	async function connectLoopback(device: {id: number; name: string; maxInputChannels: number; maxOutputChannels: number; defaultSampleRate: number}) {
		if (!audio) return;
		sourceError = '';
		showDevicePicker = false;
		_stopLoopbackIpc();
		try {
			await audio.resume();
			await audio.connectLoopbackPcm();
			manager.connectAudio(audio.gainNode);
			loopbackUnlisten = window.electronAPI!.onLoopbackData((data) => {
				audio?.pushLoopbackPcm(data);
			});
			const res = await window.electronAPI!.startLoopback(device.id);
			if (!res.ok) throw new Error(res.error ?? 'loopback start failed');
			currentLoopbackDeviceId = device.id;
			currentDeviceId = '';
			sourceLabel = device.name;
			sync?.sendLoopback(device.id);
		} catch (e) {
			sourceError = e instanceof Error ? e.message : String(e);
		}
	}

	async function connectFile() {
		if (!audio || !audioEl) return;
		sourceError = '';
		try {
			await audio.resume();
			audio.connectMediaElement(audioEl);
			audioEl.play();
			sourceLabel = 'file';
		} catch (e) {
			sourceError = e instanceof Error ? e.message : String(e);
		}
	}

	function onFileChange(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0];
		if (!file || !audioEl) return;
		audioEl.src = URL.createObjectURL(file);
		if (status === 'running') connectFile();
	}

	async function selectPreset(name: string) {
		const d = await loadPresetData(name);
		if (!d) return;
		const slot = activeSlot;
		if (slot === 0) presetA = name;
		else if (slot === 1) presetB = name;
		else if (slot === 2) preset2 = name;
		else preset3 = name;
		manager.loadPreset(slot, d, transitionTime);
		_slotEpoch++;
		const bus = deckBus[slot];
		if (bus === 'A') sync?.sendPreset('A', primaryPreset('A'), transitionTime);
		else if (bus === 'B') sync?.sendPreset('B', primaryPreset('B'), transitionTime);
	}

	function addToPlaylist(deck: 'A' | 'B', name: string) {
		if (deck === 'A') {
			if (playlistAItems.includes(name)) return;
			playlistAItems = [...playlistAItems, name];
			playlistA?.setItems(playlistAItems);
		} else {
			if (playlistBItems.includes(name)) return;
			playlistBItems = [...playlistBItems, name];
			playlistB?.setItems(playlistBItems);
		}
	}

	function removeFromPlaylist(deck: 'A' | 'B', name: string) {
		if (deck === 'A') {
			playlistAItems = playlistAItems.filter((n) => n !== name);
			playlistA?.setItems(playlistAItems);
		} else {
			playlistBItems = playlistBItems.filter((n) => n !== name);
			playlistB?.setItems(playlistBItems);
		}
	}

	function togglePlaylist(deck: 'A' | 'B') {
		const pl = deck === 'A' ? playlistA : playlistB;
		if (!pl) return;
		pl.setInterval(playlistIntervalSec * 1000);
		pl.setMode(playlistMode);
		if (pl.playing) {
			pl.stop();
		} else {
			pl.start();
		}
		if (deck === 'A') playlistAPlaying = pl.playing;
		else playlistBPlaying = pl.playing;
	}

	function playlistNext(deck: 'A' | 'B') {
		(deck === 'A' ? playlistA : playlistB)?.next();
	}

	function playlistPrev(deck: 'A' | 'B') {
		(deck === 'A' ? playlistA : playlistB)?.prev();
	}

	function onBeat() {
		// Pulse overlay beat-reactive
		beat = true;
		setTimeout(() => { beat = false; }, 80);
		sync?.sendBeat(clock.bpm || detectedBpm);

		if (videoEnabled && vrCut && videoAdvance !== 'manual' && allClips.length > 1) {
			videoBeatCount = (videoBeatCount + 1) % videoBeatsPerCut;
			if (videoBeatCount === 0) {
				currentClipIndex = videoAdvance === 'shuffle'
					? Math.floor(Math.random() * allClips.length)
					: (currentClipIndex + 1) % allClips.length;
			}
		}

		if (autoXfade) {
			autoXfadeCount = (autoXfadeCount + 1) % beatsPerChange;
			if (autoXfadeCount === 0) {
				crossfader = crossfader < 0.5 ? 1 : 0;
				sync?.sendCrossfader(crossfader);
			}
		}
		if (beatSyncA && !lockA) {
			beatCountA = (beatCountA + 1) % beatsPerChange;
			if (beatCountA === 0) {
				if (playlistAItems.length > 0) playlistA?.next();
				else applyMidiAction('preset-next-a', 127);
			}
		}
		if (beatSyncB && !lockB) {
			beatCountB = (beatCountB + 1) % beatsPerChange;
			if (beatCountB === 0) {
				if (playlistBItems.length > 0) playlistB?.next();
				else applyMidiAction('preset-next-b', 127);
			}
		}
	}

	// — Overlay helpers ————————————————————————————————————
	async function addOverlayFromFile(file: File) {
		return new Promise<void>((resolve) => {
			const reader = new FileReader();
			reader.onload = async () => {
				const dataUrl = reader.result as string;
				const ov = makeOverlay(file.name.replace(/\.[^.]+$/, ''), { video: file.type.startsWith('video/') });
				await saveAsset(ov.id, dataUrl);
				overlays = [...overlays, ov];
				resolve();
			};
			reader.readAsDataURL(file);
		});
	}

	async function onOverlayFilePick(e: Event) {
		const files = (e.target as HTMLInputElement).files;
		if (!files) return;
		for (const f of Array.from(files)) await addOverlayFromFile(f);
		(e.target as HTMLInputElement).value = '';
	}

	function onVisualizerDragOver(e: DragEvent) {
		if (!e.dataTransfer?.types.includes('Files')) return;
		e.preventDefault();
		overlayDragOver = true;
	}

	async function onVisualizerDrop(e: DragEvent) {
		e.preventDefault();
		overlayDragOver = false;
		if (!e.dataTransfer?.files.length) return;
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const x = (e.clientX - rect.left) / rect.width;
		const y = (e.clientY - rect.top) / rect.height;
		for (const f of Array.from(e.dataTransfer.files)) {
			if (f.type.startsWith('video/')) {
				if (f.size > 50 * 1024 * 1024) continue;
				const id = crypto.randomUUID();
				await saveVideo(id, f);
				userClips = [...userClips, { ref: { kind: 'user', id }, name: f.name.replace(/\.[^.]+$/, '') }];
				if (!videoEnabled) videoEnabled = true;
				continue;
			}
			if (!f.type.startsWith('image/')) continue;
			await new Promise<void>((res) => {
				const reader = new FileReader();
				reader.onload = async () => {
					const dataUrl = reader.result as string;
					const ov = makeOverlay(f.name.replace(/\.[^.]+$/, ''), { x, y });
					await saveAsset(ov.id, dataUrl);
					overlays = [...overlays, ov];
					res();
				};
				reader.readAsDataURL(f);
			});
		}
	}

	async function removeOverlay(id: string) {
		await deleteAsset(id);
		overlays = overlays.filter(o => o.id !== id);
	}

	function updateOverlay(id: string, patch: Partial<Overlay>) {
		overlays = overlays.map(o => o.id === id ? { ...o, ...patch } : o);
	}

	async function addVideoFromFile(file: File) {
		if (file.size > 50 * 1024 * 1024) return;
		const id = crypto.randomUUID();
		await saveVideo(id, file);
		userClips = [...userClips, { ref: { kind: 'user', id }, name: file.name.replace(/\.[^.]+$/, '') }];
		if (!videoEnabled) videoEnabled = true;
	}

	async function onVideoFilePick(e: Event) {
		const files = (e.target as HTMLInputElement).files;
		if (!files) return;
		for (const f of Array.from(files)) await addVideoFromFile(f);
		(e.target as HTMLInputElement).value = '';
	}

	async function removeVideoClip(index: number) {
		const clip = userClips[index - builtinClips.length];
		if (clip?.ref.kind === 'user') await deleteVideo(clip.ref.id);
		userClips = userClips.filter((_, i) => i !== index - builtinClips.length);
		if (currentClipIndex >= allClips.length) currentClipIndex = 0;
	}

	function toggleBeatSync(deck: 'A' | 'B') {
		if (deck === 'A') {
			beatSyncA = !beatSyncA;
			beatCountA = 0;
			playlistA?.setInterval(beatSyncA ? Infinity : playlistIntervalSec * 1000);
		} else {
			beatSyncB = !beatSyncB;
			beatCountB = 0;
			playlistB?.setInterval(beatSyncB ? Infinity : playlistIntervalSec * 1000);
		}
	}

	function tapTempo() {
		const now = performance.now();
		tapTimes.push(now);
		if (tapTimes.length > 4) tapTimes = tapTimes.slice(-4);
		if (tapTimes.length < 2) return;
		const intervals = tapTimes.slice(1).map((t, i) => t - tapTimes[i]);
		const avg = intervals.reduce((s, v) => s + v, 0) / intervals.length;
		const bpm = Math.round(60000 / avg);
		if (bpm < 40 || bpm > 300) return;
		manualBpm = bpm;
		clock.setBpm(bpm);
		clock.pulse();
	}

	function clearManualBpm() {
		manualBpm = 0;
		tapTimes = [];
		clock.setBpm(0);
	}

	async function toggleMidi() {
		if (midiConnected) {
			midi?.destroy();
			midi = null;
			midiConnected = false;
			midiDeviceNames = [];
			learningAction = null;
			midiClockBpm = 0;
			return;
		}
		try {
			midi = new MidiEngine();
			await midi.connect();
			midiConnected = true;
			midiDeviceNames = midi.deviceNames;

			// Soft-takeover: Set<key> de contrôles déjà en phase avec la valeur app
			const takenOver = new Set<MidiTriggerKey>();

			midi.onMessage((msg) => {
				const key = triggerKey(msg);

				if (learningAction !== null) {
					if (msg.type === 'note_off') return;
					midiMappings = { ...midiMappings, [learningAction]: key };
					takenOver.add(key); // immédiatement en phase après learn
					learningAction = null;
					return;
				}

				for (const [action, mapped] of Object.entries(midiMappings) as [CommandId, MidiTriggerKey][]) {
					if (mapped !== key) continue;
					if (msg.type === 'note_off') break;

					// Normaliser : 14-bit sur 0..16383, sinon 7-bit sur 0..127
					const value01 = msg.is14bit ? msg.value / 16383 : msg.value / 127;

					// Soft-takeover uniquement pour les commandes range
					const cmd = registry.get(action);
					if (cmd?.kind === 'range' && !takenOver.has(key)) {
						const current = getCommandCurrentValue(action);
						if (current !== null && Math.abs(value01 - current) > 0.08) break;
						takenOver.add(key);
					}

					if (status === 'running') registry.dispatch(action, value01, commandCtx);
					break;
				}
			});

			// MIDI clock IN → alimente la Clock (24 pulses par quarter note)
			let _clockPulses = 0;
			let _clockTsRing: number[] = [];
			let _clockTimer: ReturnType<typeof setTimeout> | null = null;

			midi.onClock(() => {
				const now = performance.now();
				_clockPulses++;
				_clockTsRing.push(now);
				if (_clockTsRing.length > 49) _clockTsRing.shift();

				// Mise à jour BPM toutes les 6 pulses (≈4× par beat à 120 BPM)
				if (_clockPulses % 6 === 0 && _clockTsRing.length >= 7) {
					const recent = _clockTsRing.slice(-7);
					const intervals = recent.slice(1).map((t, i) => t - recent[i]);
					const avg = intervals.reduce((a, b) => a + b, 0) / intervals.length;
					const bpm = Math.round(60000 / (avg * 24) * 10) / 10;
					if (bpm >= 40 && bpm <= 300) {
						midiClockBpm = bpm;
						clock.setBpm(bpm);
					}
				}

				// Beat sur chaque quarter note (24 pulses)
				if (_clockPulses % 24 === 0) clock.pulse();

				// Timeout d'inactivité : MIDI clock arrêté depuis 2s
				if (_clockTimer !== null) clearTimeout(_clockTimer);
				_clockTimer = setTimeout(() => {
					midiClockBpm = 0;
					_clockPulses = 0;
					_clockTsRing = [];
					_clockTimer = null;
				}, 2000);
			});
		} catch (e) {
			midiConnected = false;
			sourceError = e instanceof Error ? e.message : String(e);
		}
	}

	function startLearn(action: CommandId) {
		learningAction = learningAction === action ? null : action;
	}

	function clearMapping(action: CommandId) {
		const { [action]: _, ...rest } = midiMappings;
		midiMappings = rest as Partial<Record<CommandId, MidiTriggerKey>>;
	}

	function clearKeyBinding(cmdId: CommandId) {
		const key = keyById.get(cmdId);
		if (!key) return;
		const { [key]: _, ...rest } = keymap;
		keymap = rest as KeyBinding;
	}

	function doResetKeymap() {
		keymap = resetKeymap();
	}

	function applyMidiAction(action: CommandId, value: number) {
		if (status !== 'running') return;
		registry.dispatch(action, value / 127, commandCtx);
	}

	/** Lire la valeur courante (0..1) d'une commande range, pour le soft-takeover. */
	function getCommandCurrentValue(id: CommandId): number | null {
		if (id === 'crossfader') return crossfader;
		const m = id.match(/^color-(\w+)-([ab])$/);
		if (!m) return null;
		const e = COLOR_CMDS.find(([s]) => s === m[1]);
		return e ? (m[2] === 'a' ? colorParamsA : colorParamsB)[e[1]] : null;
	}

	async function selectPresetForDeck(deck: 'A' | 'B', name: string) {
		const d = await loadPresetData(name);
		if (!d) return;
		if (deck === 'A') {
			presetA = name;
			manager.loadPreset(0, d, transitionTime);
			sync?.sendPreset('A', name, transitionTime);
		} else {
			presetB = name;
			manager.loadPreset(1, d, transitionTime);
			sync?.sendPreset('B', name, transitionTime);
		}
	}

	function exportPlaylists() {
		const data = JSON.stringify({
			version: 1,
			playlistA: playlistAItems,
			playlistB: playlistBItems,
			intervalSec: playlistIntervalSec,
			mode: playlistMode,
		}, null, 2);
		const blob = new Blob([data], { type: 'application/json' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = 'opendrop-playlists.json';
		a.click();
		URL.revokeObjectURL(url);
	}

	function importPlaylists(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0];
		if (!file) return;
		const reader = new FileReader();
		reader.onload = () => {
			try {
				const data = JSON.parse(reader.result as string);
				if (Array.isArray(data.playlistA)) playlistAItems = data.playlistA;
				if (Array.isArray(data.playlistB)) playlistBItems = data.playlistB;
				if (typeof data.intervalSec === 'number') playlistIntervalSec = data.intervalSec;
				if (data.mode === 'sequential' || data.mode === 'shuffle') playlistMode = data.mode;
				playlistA?.setItems(playlistAItems);
				playlistB?.setItems(playlistBItems);
			} catch {}
		};
		reader.readAsText(file);
		(e.target as HTMLInputElement).value = '';
	}

	function openOutput() {
		outputWinRef = window.open('/output', 'opendrop-output', 'width=1280,height=720');
		outputOpen = true;
		// Give the window ~800ms to init, then push current state
		setTimeout(() => {
			sync?.sendPreset('A', busPresetA);
			sync?.sendPreset('B', busPresetB);
			sync?.sendCrossfader(crossfader);
			if (currentDeviceId) sync?.sendSource(currentDeviceId);
		}, 800);
		// Poll for output window closure to stop PCM capture and release resources.
		if (outputCloseTimer !== null) clearInterval(outputCloseTimer);
		outputCloseTimer = setInterval(() => {
			if (outputWinRef?.closed) {
				audio?.stopPcmCapture();
				outputOpen = false;
				outputWinRef = null;
				clearInterval(outputCloseTimer!);
				outputCloseTimer = null;
			}
		}, 1500);
	}

	async function openOutputFullscreen() {
		if (!isElectron) {
			// Web fallback: fullscreen the visualizer area
			const el = document.querySelector('.visualizer-wrap') as HTMLElement | null;
			el?.requestFullscreen?.();
			return;
		}
		const res = await window.electronAPI!.openOutputOnDisplay(selectedDisplayId);
		if (res?.ok) {
			outputOpen = true;
			// Push current state after the window loads
			setTimeout(() => {
				sync?.sendPreset('A', busPresetA);
				sync?.sendPreset('B', busPresetB);
				sync?.sendCrossfader(crossfader);
				if (currentDeviceId) sync?.sendSource(currentDeviceId);
			}, 800);
		}
	}

	function onResize() {
		if (status !== 'running') return;
		for (let i = 0; i < 4; i++) {
			const c = canvases[i];
			if (c) manager.resize(i, c.clientWidth, c.clientHeight);
		}
		if (compositorCanvas) {
			compositor?.resize(compositorCanvas.clientWidth, compositorCanvas.clientHeight, getQualitySettings(quality).pixelRatio);
		}
	}

	async function toggleNdi() {
		ndiError = '';
		const eAPI = window.electronAPI;
		if (ndiActive) {
			await eAPI?.ndiStop();
			ndiActive = false;
		} else {
			const w = window.screen.width;
			const h = window.screen.height;
			const res = await eAPI?.ndiStart('OpenDrop VJ', w, h);
			if (res?.ok) ndiActive = true;
			else ndiError = res?.error ?? 'NDI SDK non trouvé — installez le NDI Runtime depuis ndi.video.';
		}
	}

	async function toggleV4l2() {
		v4l2Error = '';
		const eAPI = window.electronAPI;
		if (v4l2Active) {
			await eAPI?.v4l2Stop();
			v4l2Active = false;
		} else {
			const res = await eAPI?.v4l2Start();
			if (res?.ok) v4l2Active = true;
			else v4l2Error = res?.error ?? 'Erreur v4l2 inconnue.';
		}
	}

	async function toggleSpout() {
		spoutError = '';
		const eAPI = window.electronAPI;
		if (spoutActive) {
			await eAPI?.spoutStop();
			spoutActive = false;
		} else {
			const res = await eAPI?.spoutStart('OpenDrop VJ');
			if (res?.ok) spoutActive = true;
			else spoutError = res?.error ?? 'Spout indisponible.';
		}
	}

	async function toggleOsc() {
		oscError = '';
		const eAPI = window.electronAPI;
		if (oscActive) {
			await eAPI?.stopOsc?.();
			oscActive = false;
		} else {
			const res = await eAPI?.startOsc?.(oscPort);
			if (res?.ok) oscActive = true;
			else oscError = res?.error ?? 'Erreur OSC.';
		}
	}

	async function toggleRemote() {
		remoteError = '';
		const eAPI = window.electronAPI;
		if (remoteActive) {
			await eAPI?.stopRemote?.();
			remoteActive = false;
			remoteUrl = '';
		} else {
			const res = await eAPI?.startRemote?.();
			if (res?.ok) {
				remoteActive = true;
				remoteUrl = `https://opendrop.kushie.dev/remote?host=${res.ip}&port=${res.port}&token=${res.token}`;
			} else {
				remoteError = res?.error ?? 'Erreur Remote.';
			}
		}
	}

	async function toggleLink() {
		linkError = '';
		const eAPI = window.electronAPI;
		if (linkActive) {
			await eAPI?.stopLink?.();
			linkActive = false;
			linkPeers = 0;
		} else {
			const res = await eAPI?.startLink?.(manualBpm || clock.bpm || 120);
			if (res?.ok) {
				linkActive = true;
				if (res.tempo) clock.setBpm(res.tempo);
			} else {
				linkError = res?.error ?? 'Ableton Link non disponible.';
			}
		}
	}

	async function startSlot(slot: number) {
		if (!audio || status !== 'running') return;
		const q = getQualitySettings(quality);
		const name = presets4[slot];
		const presetData = name ? await loadPresetData(name) : null;
		await manager.start(slot, audio.ctx, audio.gainNode, q, presetData);
		_slotEpoch++;
	}

	function pauseSlot(slot: number) {
		manager.pause(slot);
		_slotEpoch++;
	}

	function cycleBus(slot: number) {
		const order: Array<'A' | 'B' | 'off'> = ['A', 'B', 'off'];
		const next = order[(order.indexOf(deckBus[slot]) + 1) % order.length];
		deckBus = deckBus.map((b, i) => (i === slot ? next : b)) as Array<'A' | 'B' | 'off'>;
	}

	function isRunning(slot: number): boolean {
		return manager.isRunning(slot);
	}

	function onKeydown(e: KeyboardEvent) {
		const tag = (e.target as HTMLElement).tagName;
		if (tag === 'INPUT' || tag === 'TEXTAREA') return;

		if (learningKey !== null) {
			if (e.key === 'Escape') { learningKey = null; e.preventDefault(); return; }
			keymap = { ...keymap, [e.key]: learningKey };
			learningKey = null;
			e.preventDefault();
			return;
		}

		const action = keymap[e.key];
		if (!action) return;
		e.preventDefault();
		if (status !== 'running') return;
		registry.dispatch(action, 1, commandCtx);
	}
</script>

<svelte:window onresize={onResize} onkeydown={onKeydown} />
<audio bind:this={audioEl} style="display:none" crossorigin="anonymous"></audio>

<main>
{#snippet audioSection()}
  <SidebarAudio
    {sourceLabel}
    {status}
    {effectiveOS}
    {vuLevel}
    {sourceError}
    {showSystemAudioHelp}
    {showDevicePicker}
    {audioDevices}
    {outputDevices}
    {loopbackSupported}
    audioElHasSrc={!!audioEl?.src}
    onConnectMic={connectMic}
    onOpenDevicePicker={openDevicePicker}
    onCaptureSystemAudio={captureSystemAudio}
    onConnectFile={connectFile}
    {onFileChange}
    onConnectDevice={connectDevice}
    onConnectLoopback={connectLoopback}
    onDismissSystemAudioHelp={() => { showSystemAudioHelp = false }}
    onDismissDevicePicker={() => { showDevicePicker = false }}
  />
{/snippet}
{#snippet videoSection()}
  <SidebarVideo
    {videoEnabled}
    {videoOpacity}
    {videoAdvance}
    {videoBeatsPerCut}
    {vrCut}
    {vrFlash}
    {vrWarp}
    {vrHue}
    {currentClipIndex}
    {allClips}
    onToggleVideo={() => { videoEnabled = !videoEnabled }}
    onOpacityChange={(v) => { videoOpacity = v }}
    onAdvanceChange={(v) => { videoAdvance = v }}
    onBeatsPerCutChange={(v) => { videoBeatsPerCut = v }}
    onToggleVrCut={() => { vrCut = !vrCut }}
    onToggleVrFlash={() => { vrFlash = !vrFlash }}
    onToggleVrWarp={() => { vrWarp = !vrWarp }}
    onToggleVrHue={() => { vrHue = !vrHue }}
    onSelectClip={(i) => { currentClipIndex = i }}
    onRemoveClip={(i) => removeVideoClip(i)}
    onAddVideo={onVideoFilePick}
  />
{/snippet}
{#snippet qualiteSection()}
	<div class="controls-section">
		<div class="pl-header">
			<span class="label">Qualité rendu</span>
			{#if status === 'running' && fps > 0}
				<span class="label" style="color:var(--info)">{fps} fps</span>
			{/if}
		</div>
		<div class="btn-row">
			<button class="btn-sm" class:active={quality === 'low'} onclick={() => quality = 'low'} disabled={status !== 'running'}>Low</button>
			<button class="btn-sm" class:active={quality === 'medium'} onclick={() => quality = 'medium'} disabled={status !== 'running'}>Med</button>
			<button class="btn-sm" class:active={quality === 'high'} onclick={() => quality = 'high'} disabled={status !== 'running'}>High</button>
		</div>
		<div class="btn-row" style="margin-top:6px">
			<button class="btn-sm" class:active={targetFps === 30} onclick={() => targetFps = 30} disabled={status !== 'running'}>30 fps</button>
			<button class="btn-sm" class:active={targetFps === 45} onclick={() => targetFps = 45} disabled={status !== 'running'}>45 fps</button>
			<button class="btn-sm" class:active={targetFps === 60} onclick={() => targetFps = 60} disabled={status !== 'running'}>60 fps</button>
		</div>
		<div class="btn-row" style="margin-top:4px">
			<button class="btn-sm" class:active={invisibleMode === 'eco'} onclick={() => invisibleMode = 'eco'} disabled={status !== 'running'} title="Decks cachés à ~8 fps">Éco</button>
			<button class="btn-sm" class:active={invisibleMode === 'pause'} onclick={() => invisibleMode = 'pause'} disabled={status !== 'running'} title="Decks cachés pausés">Pause</button>
			<button class="btn-sm" class:active={invisibleMode === 'off'} onclick={() => invisibleMode = 'off'} disabled={status !== 'running'} title="Tous les decks à plein régime">Off</button>
		</div>
	</div>
{/snippet}
{#snippet outputSection()}
	<div class="controls-section">
		<div class="output-row">
			<button class="btn-output" onclick={openOutput} disabled={status !== 'running'}>
				⎋ Open output window
			</button>
			{#if isElectron && outputOpen}
				<button class="btn-stream" class:stream-active={ndiActive || v4l2Active || spoutActive}
					onclick={() => showStreamPanel = !showStreamPanel}
					title="Stream output">
					⏏ Stream {ndiActive || v4l2Active || spoutActive ? '●' : '○'}
				</button>
			{/if}
		</div>
		{#if isElectron && displays.length > 0}
			<div class="midi-row" style="gap:6px;align-items:center;margin-top:6px">
				<select
					style="flex:1;font-size:10px;background:#1a1a1a;border:1px solid #333;border-radius:3px;color:#ccc;padding:3px 4px"
					value={selectedDisplayId}
					onchange={(e) => { selectedDisplayId = Number(e.currentTarget.value); }}
				>
					{#each displays as d}
						<option value={d.id}>{d.label} ({d.bounds.width}×{d.bounds.height})</option>
					{/each}
				</select>
				<button class="btn-sm" onclick={openOutputFullscreen} disabled={status !== 'running'} title="Ouvrir en plein écran sur cet écran">
					⛶ Fullscreen
				</button>
			</div>
		{:else if !isElectron}
			<button class="btn-sm" onclick={openOutputFullscreen} disabled={status !== 'running'} style="margin-top:6px;width:100%" title="Plein écran (appui F pour quitter)">
				⛶ Fullscreen
			</button>
		{/if}
		{#if outputOpen && !isElectron}
			<span class="label" style="color:var(--info)">Output window open — use as OBS Browser Source</span>
		{/if}
		{#if showStreamPanel && isElectron}
			<div class="stream-panel">
				{#if platform === 'linux'}
					<button class="stream-btn" class:stream-btn--on={v4l2Active} onclick={toggleV4l2}
						title={v4l2Active ? 'Stop V4L2' : 'Start V4L2 (webcam virtuelle)'}>
						V4L2 {v4l2Active ? '●' : '○'}
					</button>
				{/if}
				<button class="stream-btn stream-btn--ndi" class:stream-btn--on={ndiActive} onclick={toggleNdi}
					title={ndiActive ? 'Stop NDI' : 'Start NDI'}>
					NDI {ndiActive ? '●' : '○'}
				</button>
				{#if platform === 'win32'}
					<button class="stream-btn stream-btn--spout" class:stream-btn--on={spoutActive} onclick={toggleSpout}
						title={spoutActive ? 'Stop Spout' : 'Start Spout'}>
						SPOUT {spoutActive ? '●' : '○'}
					</button>
				{/if}
				{#if v4l2Error}<div class="stream-error">{v4l2Error}</div>{/if}
				{#if ndiError}<div class="stream-error">{ndiError}</div>{/if}
				{#if spoutError}<div class="stream-error">{spoutError}</div>{/if}
			</div>
		{/if}
	</div>
{/snippet}
{#snippet midiSection()}
	<div class="controls-section">
		<div class="pl-header">
			<span class="label">MIDI</span>
			{#if midiSupported}
				<button class="btn-sm" class:active={midiConnected} onclick={toggleMidi}>
					{midiConnected ? 'Déconnecter' : 'Connecter'}
				</button>
			{:else}
				<span style="font-size:10px;color:var(--error)">Chromium only</span>
			{/if}
		</div>
		{#if midiConnected}
			<span class="source-badge">▶ {midiDeviceNames.length > 0 ? midiDeviceNames.join(', ') : 'aucun périphérique'}</span>
			{#if midiClockBpm > 0}
				<span class="source-badge" style="color:var(--ok)">♩ MIDI Clock {midiClockBpm} BPM</span>
			{/if}
			{#if learningAction !== null}
				<span style="font-size:11px;color:var(--warn)">Bouge un knob/bouton sur ton contrôleur…</span>
			{/if}
			<div class="midi-list">
				{#each registry.all() as cmd}
					{@const mapped = midiMappings[cmd.id]}
					<div class="midi-row">
						<span class="midi-label">{cmd.label}</span>
						<span class="midi-binding" class:midi-learning={learningAction === cmd.id}>
							{mapped ? formatTrigger(mapped) : '—'}
						</span>
						<button class="btn-sm pl-btn" class:active={learningAction === cmd.id}
							onclick={() => startLearn(cmd.id)}>
							{learningAction === cmd.id ? '…' : 'Learn'}
						</button>
						{#if mapped}
							<button class="pl-remove" onclick={() => clearMapping(cmd.id)}>×</button>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	</div>
{/snippet}
{#snippet clavierSection()}
	<div class="controls-section">
		<div class="pl-header">
			<span class="label">Clavier</span>
			<button class="btn-sm" onclick={doResetKeymap}>Reset</button>
		</div>
		{#if learningKey !== null}
			<span style="font-size:11px;color:var(--warn)">Appuie sur la touche à assigner… (Esc = annuler)</span>
		{/if}
		<div class="midi-list">
			{#each registry.all() as cmd}
				{@const assignedKey = keyById.get(cmd.id)}
				<div class="midi-row">
					<span class="midi-label">{cmd.label}</span>
					<span class="midi-binding" class:midi-learning={learningKey === cmd.id}>
						{assignedKey ? formatKey(assignedKey) : '—'}
					</span>
					<button class="btn-sm pl-btn" class:active={learningKey === cmd.id}
						onclick={() => { learningKey = learningKey === cmd.id ? null : cmd.id; }}>
						{learningKey === cmd.id ? '…' : 'Learn'}
					</button>
					{#if assignedKey}
						<button class="pl-remove" onclick={() => clearKeyBinding(cmd.id)}>×</button>
					{/if}
				</div>
			{/each}
		</div>
	</div>
{/snippet}
{#snippet strobeSection()}
	<div class="controls-section">
		<div class="pl-header">
			<span class="label">Strobe</span>
			<button class="btn-sm" class:active={strobeOn} onclick={() => { strobeOn = !strobeOn; }}>
				{strobeOn ? 'ON' : 'OFF'}
			</button>
		</div>
		{#if strobeOn}
			<div class="midi-row" style="gap:4px;flex-wrap:wrap">
				<span class="midi-label">Rate</span>
				{#each [0.25, 0.5, 1, 2, 4] as r}
					<button class="btn-sm" class:active={strobeRate === r}
						onclick={() => { strobeRate = r; }}>
						{r < 1 ? `1/${Math.round(1/r)}` : `${r}×`}
					</button>
				{/each}
			</div>
			<div class="midi-row" style="gap:6px;align-items:center">
				<span class="midi-label">Intensité</span>
				<input type="range" min="0" max="1" step="0.05" bind:value={strobeIntensity} style="flex:1" />
				<span style="font-size:10px;color:#aaa">{Math.round(strobeIntensity*100)}%</span>
			</div>
			<div class="midi-row" style="gap:6px;align-items:center">
				<span class="midi-label">Couleur</span>
				<input type="color" bind:value={strobeColor} style="width:32px;height:20px;padding:0;border:none;background:none;cursor:pointer" />
			</div>
		{/if}
	</div>
{/snippet}
{#snippet lfoSection()}
	<div class="controls-section">
		<div class="pl-header"><span class="label">LFO</span></div>
		{#each lfoSlots as slot, i}
			<div style="margin-bottom:6px;font-size:11px">
				<div class="midi-row" style="gap:4px;flex-wrap:wrap">
					<input type="checkbox" bind:checked={slot.enabled} />
					<span class="midi-label">LFO {i+1}</span>
					{#each (['sine','saw','square','sh'] as const) as shape}
						<button class="btn-sm" class:active={slot.shape === shape}
							onclick={() => { slot.shape = shape; }}>
							{shape}
						</button>
					{/each}
				</div>
				{#if slot.enabled}
					<div class="midi-row" style="gap:6px;align-items:center;margin-top:3px">
						<span class="midi-label">Cible</span>
						<select style="flex:1;font-size:10px;background:#222;color:#ccc;border:1px solid #444;border-radius:3px"
							value={slot.target ?? ''}
							onchange={(e) => { slot.target = (e.currentTarget.value || null) as typeof slot.target; }}>
							<option value="">—</option>
							{#each registry.all().filter(c => c.kind === 'range') as cmd}
								<option value={cmd.id}>{cmd.label}</option>
							{/each}
						</select>
					</div>
					<div class="midi-row" style="gap:6px;align-items:center;margin-top:2px">
						<span class="midi-label">Rate</span>
						<input type="range" min="0.25" max="4" step="0.25" bind:value={slot.rate} style="flex:1" />
						<span style="font-size:10px;color:#aaa">{slot.rate}×</span>
					</div>
					<div class="midi-row" style="gap:6px;align-items:center;margin-top:2px">
						<span class="midi-label">Amount</span>
						<input type="range" min="0" max="1" step="0.05" bind:value={slot.amount} style="flex:1" />
					</div>
				{/if}
			</div>
		{/each}
	</div>
{/snippet}
{#snippet colorDeck(label: string, params: ColorParams, onUpdate: (p: ColorParams) => void, mt?: string)}
	<div class="pl-header" style={mt}>
		<span class="label">Color {label}</span>
		<button class="btn-sm" onclick={() => onUpdate({ ...DEFAULT_COLOR_PARAMS })}>↺</button>
	</div>
	{#each ([['Hue', 'hueRotate', 0, 1, '°', 360], ['Sat', 'saturate', 0, 1, '%', 200], ['Bright', 'brightness', 0, 1, '%', 200], ['Contrast', 'contrast', 0, 1, '%', 200], ['Invert', 'invert', 0, 1, '%', 100]] as const) as [lbl, key, min, max, unit, scale]}
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label" style="width:48px">{lbl}</span>
			<input type="range" {min} {max} step="0.01" value={params[key]}
				oninput={(e) => { onUpdate({ ...params, [key]: +e.currentTarget.value }); }}
				style="flex:1" />
			<span style="font-size:9px;color:#aaa;width:28px;text-align:right">{Math.round(params[key] * scale)}{unit}</span>
		</div>
	{/each}
{/snippet}
{#snippet colorSection()}
	<div class="controls-section">
		{@render colorDeck('A', colorParamsA, (p) => { colorParamsA = p; })}
		{@render colorDeck('B', colorParamsB, (p) => { colorParamsB = p; }, 'margin-top:6px')}
	</div>
{/snippet}
{#snippet electronSection()}
	{#if isElectron}
	<div class="controls-section">
		<div class="pl-header">
			<span class="label">OSC</span>
			<button class="btn-sm" class:active={oscActive} onclick={toggleOsc}>
				{oscActive ? 'Stop' : 'Start'}
			</button>
		</div>
		{#if oscActive}
			<span class="source-badge">Écoute UDP :{oscPort}</span>
			<span style="font-size:10px;color:#aaa">Adresse : /opendrop/&lt;commandId&gt; float32</span>
		{:else}
			<div class="midi-row" style="gap:6px;align-items:center">
				<span class="midi-label">Port</span>
				<input type="number" min="1024" max="65535" bind:value={oscPort}
					style="width:70px;background:#1a1a1a;border:1px solid #333;border-radius:3px;color:#ccc;font-size:11px;padding:2px 4px" />
			</div>
		{/if}
		{#if oscError}<div style="font-size:10px;color:var(--error);margin-top:4px">{oscError}</div>{/if}
	</div>
	<div class="controls-section">
		<div class="pl-header">
			<span class="label">Remote</span>
			<button class="btn-sm" class:active={remoteActive} onclick={toggleRemote}>
				{remoteActive ? 'Stop' : 'Démarrer'}
			</button>
		</div>
		{#if remoteActive && remoteUrl}
			<span style="font-size:10px;color:#aaa;word-break:break-all">{remoteUrl}</span>
			<a href={remoteUrl} target="_blank" rel="noopener" style="font-size:10px;color:var(--info);display:block;margin-top:4px">
				Ouvrir sur cet appareil ↗
			</a>
		{/if}
		{#if !remoteActive}
			<span style="font-size:10px;color:#666">Démarre un serveur WS local pour piloter OpenDrop depuis un téléphone sur le même réseau.</span>
		{/if}
		{#if remoteError}<div style="font-size:10px;color:var(--error);margin-top:4px">{remoteError}</div>{/if}
	</div>
	<div class="controls-section">
		<div class="pl-header">
			<span class="label">Ableton Link</span>
			<button class="btn-sm" class:active={linkActive} onclick={toggleLink}>
				{linkActive ? 'Stop' : 'Démarrer'}
			</button>
		</div>
		{#if linkActive}
			<span class="source-badge">{linkPeers} pair{linkPeers !== 1 ? 's' : ''} connecté{linkPeers !== 1 ? 's' : ''}</span>
		{:else}
			<span style="font-size:10px;color:#666">Synchronise le tempo avec Ableton Live et autres apps Link sur le réseau local.</span>
		{/if}
		{#if linkError}<div style="font-size:10px;color:var(--error);margin-top:4px">{linkError}</div>{/if}
	</div>
	{/if}
{/snippet}
{#if layout === 'stage'}
	<div
		class="visualizer-wrap"
		class:drag-over={overlayDragOver}
		ondragover={onVisualizerDragOver}
		ondragleave={() => overlayDragOver = false}
		ondrop={onVisualizerDrop}
		role="region"
		aria-label="Visualizer"
	>
		<!-- Video loop — premier enfant = derrière les decks -->
		<VideoLayer clip={currentClip} opacity={videoOpacity} {beat} playbackRate={videoPlaybackRate} flashOn={vrFlash} hueOn={vrHue} />
		<!-- Deck canvases — 4 slots, texture sources pour le Compositor (cachés) -->
		{#each [0, 1, 2, 3] as i}
			<canvas bind:this={canvases[i]} class="deck-src"></canvas>
		{/each}
		<!-- Rendu composé (blend + lumaKey + colorKey par slot) -->
		<canvas bind:this={compositorCanvas} class="deck-canvas" style:mix-blend-mode={videoEnabled ? 'screen' : 'normal'}></canvas>
		<!-- Overlay sprites -->
		<OverlayLayer {overlays} {beat} />
		<!-- Strobe flash — top z-index, pointer-events none -->
		{#if strobeOn && strobeFlash}
			<div class="strobe-flash" style="background:{strobeColor};opacity:{strobeIntensity}"></div>
		{/if}

		{#if status === 'idle'}
			<div class="overlay">
				<h1 class="logo">OpenDrop</h1>
				<p class="tagline">Milkdrop visualizer — web-first</p>
				<button class="btn-primary" onclick={startVisualizer}>▶ Start</button>
			</div>
		{/if}

		{#if status === 'error'}
			<div class="overlay error">
				<p>⚠ {errorMsg}</p>
				<button class="btn-secondary" onclick={() => { status = 'idle'; errorMsg = ''; }}>Retry</button>
			</div>
		{/if}
	</div>

	<aside class="controls">
		<!-- Layout toggle -->
		<div class="controls-section">
			<LayoutToggle {layout} onToggle={(l) => { layout = l }} />
		</div>

		<!-- Audio source -->
		<SidebarAudio
			{sourceLabel}
			{status}
			{effectiveOS}
			{vuLevel}
			{sourceError}
			{showSystemAudioHelp}
			{showDevicePicker}
			{audioDevices}
			{outputDevices}
			{loopbackSupported}
			audioElHasSrc={!!audioEl?.src}
			onConnectMic={connectMic}
			onOpenDevicePicker={openDevicePicker}
			onCaptureSystemAudio={captureSystemAudio}
			onConnectFile={connectFile}
			{onFileChange}
			onConnectDevice={connectDevice}
			onConnectLoopback={connectLoopback}
			onDismissSystemAudioHelp={() => showSystemAudioHelp = false}
			onDismissDevicePicker={() => showDevicePicker = false}
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
						isActive={activeSlot === i}
						isLive={opacities[i] > 0.5}
						bus={deckBus[i]}
						running={isRunning(i)}
						onSelect={() => { activeSlot = i }}
						onCycleBus={() => cycleBus(i)}
						onToggleRun={() => isRunning(i) ? pauseSlot(i) : startSlot(i)}
					/>
				{/each}
			</div>
			<div class="crossfader-row">
				<span class="cf-label" class:bright={crossfader < 0.2}>A</span>
				<input class="crossfader" type="range" min="0" max="1" step="0.01" bind:value={crossfader} />
				<span class="cf-label" class:bright={crossfader > 0.8}>B</span>
			</div>
			<div class="transition-row">
				<span class="transition-label">Fondu</span>
				<input class="transition-slider" type="range" min="0" max="5" step="0.1" bind:value={transitionTime} title="Durée de transition preset (s)" />
				<span class="transition-value">{transitionTime.toFixed(1)}s</span>
				<button class="btn-sm" onclick={() => { transitionTime = 0 }} title="Coupe nette">Hard Cut</button>
			</div>
			<div class="blendmode-row">
				<span class="transition-label">Mix</span>
				<select class="blendmode-select" bind:value={deckBlendMode}>
					<option value="screen">Screen (additif)</option>
					<option value="plus-lighter">Plus Lighter</option>
					<option value="multiply">Multiply</option>
					<option value="overlay">Overlay</option>
					<option value="lighten">Lighten</option>
					<option value="hard-light">Hard Light</option>
					<option value="difference">Difference</option>
				</select>
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
			{playlistMode}
			{playlistIntervalSec}
			{beatSyncA}
			{beatSyncB}
			{autoXfade}
			{beatsPerChange}
			{detectedBpm}
			{manualBpm}
			{playlistAItems}
			{playlistBItems}
			{playlistAPlaying}
			{playlistBPlaying}
			audioRunning={status === 'running'}
			{presetA}
			{presetB}
			{lockA}
			{lockB}
			onModeChange={(m) => { playlistMode = m }}
			onIntervalChange={(s) => { playlistIntervalSec = s }}
			onBeatsPerChangeChange={(n) => { beatsPerChange = n }}
			onTapTempo={tapTempo}
			onClearManualBpm={clearManualBpm}
			onToggleBeatSyncA={() => toggleBeatSync('A')}
			onToggleBeatSyncB={() => toggleBeatSync('B')}
			onToggleAutoXfade={() => { autoXfade = !autoXfade; autoXfadeCount = 0; }}
			onTogglePlaylistA={() => togglePlaylist('A')}
			onTogglePlaylistB={() => togglePlaylist('B')}
			onPlaylistNext={(deck) => playlistNext(deck)}
			onPlaylistPrev={(deck) => playlistPrev(deck)}
			onRemoveFromPlaylistA={(name) => removeFromPlaylist('A', name)}
			onRemoveFromPlaylistB={(name) => removeFromPlaylist('B', name)}
			onToggleLockA={() => { lockA = !lockA }}
			onToggleLockB={() => { lockB = !lockB }}
			onExportPlaylists={exportPlaylists}
			onImportPlaylists={importPlaylists}
		/>

		<!-- Overlays -->
		<SidebarOverlays
			{overlays}
			onAddOverlays={onOverlayFilePick}
			onRemoveOverlay={(id) => removeOverlay(id)}
			onUpdateOverlay={(id, patch) => updateOverlay(id, patch)}
		/>

		<!-- Video loops -->
		<SidebarVideo
			{videoEnabled}
			{videoOpacity}
			{videoAdvance}
			{videoBeatsPerCut}
			{vrCut}
			{vrFlash}
			{vrWarp}
			{vrHue}
			{currentClipIndex}
			{allClips}
			onToggleVideo={() => { videoEnabled = !videoEnabled }}
			onOpacityChange={(v) => { videoOpacity = v }}
			onAdvanceChange={(v) => { videoAdvance = v }}
			onBeatsPerCutChange={(v) => { videoBeatsPerCut = v }}
			onToggleVrCut={() => { vrCut = !vrCut }}
			onToggleVrFlash={() => { vrFlash = !vrFlash }}
			onToggleVrWarp={() => { vrWarp = !vrWarp }}
			onToggleVrHue={() => { vrHue = !vrHue }}
			onSelectClip={(i) => { currentClipIndex = i }}
			onRemoveClip={(i) => removeVideoClip(i)}
			onAddVideo={onVideoFilePick}
		/>

		{@render qualiteSection()}

		{@render outputSection()}

		{@render midiSection()}

		{@render clavierSection()}

		{@render strobeSection()}

		{@render lfoSection()}

		{@render colorSection()}

		{@render electronSection()}

	</aside>
	<PresetBrowser
		presets={presetList}
		isOpen={showPresetBrowser}
		{activeDeck}
		targetSlot={activeSlot}
		{playlistAItems}
		{playlistBItems}
		onClose={() => { showPresetBrowser = false }}
		onLoadPreset={selectPreset}
		onAddToPlaylist={addToPlaylist}
	/>
{:else}
  <MixerLayout
    {canvases}
    {presets4}
    {deckBus}
    {runningCount}
    {isRunning}
    selectedSlot={mixerSelectedSlot}
    {crossfader}
    {transitionTime}
    {deckBlendMode}
    {presetList}
    {playlistAItems}
    {playlistBItems}
    {layout}
    onStartSlot={startSlot}
    onPauseSlot={pauseSlot}
    onSelectSlot={(s) => { mixerSelectedSlot = s }}
    onCycleBus={cycleBus}
    onCrossfaderChange={(v) => { crossfader = v }}
    onTransitionChange={(v) => { transitionTime = v }}
    onBlendModeChange={(mode) => { deckBlendMode = mode }}
    onLoadPreset={selectPreset}
    onAddToPlaylist={addToPlaylist}
    onLayoutToggle={(l) => { layout = l }}
    {audioSection}
    {videoSection}
    {qualiteSection}
    {outputSection}
    {midiSection}
    {clavierSection}
    {strobeSection}
    {lfoSection}
    {colorSection}
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

	.btn-row { display: flex; gap: 0.4rem; }

	.source-badge { font-size: 11px; color: var(--cyan); }


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

	.blendmode-row { display: flex; align-items: center; gap: 0.4rem; margin-top: 0.3rem; }
	.blendmode-select {
		flex: 1; background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.4rem; font-size: 11px; cursor: pointer;
	}

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

	.pl-btn { padding: 0.22rem 0.4rem; font-size: 11px; }

	.pl-header { display: flex; align-items: center; justify-content: space-between; }

	.pl-remove {
		background: none; border: none; color: var(--text-muted);
		cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; flex-shrink: 0;
		transition: color var(--t-fast);
	}

	.pl-remove:hover { color: var(--accent); }

	/* MIDI */
	.midi-list {
		display: flex; flex-direction: column; gap: 2px;
		max-height: 160px; overflow-y: auto;
	}

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.midi-label { font-size: 10px; color: var(--text-muted); width: 80px; flex-shrink: 0; white-space: nowrap; }

	.midi-binding {
		flex: 1; font-size: 10px; color: var(--text-muted); font-family: 'Courier New', monospace;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.midi-binding.midi-learning { color: var(--warn); animation: blink 0.6s step-end infinite; }

	@keyframes blink { 50% { opacity: 0; } }

	/* Output button */
	.btn-output {
		width: 100%;
		background: linear-gradient(135deg, rgba(0,229,255,0.08), rgba(180,79,255,0.08));
		color: var(--cyan); border: 1px solid #004455;
		border-radius: 6px; padding: 0.45rem; font-size: 12px; font-weight: 600;
		cursor: pointer; letter-spacing: 0.03em;
		transition: all 0.15s;
		box-shadow: 0 0 12px rgba(0,229,255,0.1);
	}

	.btn-output:hover:not(:disabled) {
		background: linear-gradient(135deg, rgba(0,229,255,0.14), rgba(180,79,255,0.14));
		box-shadow: 0 0 20px rgba(0,229,255,0.25);
		border-color: var(--cyan);
	}

	.btn-output:disabled { opacity: 0.3; cursor: not-allowed; }

	.output-row {
		display: flex;
		gap: 6px;
		align-items: center;
	}

	.btn-stream {
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 700;
		padding: 4px 8px;
		cursor: pointer;
		transition: all 0.15s;
	}
	.btn-stream:hover { border-color: #aaa; color: #aaa; }
	.btn-stream.stream-active { border-color: var(--info); color: var(--info); }

	.stream-panel {
		margin-top: 6px;
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		align-items: center;
	}

	.stream-btn {
		background: rgba(0,0,0,0.4);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 700;
		letter-spacing: 0.05em;
		padding: 4px 10px;
		cursor: pointer;
		transition: all 0.15s;
	}
	.stream-btn:hover { border-color: #aaa; color: #aaa; }
	.stream-btn--on { border-color: currentColor; }
	.stream-btn--ndi { color: var(--text-muted); }
	.stream-btn--ndi:hover, .stream-btn--ndi.stream-btn--on { border-color: var(--warn); color: var(--warn); background: var(--warn-dim, rgba(255,140,0,0.1)); }
	.stream-btn--spout { color: var(--text-muted); }
	.stream-btn--spout:hover, .stream-btn--spout.stream-btn--on { border-color: var(--violet); color: var(--violet); background: var(--violet-dim); }
	.stream-btn.stream-btn--on:not(.stream-btn--ndi):not(.stream-btn--spout) { border-color: var(--cyan); color: var(--cyan); background: var(--cyan-dim); }

	.stream-error {
		width: 100%;
		font-size: 10px;
		color: var(--error);
	}

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
