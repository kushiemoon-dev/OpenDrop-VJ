<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Deck } from '$lib/engine/deck.js';
	import { AudioEngine } from '$lib/engine/audio.js';
	import { MainSync } from '$lib/engine/sync.js';
	import { PlaylistEngine, type PlaylistMode } from '$lib/engine/playlist.js';
	import { initPresets, buildPresetList, loadPresetData, searchPresets, type PresetMeta } from '$lib/presets/index.js';
	import { MidiEngine, triggerKey, formatTrigger, type MidiTriggerKey } from '$lib/engine/midi.js';
	import { BeatDetector } from '$lib/engine/bpm.js';
	import { getQualitySettings, DEFAULT_TIER, type QualityTier } from '$lib/engine/quality.js';
	import { makeOverlay, saveAsset, deleteAsset, type Overlay } from '$lib/engine/overlay.js';
	import OverlayLayer from '$lib/components/OverlayLayer.svelte';

	// — State —————————————————————————————————————————————
	let canvasA: HTMLCanvasElement | undefined = $state();
	let canvasB: HTMLCanvasElement | undefined = $state();
	let deckA: Deck | null = null;
	let deckB: Deck | null = null;
	let audio: AudioEngine | null = null;

	let presetList: PresetMeta[] = $state([]);
	let searchQuery = $state('');

	let activeDeck = $state<'A' | 'B'>('A');
	let presetA = $state('');
	let presetB = $state('');
	let crossfader = $state(0); // 0 = 100% A, 1 = 100% B

	let sourceLabel = $state('none');
	let currentDeviceId = $state('');
	let status = $state<'idle' | 'running' | 'error'>('idle');
	let errorMsg = $state('');
	let sourceError = $state('');
	let audioEl: HTMLAudioElement | undefined = $state();
	let audioDevices = $state<MediaDeviceInfo[]>([]);
	let showDevicePicker = $state(false);
	let vuLevel = $state(0);
	let outputOpen = $state(false);
	let sync: MainSync | null = null;

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
	type MidiAction =
		| 'crossfader'
		| 'preset-prev-a' | 'preset-next-a'
		| 'preset-prev-b' | 'preset-next-b'
		| 'playlist-toggle-a' | 'playlist-toggle-b'
		| 'playlist-prev-a' | 'playlist-next-a'
		| 'playlist-prev-b' | 'playlist-next-b';

	const MIDI_ACTIONS: MidiAction[] = [
		'crossfader',
		'preset-prev-a', 'preset-next-a',
		'preset-prev-b', 'preset-next-b',
		'playlist-toggle-a', 'playlist-toggle-b',
		'playlist-prev-a', 'playlist-next-a',
		'playlist-prev-b', 'playlist-next-b',
	];

	const MIDI_LABELS: Record<MidiAction, string> = {
		'crossfader': 'Crossfader',
		'preset-prev-a': '◀ Preset A', 'preset-next-a': '▶ Preset A',
		'preset-prev-b': '◀ Preset B', 'preset-next-b': '▶ Preset B',
		'playlist-toggle-a': '⏯ Playlist A', 'playlist-toggle-b': '⏯ Playlist B',
		'playlist-prev-a': '⏮ Playlist A', 'playlist-next-a': '⏭ Playlist A',
		'playlist-prev-b': '⏮ Playlist B', 'playlist-next-b': '⏭ Playlist B',
	};

	const midiSupported = typeof navigator !== 'undefined' && 'requestMIDIAccess' in navigator;
	let midi: MidiEngine | null = null;
	let midiConnected = $state(false);
	let midiDeviceNames = $state<string[]>([]);
	let midiMappings = $state<Partial<Record<MidiAction, MidiTriggerKey>>>({});
	let learningAction = $state<MidiAction | null>(null);

	// — Electron ——————————————————————————————————————————
	const isElectron = typeof window !== 'undefined' && !!window.electronAPI?.isElectron;
	let platform = $state('');
	let showSystemAudioHelp = $state(false);

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
	let metronomeId: ReturnType<typeof setInterval> | null = null;

	// — Lock deck ——————————————————————————————————————————
	let lockA = $state(false);
	let lockB = $state(false);

	// — Qualité rendu ——————————————————————————————————————
	let quality = $state<QualityTier>(DEFAULT_TIER);
	let fps = $state(0);

	// — Overlays ——————————————————————————————————————————
	let overlays = $state<Overlay[]>([]);
	let beat = $state(false);
	let overlayDragOver = $state(false);
	let expandedOverlayId = $state<string | null>(null);

	const BLEND_MODES = ['screen', 'normal', 'plus-lighter', 'multiply', 'overlay', 'hard-light'];

	// — Virtual list preset ————————————————————————————————
	const PRESET_ROW_H = 24;
	const PRESET_BUF = 5;
	let presetListEl: HTMLUListElement | undefined = $state();
	let presetScrollTop = $state(0);
	let presetContainerH = $state(500);
	let debouncedQuery = $state('');

	function onPresetScroll() {
		if (presetListEl) presetScrollTop = presetListEl.scrollTop;
	}

	// — Favoris + tags ————————————————————————————————————
	let favorites = $state<string[]>([]);
	let activeTag = $state<string>(''); // '' = tous, '★' = favoris, 'Auteur' = tag

	// — Derived ———————————————————————————————————————————
	let filteredPresets = $derived.by(() => {
		let list = searchPresets(presetList, debouncedQuery);
		if (activeTag === '★') return list.filter((p) => favorites.includes(p.name));
		if (activeTag) return list.filter((p) => p.category === activeTag);
		return list;
	});

	const vStart = $derived(Math.max(0, Math.floor(presetScrollTop / PRESET_ROW_H) - PRESET_BUF));
	const vEnd = $derived(Math.min(filteredPresets.length, Math.ceil((presetScrollTop + presetContainerH) / PRESET_ROW_H) + PRESET_BUF));
	let activePreset = $derived(activeDeck === 'A' ? presetA : presetB);
	let opacityA = $derived(1 - crossfader);
	let opacityB = $derived(crossfader);
	let presetIdxA = $derived(presetList.findIndex((p) => p.name === presetA));
	let presetIdxB = $derived(presetList.findIndex((p) => p.name === presetB));

	// — Sync crossfader to output window ——————————————————
	$effect(() => {
		sync?.sendCrossfader(crossfader);
	});

	// — VU meter polling ——————————————————————————————————
	$effect(() => {
		if (status !== 'running' || !audio) return;
		let rafId: number;
		const tick = () => {
			vuLevel = audio!.getLevels().rms;
			rafId = requestAnimationFrame(tick);
		};
		rafId = requestAnimationFrame(tick);
		return () => cancelAnimationFrame(rafId);
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
		localStorage.setItem('od-favorites', JSON.stringify(favorites));
		localStorage.setItem('od-quality', quality);
		localStorage.setItem('od-overlays', JSON.stringify(overlays));
	});

	// — Sync overlays vers output ——————————————————————————
	$effect(() => {
		sync?.sendOverlays(overlays);
	});

	// — Appliquer la qualité aux decks + sync output ———————
	$effect(() => {
		if (status !== 'running') return;
		const settings = getQualitySettings(quality);
		deckA?.applyQuality(settings);
		deckB?.applyQuality(settings);
		sync?.sendQuality(quality);
	});

	// — Debounce recherche preset ——————————————————————————
	$effect(() => {
		const q = searchQuery;
		const t = setTimeout(() => { debouncedQuery = q; }, 150);
		return () => clearTimeout(t);
	});

	// — ResizeObserver pour la liste preset ———————————————
	$effect(() => {
		if (!presetListEl) return;
		const ro = new ResizeObserver(([e]) => { presetContainerH = e.contentRect.height; });
		ro.observe(presetListEl);
		return () => ro.disconnect();
	});

	// — Reset scroll quand le filtre change ———————————————
	$effect(() => {
		filteredPresets; // track
		presetScrollTop = 0;
		if (presetListEl) presetListEl.scrollTop = 0;
	});

	// — FPS counter ————————————————————————————————————————
	$effect(() => {
		if (status !== 'running') return;
		let count = 0;
		let last = performance.now();
		let rafId: number;
		const tick = (t: number) => {
			count++;
			if (t - last >= 500) {
				fps = Math.round(count * 1000 / (t - last));
				count = 0;
				last = t;
			}
			rafId = requestAnimationFrame(tick);
		};
		rafId = requestAnimationFrame(tick);
		return () => { cancelAnimationFrame(rafId); fps = 0; };
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
			const savedFavs = localStorage.getItem('od-favorites');
			if (savedFavs) favorites = JSON.parse(savedFavs);
			const savedQuality = localStorage.getItem('od-quality');
			if (savedQuality === 'low' || savedQuality === 'medium' || savedQuality === 'high') quality = savedQuality;
			const savedOverlays = localStorage.getItem('od-overlays');
			if (savedOverlays) overlays = JSON.parse(savedOverlays);
		} catch {}
		_ready = true; // autorise les $effect de sauvegarde

		await initPresets();
		presetList = buildPresetList();
		if (presetList.length > 0) presetA = presetList[0].name;
		if (presetList.length > 1) presetB = presetList[1].name;
	});

	onDestroy(() => {
		playlistA?.destroy();
		playlistB?.destroy();
		deckA?.destroy();
		deckB?.destroy();
		audio?.destroy();
		sync?.destroy();
		midi?.destroy();
		beatDetector?.destroy();
		if (metronomeId !== null) clearInterval(metronomeId);
	});

	// — Actions ————————————————————————————————————————————
	async function startVisualizer() {
		if (!canvasA || !canvasB) return;
		try {
			const testCtx = canvasA.getContext('webgl2');
			if (!testCtx) {
				throw new Error(
					'WebGL2 unavailable. In LibreWolf/Firefox: go to about:config → set webgl.disabled = false.'
				);
			}

			audio = new AudioEngine();
			await audio.resume();

			const w = canvasA.clientWidth || 1280;
			const h = canvasA.clientHeight || 720;

			deckA = new Deck(canvasA, 'deck-a');
			deckB = new Deck(canvasB, 'deck-b');

			const q = getQualitySettings(quality);
			await deckA.init(audio.ctx, { width: w, height: h, ...q });
			await deckB.init(audio.ctx, { width: w, height: h, ...q });

			if (presetA) { const d = await loadPresetData(presetA); if (d) deckA.loadPreset(d, 0.0); }
			if (presetB) { const d = await loadPresetData(presetB); if (d) deckB.loadPreset(d, 0.0); }

			deckA.connectAudio(audio.gainNode);
			deckB.connectAudio(audio.gainNode);

			deckA.startRenderLoop();
			deckB.startRenderLoop();

			playlistA = new PlaylistEngine(playlistAItems, playlistMode, playlistIntervalSec * 1000, async (name) => {
				presetA = name;
				const d = await loadPresetData(name); if (d) deckA?.loadPreset(d, 2.0);
				sync?.sendPreset('A', name);
				playlistAPlaying = playlistA?.playing ?? false;
			});
			playlistB = new PlaylistEngine(playlistBItems, playlistMode, playlistIntervalSec * 1000, async (name) => {
				presetB = name;
				const d = await loadPresetData(name); if (d) deckB?.loadPreset(d, 2.0);
				sync?.sendPreset('B', name);
				playlistBPlaying = playlistB?.playing ?? false;
			});

			sync = new MainSync();
			sync.onOutputReady(() => {
				sync?.sendPreset('A', presetA);
				sync?.sendPreset('B', presetB);
				sync?.sendCrossfader(crossfader);
				sync?.sendQuality(quality);
				sync?.sendOverlays(overlays);
				if (currentDeviceId) sync?.sendSource(currentDeviceId);
			});

			beatDetector = new BeatDetector(audio.analyser);
			beatDetector.start(() => {
				detectedBpm = beatDetector?.bpm ?? 0;
				if (!manualBpm) onBeat();
			});

			status = 'running';
		} catch (e) {
			status = 'error';
			errorMsg = e instanceof Error ? e.message : String(e);
		}
	}

	async function captureSystemAudio() {
		if (!audio) return;
		sourceError = '';
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
			showDevicePicker = true;
		} catch (e) {
			sourceError = e instanceof Error ? e.message : String(e);
		}
	}

	async function connectDevice(device: MediaDeviceInfo) {
		if (!audio) return;
		sourceError = '';
		showDevicePicker = false;
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
		if (activeDeck === 'A') {
			presetA = name;
			deckA?.loadPreset(d, 2.0);
			sync?.sendPreset('A', name);
		} else {
			presetB = name;
			deckB?.loadPreset(d, 2.0);
			sync?.sendPreset('B', name);
		}
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
		sync?.sendBeat();

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
				const ov = makeOverlay(file.name.replace(/\.[^.]+$/, ''));
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
		if (expandedOverlayId === id) expandedOverlayId = null;
	}

	function updateOverlay(id: string, patch: Partial<Overlay>) {
		overlays = overlays.map(o => o.id === id ? { ...o, ...patch } : o);
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
		if (metronomeId !== null) clearInterval(metronomeId);
		metronomeId = setInterval(onBeat, avg);
	}

	function clearManualBpm() {
		manualBpm = 0;
		tapTimes = [];
		if (metronomeId !== null) { clearInterval(metronomeId); metronomeId = null; }
	}

	async function toggleMidi() {
		if (midiConnected) {
			midi?.destroy();
			midi = null;
			midiConnected = false;
			midiDeviceNames = [];
			learningAction = null;
			return;
		}
		try {
			midi = new MidiEngine();
			await midi.connect();
			midiConnected = true;
			midiDeviceNames = midi.deviceNames;
			midi.onMessage((msg) => {
				const key = triggerKey(msg);
				if (learningAction !== null) {
					// Mode apprentissage : enregistre le trigger
					if (msg.type === 'note_off') return; // ignore note-off pendant learn
					midiMappings = { ...midiMappings, [learningAction]: key };
					learningAction = null;
					return;
				}
				// Dispatcher
				for (const [action, mapped] of Object.entries(midiMappings) as [MidiAction, MidiTriggerKey][]) {
					if (mapped !== key) continue;
					if (msg.type === 'note_off') break; // actions déclenchées sur note_on ou cc
					applyMidiAction(action as MidiAction, msg.value);
					break;
				}
			});
		} catch (e) {
			midiConnected = false;
			sourceError = e instanceof Error ? e.message : String(e);
		}
	}

	function startLearn(action: MidiAction) {
		learningAction = learningAction === action ? null : action;
	}

	function clearMapping(action: MidiAction) {
		const { [action]: _, ...rest } = midiMappings;
		midiMappings = rest as Partial<Record<MidiAction, MidiTriggerKey>>;
	}

	function applyMidiAction(action: MidiAction, value: number) {
		if (status !== 'running') return;
		switch (action) {
			case 'crossfader':
				crossfader = value / 127;
				break;
			case 'preset-prev-a': {
				if (presetList.length === 0) break;
				const idx = ((presetIdxA <= 0 ? presetList.length : presetIdxA) - 1) % presetList.length;
				selectPresetForDeck('A', presetList[idx].name);
				break;
			}
			case 'preset-next-a': {
				if (presetList.length === 0) break;
				const idx = (presetIdxA + 1) % presetList.length;
				selectPresetForDeck('A', presetList[idx].name);
				break;
			}
			case 'preset-prev-b': {
				if (presetList.length === 0) break;
				const idx = ((presetIdxB <= 0 ? presetList.length : presetIdxB) - 1) % presetList.length;
				selectPresetForDeck('B', presetList[idx].name);
				break;
			}
			case 'preset-next-b': {
				if (presetList.length === 0) break;
				const idx = (presetIdxB + 1) % presetList.length;
				selectPresetForDeck('B', presetList[idx].name);
				break;
			}
			case 'playlist-toggle-a': togglePlaylist('A'); break;
			case 'playlist-toggle-b': togglePlaylist('B'); break;
			case 'playlist-prev-a': playlistPrev('A'); break;
			case 'playlist-next-a': playlistNext('A'); break;
			case 'playlist-prev-b': playlistPrev('B'); break;
			case 'playlist-next-b': playlistNext('B'); break;
		}
	}

	async function selectPresetForDeck(deck: 'A' | 'B', name: string) {
		const d = await loadPresetData(name);
		if (!d) return;
		if (deck === 'A') {
			presetA = name;
			deckA?.loadPreset(d, 2.0);
			sync?.sendPreset('A', name);
		} else {
			presetB = name;
			deckB?.loadPreset(d, 2.0);
			sync?.sendPreset('B', name);
		}
	}

	function toggleFavorite(name: string) {
		if (favorites.includes(name)) {
			favorites = favorites.filter((n) => n !== name);
		} else {
			favorites = [...favorites, name];
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
		window.open('/output', 'opendrop-output', 'width=1280,height=720');
		outputOpen = true;
		// Give the window ~800ms to init, then push current state
		setTimeout(() => {
			sync?.sendPreset('A', presetA);
			sync?.sendPreset('B', presetB);
			sync?.sendCrossfader(crossfader);
			if (currentDeviceId) sync?.sendSource(currentDeviceId);
		}, 800);
	}

	function onResize() {
		if (status !== 'running') return;
		if (canvasA) deckA?.resize(canvasA.clientWidth, canvasA.clientHeight);
		if (canvasB) deckB?.resize(canvasB.clientWidth, canvasB.clientHeight);
	}

	function onKeydown(e: KeyboardEvent) {
		// Ignorer si on tape dans un champ texte
		const tag = (e.target as HTMLElement).tagName;
		if (tag === 'INPUT' || tag === 'TEXTAREA') return;

		switch (e.key) {
			case 'ArrowLeft':
				e.preventDefault();
				crossfader = Math.max(0, parseFloat((crossfader - 0.05).toFixed(2)));
				break;
			case 'ArrowRight':
				e.preventDefault();
				crossfader = Math.min(1, parseFloat((crossfader + 0.05).toFixed(2)));
				break;
			case 'Tab':
				e.preventDefault();
				activeDeck = activeDeck === 'A' ? 'B' : 'A';
				break;
			case '[':
				e.preventDefault();
				applyMidiAction(activeDeck === 'A' ? 'preset-prev-a' : 'preset-prev-b', 127);
				break;
			case ']':
				e.preventDefault();
				applyMidiAction(activeDeck === 'A' ? 'preset-next-a' : 'preset-next-b', 127);
				break;
			case ' ':
				e.preventDefault();
				togglePlaylist(activeDeck);
				break;
			case 'n':
			case 'N':
				e.preventDefault();
				playlistNext(activeDeck);
				break;
			case 'p':
			case 'P':
				e.preventDefault();
				playlistPrev(activeDeck);
				break;
		}
	}
</script>

<svelte:window onresize={onResize} onkeydown={onKeydown} />
<audio bind:this={audioEl} style="display:none" crossorigin="anonymous"></audio>

<main>
	<div
		class="visualizer-wrap"
		class:drag-over={overlayDragOver}
		ondragover={onVisualizerDragOver}
		ondragleave={() => overlayDragOver = false}
		ondrop={onVisualizerDrop}
		role="region"
		aria-label="Visualizer"
	>
		<!-- Deck A — base layer -->
		<canvas
			bind:this={canvasA}
			class="deck-canvas"
			style="opacity:{opacityA}"
		></canvas>
		<!-- Deck B — top layer, screen blend pour rendu additif -->
		<canvas
			bind:this={canvasB}
			class="deck-canvas deck-canvas-b"
			style="opacity:{opacityB}"
		></canvas>
		<!-- Overlay sprites -->
		<OverlayLayer {overlays} {beat} />

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
		<!-- Audio source -->
		<div class="controls-section">
			<span class="label">Audio source</span>
			<div class="btn-row">
				<button class="btn-sm" class:active={sourceLabel === 'microphone'} onclick={connectMic} disabled={status !== 'running'}>Mic</button>
				<button class="btn-sm" onclick={openDevicePicker} disabled={status !== 'running'}>Pick device</button>
				<button class="btn-sm" class:active={sourceLabel === 'system audio'} onclick={captureSystemAudio} disabled={status !== 'running'} title="Capturer le son système">🔊 Audio système</button>
			</div>
			{#if showSystemAudioHelp}
				<div class="device-picker">
					{#if effectiveOS === 'darwin'}
						<span class="label">Audio système sur macOS</span>
						<p class="hint">Installer <strong>BlackHole</strong> (gratuit) :<br><code>brew install blackhole-2ch</code><br>Créer un Multi-Output Device dans Audio MIDI Setup,<br>puis <strong>Pick device</strong> → BlackHole.</p>
					{:else if effectiveOS === 'linux'}
						<span class="label">Audio système sur Linux</span>
						<p class="hint">Aucun périphérique monitor trouvé.<br>Utilisez <strong>Pick device</strong> → entrée se terminant par <code>.monitor</code> (sortie système).<br>Optionnel : <code>bash scripts/setup-audio.sh</code> pour un device nommé.</p>
					{:else}
						<span class="label">Audio système</span>
						<p class="hint">Dans Chrome/Edge : cliquer <strong>Audio système</strong> → choisir <strong>Écran entier</strong> → cocher <strong>"Partager l'audio système"</strong>.</p>
					{/if}
					<button class="btn-sm" onclick={() => showSystemAudioHelp = false}>OK</button>
				</div>
			{/if}
			<div class="file-row">
				<label class="btn-sm file-label">
					File
					<input type="file" accept="audio/*" onchange={onFileChange} style="display:none" />
				</label>
				{#if audioEl?.src && status === 'running'}
					<button class="btn-sm" class:active={sourceLabel === 'file'} onclick={connectFile}>▶ Play</button>
				{/if}
			</div>
			{#if sourceLabel !== 'none'}
				<span class="source-badge">▶ {sourceLabel}</span>
			{/if}
			{#if status === 'running'}
				<div class="vu-meter">
					<div class="vu-bar" style="width:{Math.round(vuLevel * 100)}%"></div>
				</div>
			{/if}
			{#if sourceError}
				<span class="source-error">⚠ {sourceError}</span>
			{/if}
			{#if showDevicePicker}
				<div class="device-picker">
					<span class="label">Select input device</span>
					{#each audioDevices as device}
						<button class="device-item" onclick={() => connectDevice(device)}>
							{device.label || `Device ${device.deviceId.slice(0, 8)}`}
						</button>
					{/each}
					<button class="btn-sm" onclick={() => showDevicePicker = false}>Cancel</button>
				</div>
			{/if}
		</div>

		<!-- Mixer -->
		<div class="controls-section">
			<span class="label">Mixer</span>
			<div class="deck-tabs">
				<button
					class="deck-tab"
					class:active={activeDeck === 'A'}
					onclick={() => activeDeck = 'A'}
				>
					<span class="deck-letter">A</span>
					<span class="deck-preset-name">{presetA.split(' - ')[0] || '—'}</span>
				</button>
				<button
					class="deck-tab"
					class:active={activeDeck === 'B'}
					onclick={() => activeDeck = 'B'}
				>
					<span class="deck-letter">B</span>
					<span class="deck-preset-name">{presetB.split(' - ')[0] || '—'}</span>
				</button>
			</div>
			<div class="crossfader-row">
				<span class="cf-label" class:bright={crossfader < 0.2}>A</span>
				<input
					class="crossfader"
					type="range"
					min="0"
					max="1"
					step="0.01"
					bind:value={crossfader}
				/>
				<span class="cf-label" class:bright={crossfader > 0.8}>B</span>
			</div>
		</div>

		<!-- Playlist -->
		<div class="controls-section pl-section">
			<div class="pl-header">
				<span class="label">Playlist</span>
				<div class="btn-row">
					<button class="btn-sm" class:active={playlistMode === 'sequential'} onclick={() => playlistMode = 'sequential'}>Seq</button>
					<button class="btn-sm" class:active={playlistMode === 'shuffle'} onclick={() => playlistMode = 'shuffle'}>Shuffle</button>
					<button class="btn-sm" onclick={exportPlaylists} title="Exporter les playlists">⬇</button>
					<label class="btn-sm file-label" title="Importer des playlists">⬆<input type="file" accept=".json" onchange={importPlaylists} style="display:none" /></label>
				</div>
			</div>
			<div class="crossfader-row">
				<span class="cf-label">⏱</span>
				<input class="crossfader" type="range" min="2" max="120" step="1" bind:value={playlistIntervalSec} />
				<span class="cf-label bright">{playlistIntervalSec}s</span>
			</div>

			<!-- Beat sync -->
			{#if status === 'running'}
				<div class="beat-sync-row">
					<span class="bpm-display" class:manual={manualBpm > 0}>♩ {manualBpm > 0 ? manualBpm : detectedBpm > 0 ? detectedBpm : '—'}</span>
					<button class="btn-sm tap-btn" onclick={tapTempo} title="Tap tempo">TAP</button>
					{#if manualBpm > 0}
						<button class="btn-sm" onclick={clearManualBpm} title="Clear manual BPM">✕</button>
					{/if}
					<select class="beats-select" bind:value={beatsPerChange}>
						<option value={4}>4</option>
						<option value={8}>8</option>
						<option value={16}>16</option>
						<option value={32}>32</option>
					</select>
					<button class="btn-sm pl-btn" class:active={beatSyncA} onclick={() => toggleBeatSync('A')} title="Beat-sync Deck A">A</button>
					<button class="btn-sm pl-btn" class:active={beatSyncB} onclick={() => toggleBeatSync('B')} title="Beat-sync Deck B">B</button>
					<button class="btn-sm pl-btn" class:active={autoXfade} onclick={() => { autoXfade = !autoXfade; autoXfadeCount = 0; }} title="Auto-cut crossfader on beat">⇄</button>
				</div>
			{/if}

			<!-- Deck A playlist -->
			<div class="pl-deck">
				<div class="pl-deck-header">
					<span class="pl-deck-label">A</span>
					<span class="label">{playlistAItems.length} preset{playlistAItems.length !== 1 ? 's' : ''}</span>
					<div class="pl-transport">
						<button class="btn-sm pl-btn" onclick={() => playlistPrev('A')} disabled={status !== 'running' || playlistAItems.length === 0}>⏮</button>
						<button class="btn-sm pl-btn" class:active={playlistAPlaying} onclick={() => togglePlaylist('A')} disabled={status !== 'running' || playlistAItems.length === 0}>
							{playlistAPlaying ? '⏹' : '▶'}
						</button>
						<button class="btn-sm pl-btn" onclick={() => playlistNext('A')} disabled={status !== 'running' || playlistAItems.length === 0}>⏭</button>
						<button class="btn-sm pl-btn lock-btn" class:locked={lockA} onclick={() => lockA = !lockA} title={lockA ? 'Unlock deck A' : 'Lock deck A'}>🔒</button>
					</div>
				</div>
				{#if playlistAItems.length > 0}
					<ul class="pl-items">
						{#each playlistAItems as name (name)}
							<li class="pl-item">
								<span class="pl-item-name" class:pl-active={name === presetA}>{name}</span>
								<button class="pl-remove" onclick={() => removeFromPlaylist('A', name)}>×</button>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="pl-empty">Use +A in the preset list below</p>
				{/if}
			</div>

			<!-- Deck B playlist -->
			<div class="pl-deck">
				<div class="pl-deck-header">
					<span class="pl-deck-label">B</span>
					<span class="label">{playlistBItems.length} preset{playlistBItems.length !== 1 ? 's' : ''}</span>
					<div class="pl-transport">
						<button class="btn-sm pl-btn" onclick={() => playlistPrev('B')} disabled={status !== 'running' || playlistBItems.length === 0}>⏮</button>
						<button class="btn-sm pl-btn" class:active={playlistBPlaying} onclick={() => togglePlaylist('B')} disabled={status !== 'running' || playlistBItems.length === 0}>
							{playlistBPlaying ? '⏹' : '▶'}
						</button>
						<button class="btn-sm pl-btn" onclick={() => playlistNext('B')} disabled={status !== 'running' || playlistBItems.length === 0}>⏭</button>
						<button class="btn-sm pl-btn lock-btn" class:locked={lockB} onclick={() => lockB = !lockB} title={lockB ? 'Unlock deck B' : 'Lock deck B'}>🔒</button>
					</div>
				</div>
				{#if playlistBItems.length > 0}
					<ul class="pl-items">
						{#each playlistBItems as name (name)}
							<li class="pl-item">
								<span class="pl-item-name" class:pl-active={name === presetB}>{name}</span>
								<button class="pl-remove" onclick={() => removeFromPlaylist('B', name)}>×</button>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="pl-empty">Use +B in the preset list below</p>
				{/if}
			</div>
		</div>

		<!-- Overlays -->
		<div class="controls-section">
			<div class="pl-header">
				<span class="label">Overlays ({overlays.length})</span>
				<label class="btn-sm file-label" title="Ajouter une image">
					+ Image
					<input type="file" accept="image/*" multiple onchange={onOverlayFilePick} style="display:none" />
				</label>
			</div>
			{#if overlays.length === 0}
				<p class="hint">Glisse une image sur le visualizer ou clique + Image</p>
			{/if}
			<ul class="overlay-list">
				{#each overlays as ov (ov.id)}
					<li class="overlay-item">
						<div class="overlay-row">
							<button class="overlay-name" onclick={() => expandedOverlayId = expandedOverlayId === ov.id ? null : ov.id}>
								{ov.name}
							</button>
							<button class="btn-sm pl-btn" class:active={ov.beatReactive} onclick={() => updateOverlay(ov.id, { beatReactive: !ov.beatReactive })} title="Beat reactive">♩</button>
							<button class="pl-remove" onclick={() => removeOverlay(ov.id)} title="Supprimer">×</button>
						</div>
						{#if expandedOverlayId === ov.id}
							<div class="overlay-controls">
								<label class="ov-label">Opacity
									<input type="range" min="0" max="1" step="0.01" value={ov.opacity} oninput={(e) => updateOverlay(ov.id, { opacity: +(e.target as HTMLInputElement).value })} />
								</label>
								<label class="ov-label">Scale
									<input type="range" min="0.05" max="4" step="0.05" value={ov.scale} oninput={(e) => updateOverlay(ov.id, { scale: +(e.target as HTMLInputElement).value })} />
								</label>
								<label class="ov-label">X
									<input type="range" min="0" max="1" step="0.01" value={ov.x} oninput={(e) => updateOverlay(ov.id, { x: +(e.target as HTMLInputElement).value })} />
								</label>
								<label class="ov-label">Y
									<input type="range" min="0" max="1" step="0.01" value={ov.y} oninput={(e) => updateOverlay(ov.id, { y: +(e.target as HTMLInputElement).value })} />
								</label>
								<label class="ov-label">Rotation
									<input type="range" min="-180" max="180" step="1" value={ov.rotation} oninput={(e) => updateOverlay(ov.id, { rotation: +(e.target as HTMLInputElement).value })} />
								</label>
								<label class="ov-label">Blend
									<select class="ov-select" value={ov.blendMode} onchange={(e) => updateOverlay(ov.id, { blendMode: (e.target as HTMLSelectElement).value })}>
										{#each BLEND_MODES as mode}
											<option value={mode}>{mode}</option>
										{/each}
									</select>
								</label>
							</div>
						{/if}
					</li>
				{/each}
			</ul>
		</div>

		<!-- Qualité rendu -->
		<div class="controls-section">
			<div class="pl-header">
				<span class="label">Qualité rendu</span>
				{#if status === 'running' && fps > 0}
					<span class="label" style="color:#7af">{fps} fps</span>
				{/if}
			</div>
			<div class="btn-row">
				<button class="btn-sm" class:active={quality === 'low'} onclick={() => quality = 'low'} disabled={status !== 'running'}>Low</button>
				<button class="btn-sm" class:active={quality === 'medium'} onclick={() => quality = 'medium'} disabled={status !== 'running'}>Med</button>
				<button class="btn-sm" class:active={quality === 'high'} onclick={() => quality = 'high'} disabled={status !== 'running'}>High</button>
			</div>
		</div>

		<!-- Output -->
		<div class="controls-section">
			<button class="btn-output" onclick={openOutput} disabled={status !== 'running'}>
				⎋ Open output window
			</button>
			{#if outputOpen}
				<span class="label" style="color:#7af">Output window open — use as OBS Browser Source</span>
			{/if}
		</div>

		<!-- MIDI -->
		<div class="controls-section">
			<div class="pl-header">
				<span class="label">MIDI</span>
				{#if midiSupported}
					<button class="btn-sm" class:active={midiConnected} onclick={toggleMidi}>
						{midiConnected ? 'Déconnecter' : 'Connecter'}
					</button>
				{:else}
					<span style="font-size:10px;color:#f87">Chromium only</span>
				{/if}
			</div>
			{#if midiConnected}
				<span class="source-badge">▶ {midiDeviceNames.length > 0 ? midiDeviceNames.join(', ') : 'aucun périphérique'}</span>
				{#if learningAction !== null}
					<span style="font-size:11px;color:#fa7">Bouge un knob/bouton sur ton contrôleur…</span>
				{/if}
				<div class="midi-list">
					{#each MIDI_ACTIONS as action}
						{@const mapped = midiMappings[action]}
						<div class="midi-row">
							<span class="midi-label">{MIDI_LABELS[action]}</span>
							<span class="midi-binding" class:midi-learning={learningAction === action}>
								{mapped ? formatTrigger(mapped) : '—'}
							</span>
							<button class="btn-sm pl-btn" class:active={learningAction === action}
								onclick={() => startLearn(action)}>
								{learningAction === action ? '…' : 'Learn'}
							</button>
							{#if mapped}
								<button class="pl-remove" onclick={() => clearMapping(action)}>×</button>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		</div>

		<!-- Preset browser -->
		<div class="controls-section preset-browser">
			<span class="label">Presets → Deck {activeDeck} ({filteredPresets.length}/{presetList.length})</span>
			<input
				class="search-input"
				type="search"
				placeholder="Search presets…"
				bind:value={searchQuery}
			/>
			<!-- Filtre favoris -->
			<div class="tag-chips">
				<button class="tag-chip" class:tag-active={activeTag === '★'} onclick={() => activeTag = activeTag === '★' ? '' : '★'}>★ Favorites</button>
			</div>
			<ul class="preset-list" bind:this={presetListEl} onscroll={onPresetScroll}>
				<li style="height:{vStart * PRESET_ROW_H}px" aria-hidden="true"></li>
				{#each filteredPresets.slice(vStart, vEnd) as p (p.name)}
					{@const isFav = favorites.includes(p.name)}
					<li class="preset-row">
						<button
							class="fav-btn"
							class:fav-on={isFav}
							onclick={() => toggleFavorite(p.name)}
							title={isFav ? 'Retirer des favoris' : 'Ajouter aux favoris'}
						>★</button>
						<button
							class="preset-item"
							class:active={p.name === activePreset}
							onclick={() => selectPreset(p.name)}
						>
							{p.name}
						</button>
						<button class="pl-add" class:in-list={playlistAItems.includes(p.name)} onclick={() => addToPlaylist('A', p.name)} title="Add to playlist A">A</button>
						<button class="pl-add" class:in-list={playlistBItems.includes(p.name)} onclick={() => addToPlaylist('B', p.name)} title="Add to playlist B">B</button>
					</li>
				{/each}
				<li style="height:{Math.max(0, filteredPresets.length - vEnd) * PRESET_ROW_H}px" aria-hidden="true"></li>
			</ul>
		</div>
	</aside>
</main>

<style>
	:global(*, *::before, *::after) {
		box-sizing: border-box;
		margin: 0;
		padding: 0;
	}

	/* ── City Pop Tokyo Night ── */
	:global(*, *::before, *::after) { box-sizing: border-box; margin: 0; padding: 0; }

	:global(html, body) {
		width: 100%; height: 100%;
		background: #07071a;
		color: #ddddf5;
		font-family: 'Inter', system-ui, sans-serif;
		font-size: 13px;
		overflow: hidden;
	}

	/* Scrollbars */
	:global(::-webkit-scrollbar) { width: 4px; }
	:global(::-webkit-scrollbar-track) { background: transparent; }
	:global(::-webkit-scrollbar-thumb) { background: #2a2a5a; border-radius: 2px; }
	:global(::-webkit-scrollbar-thumb:hover) { background: #ff2d78; }

	main { display: flex; width: 100vw; height: 100vh; overflow: hidden; }

	.visualizer-wrap { flex: 1; position: relative; background: #000; min-width: 0; isolation: isolate; }

	.deck-canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
	.deck-canvas-b { mix-blend-mode: screen; }

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
		background: linear-gradient(135deg, #ff2d78 0%, #00e5ff 100%);
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

	.controls-section {
		padding: 0.7rem 0.75rem;
		border-bottom: 1px solid #131330;
		display: flex; flex-direction: column; gap: 0.4rem;
	}

	.preset-browser { flex: 1 0 180px; overflow: hidden; }

	.label {
		font-size: 10px; text-transform: uppercase; letter-spacing: 0.1em;
		color: #444470; font-weight: 600;
	}

	.btn-row, .file-row { display: flex; gap: 0.4rem; }

	.source-badge { font-size: 11px; color: #00e5ff; }
	.source-error { font-size: 11px; color: #ff6090; word-break: break-word; }

	/* VU meter — pink→cyan gradient */
	.vu-meter {
		height: 5px; background: #111130; border-radius: 3px; overflow: hidden;
		border: 1px solid #1a1a40;
	}

	.vu-bar {
		height: 100%;
		background: linear-gradient(90deg, #ff2d78, #b44fff 50%, #00e5ff);
		border-radius: 3px;
		transition: width 50ms linear;
		box-shadow: 0 0 8px rgba(255, 45, 120, 0.5);
	}

	.device-picker {
		display: flex; flex-direction: column; gap: 0.2rem;
		margin-top: 0.2rem; padding: 0.4rem;
		background: #0e0e28; border: 1px solid #232350; border-radius: 6px;
	}

	.device-item {
		display: block; width: 100%; text-align: left;
		background: none; border: none; color: #aaaacc;
		padding: 0.3rem 0.4rem; cursor: pointer; font-size: 11px;
		border-radius: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
		transition: all 0.1s;
	}

	.device-item:hover { background: #191940; color: #fff; }

	.hint { margin: 0.2rem 0; font-size: 11px; color: #aaaacc; line-height: 1.5; }
	.hint code { background: #191940; padding: 0.1rem 0.3rem; border-radius: 3px; font-size: 10px; }
	.hint strong { color: #e0e0ff; }

	.tap-btn { font-weight: 700; letter-spacing: 0.05em; }
	.bpm-display.manual { color: #b44fff; text-shadow: 0 0 8px rgba(180,79,255,0.5); }
	.lock-btn { opacity: 0.35; }
	.lock-btn.locked { opacity: 1; color: #ff2d78; }

	/* ── Mixer ── */
	.deck-tabs { display: flex; gap: 0.4rem; }

	.deck-tab {
		flex: 1; display: flex; flex-direction: column; align-items: center; gap: 0.2rem;
		padding: 0.45rem 0.4rem;
		background: #0e0e26; border: 1px solid #1e1e48;
		border-radius: 6px; cursor: pointer; color: #44447a;
		transition: all 0.15s;
	}

	.deck-tab:hover { border-color: #ff2d78; color: #cc88aa; }

	.deck-tab.active {
		background: #1a0a22;
		border-color: #ff2d78;
		color: #ff2d78;
		box-shadow: 0 0 12px rgba(255, 45, 120, 0.3), inset 0 0 8px rgba(255, 45, 120, 0.06);
	}

	.deck-letter { font-size: 16px; font-weight: 800; }

	.deck-preset-name {
		font-size: 10px; white-space: nowrap; overflow: hidden;
		text-overflow: ellipsis; max-width: 90px; color: inherit;
	}

	.crossfader-row { display: flex; align-items: center; gap: 0.4rem; }

	.cf-label {
		font-size: 11px; font-weight: 700; color: #33335a;
		width: 12px; text-align: center; transition: color 0.15s;
	}

	.cf-label.bright { color: #ff2d78; text-shadow: 0 0 8px rgba(255,45,120,0.8); }

	.crossfader { flex: 1; accent-color: #ff2d78; cursor: pointer; }

	/* ── Preset browser ── */
	.search-input {
		width: 100%;
		background: #0e0e26; border: 1px solid #1e1e48;
		border-radius: 6px; color: #ddddf5;
		padding: 0.35rem 0.5rem; font-size: 12px; outline: none;
		transition: border-color 0.15s;
	}

	.search-input:focus { border-color: #00e5ff; box-shadow: 0 0 0 2px rgba(0,229,255,0.1); }
	.search-input::placeholder { color: #33335a; }

	.preset-list { flex: 1; overflow-y: auto; list-style: none; }

	.preset-item {
		display: block; width: 100%; text-align: left;
		background: none; border: none; color: #8888bb;
		padding: 0.15rem 0.4rem; cursor: pointer; font-size: 11px;
		border-radius: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
		transition: all 0.1s;
	}

	.preset-item:hover { background: #111130; color: #eeeeff; }

	.preset-item.active {
		background: #0c0c2a;
		color: #00e5ff;
		text-shadow: 0 0 8px rgba(0,229,255,0.5);
	}

	/* ── Buttons ── */
	.btn-primary {
		background: linear-gradient(135deg, #ff2d78, #b44fff);
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

	.btn-secondary:hover { border-color: #ff2d78; color: #fff; }

	.btn-sm {
		background: #0e0e26; color: #7777aa;
		border: 1px solid #1e1e48; border-radius: 5px;
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: all 0.12s;
	}

	.btn-sm:hover:not(:disabled) { background: #141436; color: #ddddf5; border-color: #3a3a6a; }

	.btn-sm.active {
		background: #1a0822; border-color: #ff2d78; color: #ff2d78;
		box-shadow: 0 0 8px rgba(255,45,120,0.25);
	}

	.btn-sm:disabled { opacity: 0.3; cursor: not-allowed; }

	.file-label { display: inline-block; cursor: pointer; }

	.pl-btn { padding: 0.22rem 0.4rem; font-size: 11px; }

	.pl-section { gap: 0.5rem; }

	.pl-header { display: flex; align-items: center; justify-content: space-between; }

	.pl-deck {
		background: #0a0a1e; border: 1px solid #161640;
		border-radius: 6px; padding: 0.4rem;
		display: flex; flex-direction: column; gap: 0.3rem;
	}

	.pl-deck-header { display: flex; align-items: center; gap: 0.4rem; }

	.pl-deck-label {
		font-size: 13px; font-weight: 800; width: 14px;
		color: #ff2d78; text-shadow: 0 0 8px rgba(255,45,120,0.7);
	}

	.pl-transport { display: flex; gap: 0.25rem; margin-left: auto; }

	.pl-items {
		list-style: none; max-height: 80px; overflow-y: auto;
		display: flex; flex-direction: column; gap: 1px;
	}

	.pl-item { display: flex; align-items: center; gap: 0.25rem; }

	.pl-item-name {
		flex: 1; font-size: 11px; color: #666690;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.pl-item-name.pl-active { color: #00e5ff; text-shadow: 0 0 6px rgba(0,229,255,0.5); }

	.pl-remove {
		background: none; border: none; color: #33335a;
		cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; flex-shrink: 0;
		transition: color 0.1s;
	}

	.pl-remove:hover { color: #ff2d78; }

	.pl-empty { font-size: 10px; color: #2a2a50; font-style: italic; }

	/* Tag chips */
	.tag-chips {
		display: flex; flex-wrap: wrap; gap: 3px;
		max-height: 52px; overflow-y: auto;
		scrollbar-width: thin; scrollbar-color: #2a2a5a transparent;
	}

	.tag-chip {
		background: #0e0e26; border: 1px solid #1e1e48;
		border-radius: 10px; color: #44447a;
		font-size: 10px; padding: 2px 7px; cursor: pointer;
		white-space: nowrap; transition: all 0.12s;
	}

	.tag-chip:hover { border-color: #b44fff; color: #b44fff; }

	.tag-chip.tag-active {
		background: #1a0830; border-color: #b44fff; color: #b44fff;
		box-shadow: 0 0 8px rgba(180,79,255,0.35);
	}

	/* Bouton favori ★ */
	.fav-btn {
		background: none; border: none; color: #22224a;
		font-size: 11px; cursor: pointer; padding: 0 2px;
		flex-shrink: 0; transition: all 0.1s; line-height: 1;
	}

	.fav-btn:hover { color: #ffcc00; }
	.fav-btn.fav-on { color: #ffcc00; text-shadow: 0 0 8px rgba(255,204,0,0.7); }

	/* Preset rows +A +B */
	.preset-row { display: flex; align-items: center; gap: 2px; height: 24px; box-sizing: border-box; }
	.preset-row .preset-item { flex: 1; min-width: 0; }

	.pl-add {
		flex-shrink: 0; background: #0e0e26;
		border: 1px solid #1e1e48; border-radius: 3px;
		color: #2a2a52; font-size: 10px; font-weight: 800;
		padding: 2px 5px; cursor: pointer; line-height: 1; transition: all 0.1s;
	}

	.pl-add:hover { border-color: #ff2d78; color: #ff2d78; background: #150a1a; }
	.pl-add.in-list { color: #00e5ff; border-color: #005566; background: #04101a; }

	/* MIDI */
	.midi-list {
		display: flex; flex-direction: column; gap: 2px;
		max-height: 160px; overflow-y: auto;
	}

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.midi-label { font-size: 10px; color: #44447a; width: 80px; flex-shrink: 0; white-space: nowrap; }

	.midi-binding {
		flex: 1; font-size: 10px; color: #33335a; font-family: 'Courier New', monospace;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.midi-binding.midi-learning { color: #ff8c00; animation: blink 0.6s step-end infinite; }

	@keyframes blink { 50% { opacity: 0; } }

	/* Beat sync */
	.beat-sync-row {
		display: flex; align-items: center; gap: 0.4rem;
		padding: 0.3rem 0.5rem;
		background: #08081e; border: 1px solid #141440;
		border-radius: 6px;
	}

	.bpm-display {
		font-size: 12px; font-weight: 700; color: #b44fff;
		text-shadow: 0 0 10px rgba(180,79,255,0.6);
		min-width: 48px; font-family: 'Courier New', monospace;
		flex-shrink: 0;
	}

	.beats-select {
		background: #0e0e26; color: #7777aa;
		border: 1px solid #1e1e48; border-radius: 5px;
		padding: 0.2rem 0.3rem; font-size: 10px; cursor: pointer;
		-webkit-appearance: none; appearance: none;
		flex: 1; min-width: 0;
	}

	.beats-select:focus { outline: none; border-color: #b44fff; }

	/* Output button */
	.btn-output {
		width: 100%;
		background: linear-gradient(135deg, rgba(0,229,255,0.08), rgba(180,79,255,0.08));
		color: #00e5ff; border: 1px solid #004455;
		border-radius: 6px; padding: 0.45rem; font-size: 12px; font-weight: 600;
		cursor: pointer; letter-spacing: 0.03em;
		transition: all 0.15s;
		box-shadow: 0 0 12px rgba(0,229,255,0.1);
	}

	.btn-output:hover:not(:disabled) {
		background: linear-gradient(135deg, rgba(0,229,255,0.14), rgba(180,79,255,0.14));
		box-shadow: 0 0 20px rgba(0,229,255,0.25);
		border-color: #00e5ff;
	}

	.btn-output:disabled { opacity: 0.3; cursor: not-allowed; }

	/* Visualizer drag-over */
	.visualizer-wrap.drag-over::after {
		content: '';
		position: absolute;
		inset: 0;
		border: 2px dashed #b44fff;
		border-radius: 6px;
		pointer-events: none;
		z-index: 20;
	}

	/* Overlay panel */
	.overlay-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 2px;
		max-height: 200px;
		overflow-y: auto;
		scrollbar-width: thin;
		scrollbar-color: #2a2a5a transparent;
	}

	.overlay-item {
		background: #0a0a1e;
		border: 1px solid #161640;
		border-radius: 5px;
		overflow: hidden;
	}

	.overlay-row {
		display: flex;
		align-items: center;
		gap: 3px;
		padding: 2px 4px;
	}

	.overlay-name {
		flex: 1;
		background: none;
		border: none;
		color: #6666aa;
		font-size: 11px;
		cursor: pointer;
		text-align: left;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		padding: 2px 0;
		transition: color 0.1s;
	}
	.overlay-name:hover { color: #b44fff; }

	.overlay-controls {
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 4px 6px 5px;
		border-top: 1px solid #161640;
		background: #06061a;
	}

	.ov-label {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		font-size: 10px;
		color: #44447a;
	}

	.ov-label input[type="range"] {
		flex: 1;
		height: 3px;
		accent-color: #b44fff;
		cursor: pointer;
	}

	.ov-select {
		flex: 1;
		background: #0e0e26;
		color: #7777aa;
		border: 1px solid #1e1e48;
		border-radius: 4px;
		font-size: 10px;
		padding: 1px 3px;
		cursor: pointer;
	}
	.ov-select:focus { outline: none; border-color: #b44fff; }
</style>
