<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { DeckManager } from '$lib/engine/deck-manager.js';
	import { AudioEngine } from '$lib/engine/audio.js';
	import { OutputSync, type ColorParams, DEFAULT_COLOR_PARAMS, colorParamsToFilter } from '$lib/engine/sync.js';
	import { initPresets, buildPresetList, loadPresetData } from '$lib/presets/index.js';
	import { getQualitySettings, DEFAULT_TIER, DEFAULT_PERF, type QualityTier, type InvisibleMode } from '$lib/engine/quality.js';
	import { type Overlay } from '$lib/engine/overlay.js';
	import OverlayLayer from '$lib/components/OverlayLayer.svelte';
	import VideoLayer from '$lib/components/VideoLayer.svelte';
	import type { ClipRef } from '$lib/engine/video-store.js';

	let canvas0: HTMLCanvasElement | undefined = $state();
	let canvas1: HTMLCanvasElement | undefined = $state();
	let canvas2: HTMLCanvasElement | undefined = $state();
	let canvas3: HTMLCanvasElement | undefined = $state();

	// Opacités par slot — remplace crossfader + opacityA/B
	let slotOpacities = $state<[number, number, number, number]>([1, 0, 0, 0]);
	// Couleurs par slot
	let slotColors = $state<[ColorParams, ColorParams, ColorParams, ColorParams]>([
		{ ...DEFAULT_COLOR_PARAMS },
		{ ...DEFAULT_COLOR_PARAMS },
		{ ...DEFAULT_COLOR_PARAMS },
		{ ...DEFAULT_COLOR_PARAMS },
	]);
	const slotFilters = $derived([
		colorParamsToFilter(slotColors[0]),
		colorParamsToFilter(slotColors[1]),
		colorParamsToFilter(slotColors[2]),
		colorParamsToFilter(slotColors[3]),
	]);

	let overlays = $state<Overlay[]>([]);
	let beat = $state(false);
	let beatTimer: ReturnType<typeof setTimeout> | null = null;

	// — Strobe ———————————————————————————————————————————
	let strobeOn = $state(false);
	let strobeRate = $state(1);
	let strobeIntensity = $state(0.8);
	let strobeColor = $state('#ffffff');
	let strobeFlash = $state(false);
	let _lastStrobeVal = 0;
	let _strobeBpm = 0;
	let _strobePhase = 0;
	let _strobeRafId: number | null = null;
	let _strobeLastTs: number | null = null;

	function _strobeStart() {
		if (_strobeRafId !== null) return;
		_strobeLastTs = null;
		const tick = (ts: number) => {
			if (_strobeLastTs !== null && _strobeBpm > 0) {
				const dt = Math.min((ts - _strobeLastTs) / 1000, 0.1);
				_strobePhase += dt * _strobeBpm / 60;
				_strobePhase %= 1;
				const p = (_strobePhase * strobeRate) % 1;
				const val = p < 0.5 ? 1 : 0;
				if (val === 1 && _lastStrobeVal === 0) {
					strobeFlash = true;
					setTimeout(() => { strobeFlash = false; }, 50);
				}
				_lastStrobeVal = val;
			}
			_strobeLastTs = ts;
			_strobeRafId = requestAnimationFrame(tick);
		};
		_strobeRafId = requestAnimationFrame(tick);
	}

	function _strobeStop() {
		if (_strobeRafId !== null) { cancelAnimationFrame(_strobeRafId); _strobeRafId = null; }
	}

	let status = $state<'initializing' | 'ready' | 'error'>('initializing');
	let errorMsg = $state('');

	// — Video loops ———————————————————————————————————————
	let videoEnabled = $state(false);
	let videoClip = $state<ClipRef | null>(null);
	let videoOpacity = $state(0.6);
	let videoPlaybackRate = $state(1);
	let vrFlash = $state(true);
	let vrHue = $state(false);

	let manager: DeckManager | null = null;
	let audio: AudioEngine | null = null;
	let sync: OutputSync | null = null;
	let helloTimer: ReturnType<typeof setInterval> | null = null;
	let loopbackUnlisten: (() => void) | null = null;
	let audioFrameUnlisten: (() => void) | null = null;
	let audioAcquired = $state(false);

	// — Performance reçue du main ——————————————————————————
	let targetFps = $state(DEFAULT_PERF.targetFps);
	let invisibleMode = $state<InvisibleMode>(DEFAULT_PERF.invisibleMode);
	let invisibleFps = $state(DEFAULT_PERF.invisibleFps);

	// Throttle des slots invisibles selon le perf mode
	$effect(() => {
		const ops = slotOpacities;
		const mode = invisibleMode;
		const target = targetFps;
		const eco = invisibleFps;
		if (!manager) return;
		for (let i = 0; i < 4; i++) {
			const visible = ops[i] > 0.001 || mode === 'off';
			const fps = visible ? target : (mode === 'eco' ? eco : 0);
			manager.setSlotTargetFps(i, fps);
		}
	});

	onMount(async () => {
		try {
			audio = new AudioEngine();

			manager = new DeckManager();
			manager.attachCanvas(0, canvas0!);
			manager.attachCanvas(1, canvas1!);
			manager.attachCanvas(2, canvas2!);
			manager.attachCanvas(3, canvas3!);

			const q = getQualitySettings(DEFAULT_TIER);

			// Démarrer slots 0 et 1 par défaut (équivalent A/B compat)
			await manager.start(0, audio.ctx, audio.analyser, q, null);
			await manager.start(1, audio.ctx, audio.analyser, q, null);
			manager.setTargetFps(targetFps);

			// Charger les presets par défaut pour éviter l'écran noir au démarrage
			await initPresets();
			const list = buildPresetList();
			if (list[0]) { const d = await loadPresetData(list[0].name); if (d) manager.loadPreset(0, d, 0.0); }
			if (list[1]) { const d = await loadPresetData(list[1].name); if (d) manager.loadPreset(1, d, 0.0); }

			// PCM frames streamés depuis le renderer principal (Electron-only)
			const eAPI = window.electronAPI;
			if (eAPI?.onAudioFrame) {
				audioFrameUnlisten = eAPI.onAudioFrame(async (frame) => {
					if (!audioAcquired) {
						audioAcquired = true;
						await audio!.resume();
						await audio!.connectLoopbackPcm();
						manager?.connectAudio(audio!.analyser);
					}
					audio!.pushCapturePcm(frame);
				});
			}

			sync = new OutputSync();
			let gotState = false;
			sync.listen(async (msg) => {
				if (msg.type === 'preset') {
					// Compat backward : deck A → slot 0, deck B → slot 1
					gotState = true;
					const slot = msg.deck === 'A' ? 0 : 1;
					const preset = await loadPresetData(msg.name);
					if (!preset) return;
					await ensureSlot(slot, q);
					manager?.loadPreset(slot, preset, 2.0);
				} else if (msg.type === 'preset-slot') {
					gotState = true;
					const preset = await loadPresetData(msg.name);
					if (!preset) return;
					await ensureSlot(msg.slot, q);
					manager?.loadPreset(msg.slot, preset, 2.0);
				} else if (msg.type === 'crossfader') {
					// Compat backward : crossfader → opacités slot 0/1
					gotState = true;
					const cf = msg.value;
					slotOpacities = [1 - cf, cf, slotOpacities[2], slotOpacities[3]];
				} else if (msg.type === 'deckbus') {
					gotState = true;
					slotOpacities = msg.opacities;
				} else if (msg.type === 'quality') {
					const settings = getQualitySettings(msg.tier as QualityTier);
					manager?.applyQuality(settings);
				} else if (msg.type === 'perf') {
					targetFps = msg.targetFps;
					invisibleMode = msg.invisibleMode as InvisibleMode;
					invisibleFps = msg.invisibleFps;
				} else if (msg.type === 'overlays') {
					overlays = msg.list;
				} else if (msg.type === 'video') {
					gotState = true;
					videoEnabled = msg.enabled;
					videoClip = msg.clip;
					videoOpacity = msg.opacity;
					videoPlaybackRate = msg.playbackRate;
					vrFlash = msg.flashOn;
					vrHue = msg.hueOn;
				} else if (msg.type === 'beat') {
					beat = true;
					if (beatTimer !== null) clearTimeout(beatTimer);
					beatTimer = setTimeout(() => { beat = false; beatTimer = null; }, 80);
					_strobeBpm = msg.bpm;
					_strobePhase = 0;
					_lastStrobeVal = 0;
				} else if (msg.type === 'color') {
					const slot = msg.deck === 'A' ? 0 : 1;
					const next: [ColorParams, ColorParams, ColorParams, ColorParams] = [...slotColors];
					next[slot] = msg.params;
					slotColors = next;
				} else if (msg.type === 'strobe') {
					strobeOn = msg.on;
					strobeRate = msg.rate;
					strobeIntensity = msg.intensity;
					strobeColor = msg.color;
					if (msg.on) _strobeStart(); else _strobeStop();
				} else if (msg.type === 'source') {
					if (audioAcquired) return;
					loopbackUnlisten?.();
					loopbackUnlisten = null;
					try {
						await audio!.resume();
						await audio!.connectDevice(msg.deviceId);
						manager?.connectAudio(audio!.analyser);
					} catch { /* device capture failed */ }
				} else if (msg.type === 'loopback') {
					if (audioAcquired) return;
					loopbackUnlisten?.();
					loopbackUnlisten = null;
					try {
						await audio!.resume();
						await audio!.connectLoopbackPcm();
						manager?.connectAudio(audio!.analyser);
						const eAPI = window.electronAPI;
						if (eAPI) {
							loopbackUnlisten = eAPI.onLoopbackData((data) => {
								audio?.pushLoopbackPcm(data);
							});
						}
					} catch { /* loopback may not be available */ }
				}
			});

			sync.sendHello();
			let tries = 0;
			helloTimer = setInterval(() => {
				if (gotState || tries++ > 12) { clearInterval(helloTimer!); helloTimer = null; return; }
				sync!.sendHello();
			}, 700);

			status = 'ready';
		} catch (e) {
			status = 'error';
			errorMsg = e instanceof Error ? e.message : String(e);
		}
	});

	async function ensureSlot(slot: number, q: ReturnType<typeof getQualitySettings>) {
		if (!manager || !audio) return;
		if (!manager.isRunning(slot)) {
			await manager.start(slot, audio.ctx, audio.analyser, q, null);
		}
	}

	onDestroy(() => {
		loopbackUnlisten?.();
		audioFrameUnlisten?.();
		if (beatTimer !== null) clearTimeout(beatTimer);
		if (helloTimer !== null) clearInterval(helloTimer);
		_strobeStop();
		manager?.destroyAll();
		audio?.destroy();
		sync?.destroy();
	});

	function onResize() {
		if (!canvas0) return;
		const w = canvas0.clientWidth || window.innerWidth;
		const h = canvas0.clientHeight || window.innerHeight;
		for (let i = 0; i < 4; i++) manager?.resize(i, w, h);
	}
</script>

<svelte:window onresize={onResize} />

<div class="output">
	<VideoLayer clip={videoEnabled ? videoClip : null} opacity={videoOpacity} {beat} playbackRate={videoPlaybackRate} flashOn={vrFlash} hueOn={vrHue} />
	<canvas bind:this={canvas0} class="layer" style="opacity:{slotOpacities[0]}; mix-blend-mode:{videoEnabled ? 'screen' : 'normal'}; filter:{slotFilters[0]}"></canvas>
	<canvas bind:this={canvas1} class="layer layer-blend" style="opacity:{slotOpacities[1]}; filter:{slotFilters[1]}"></canvas>
	<canvas bind:this={canvas2} class="layer layer-blend" style="opacity:{slotOpacities[2]}; filter:{slotFilters[2]}"></canvas>
	<canvas bind:this={canvas3} class="layer layer-blend" style="opacity:{slotOpacities[3]}; filter:{slotFilters[3]}"></canvas>
	<OverlayLayer {overlays} {beat} />
	{#if strobeOn && strobeFlash}
		<div class="strobe-flash" style="background:{strobeColor};opacity:{strobeIntensity}"></div>
	{/if}

	{#if status === 'initializing'}
		<div class="notice">Initializing…</div>
	{/if}
	{#if status === 'error'}
		<div class="notice error">⚠ {errorMsg}</div>
	{/if}
</div>

<style>
	:global(*, *::before, *::after) { box-sizing: border-box; margin: 0; padding: 0; }
	:global(html, body) { width: 100%; height: 100%; background: #000; overflow: hidden; }

	.output {
		width: 100vw;
		height: 100vh;
		position: relative;
		background: #000;
		isolation: isolate;
	}

	.layer {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		display: block;
	}
	.layer-blend { mix-blend-mode: screen; }

	.notice {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		color: #555;
		font-family: system-ui, sans-serif;
		font-size: 14px;
		z-index: 10;
	}

	.notice.error { color: #f87; }
	.strobe-flash { position: absolute; inset: 0; z-index: 200; pointer-events: none; }
</style>
