<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Deck } from '$lib/engine/deck.js';
	import { AudioEngine } from '$lib/engine/audio.js';
	import { OutputSync } from '$lib/engine/sync.js';
	import { initPresets, buildPresetList, loadPresetData } from '$lib/presets/index.js';
	import { getQualitySettings, DEFAULT_TIER, DEFAULT_PERF, type QualityTier, type InvisibleMode } from '$lib/engine/quality.js';
	import { type Overlay } from '$lib/engine/overlay.js';
	import OverlayLayer from '$lib/components/OverlayLayer.svelte';
	import VideoLayer from '$lib/components/VideoLayer.svelte';
	import type { ClipRef } from '$lib/engine/video-store.js';

	let canvasA: HTMLCanvasElement | undefined = $state();
	let canvasB: HTMLCanvasElement | undefined = $state();
	let crossfader = $state(0);
	let overlays = $state<Overlay[]>([]);
	let beat = $state(false);
	let beatTimer: ReturnType<typeof setTimeout> | null = null;
	let status = $state<'initializing' | 'ready' | 'error'>('initializing');
	let errorMsg = $state('');
	// — Video loops ———————————————————————————————————————
	let videoEnabled = $state(false);
	let videoClip = $state<ClipRef | null>(null);
	let videoOpacity = $state(0.6);
	let videoPlaybackRate = $state(1);
	let vrFlash = $state(true);
	let vrHue = $state(false);

	let deckA: Deck | null = null;
	let deckB: Deck | null = null;
	let audio: AudioEngine | null = null;
	let sync: OutputSync | null = null;
	let helloTimer: ReturnType<typeof setInterval> | null = null;
	let loopbackUnlisten: (() => void) | null = null;
	let audioFrameUnlisten: (() => void) | null = null;
	// Set to true once PCM frames from the main window are flowing — prevents the
	// output from also trying to re-capture the same device independently (fragile).
	let audioAcquired = $state(false);
	// — Performance reçue du main ——————————————————————————
	let targetFps = $state(DEFAULT_PERF.targetFps);
	let invisibleMode = $state<InvisibleMode>(DEFAULT_PERF.invisibleMode);
	let invisibleFps = $state(DEFAULT_PERF.invisibleFps);

	const opacityA = $derived(1 - crossfader);
	const opacityB = $derived(crossfader);

	// — Throttle des decks invisibles selon le perf mode ——
	$effect(() => {
		const oa = opacityA;    // lire avant appels non-réactifs
		const ob = opacityB;
		const mode = invisibleMode;
		const target = targetFps;
		const eco = invisibleFps;

		const wantA = (oa > 0.001 || mode === 'off') ? target : (mode === 'eco' ? eco : 0);
		const wantB = (ob > 0.001 || mode === 'off') ? target : (mode === 'eco' ? eco : 0);

		if (mode === 'pause') {
			// En mode pause, les decks sont gérés via pause/resume — pas setTargetFps
			// On laisse ça minimal car pause n'est pas le mode par défaut (éco l'est)
			if (oa <= 0.001) deckA?.setTargetFps(eco);   // throttle lourd au lieu de pause
			if (ob <= 0.001) deckB?.setTargetFps(eco);   // (Deck.pause() nécessiterait un resume piloté)
		} else {
			deckA?.setTargetFps(wantA);
			deckB?.setTargetFps(wantB);
		}
	});

	onMount(async () => {
		try {
			// Minimal AudioContext — no source, just needed by Butterchurn
			audio = new AudioEngine();

			const w = canvasA!.clientWidth || window.innerWidth;
			const h = canvasA!.clientHeight || window.innerHeight;

			deckA = new Deck(canvasA!, 'out-a');
			deckB = new Deck(canvasB!, 'out-b');
			const q = getQualitySettings(DEFAULT_TIER);
			await deckA.init(audio.ctx, { width: w, height: h, ...q });
			await deckB.init(audio.ctx, { width: w, height: h, ...q });

			deckA.startRenderLoop();
			deckB.startRenderLoop();
			// Apply initial FPS cap from perf settings (effect fires before decks are ready)
			deckA.setTargetFps(targetFps);
			deckB.setTargetFps(targetFps);

			// Charger les presets par défaut (mêmes indices 0/1 que le main) pour ne jamais
			// être noir si le handshake de sync tarde ou est absent.
			await initPresets();
			const list = buildPresetList();
			if (list[0]) { const d = await loadPresetData(list[0].name); if (d) deckA.loadPreset(d, 0.0); }
			if (list[1]) { const d = await loadPresetData(list[1].name); if (d) deckB.loadPreset(d, 0.0); }

			// Subscribe to PCM frames streamed from the main renderer (Electron-only).
			// On the first frame, initialize the loopback worklet so Butterchurn reacts
			// to the same audio signal as the main window — regardless of source type.
			const eAPI = window.electronAPI;
			if (eAPI?.onAudioFrame) {
				audioFrameUnlisten = eAPI.onAudioFrame(async (frame) => {
					if (!audioAcquired) {
						audioAcquired = true;
						await audio!.resume();
						await audio!.connectLoopbackPcm();
						deckA?.connectAudio(audio!.analyser);
						deckB?.connectAudio(audio!.analyser);
					}
					audio!.pushCapturePcm(frame);
				});
			}

			sync = new OutputSync();
			let gotState = false;
			sync.listen(async (msg) => {
				if (msg.type === 'preset') {
					gotState = true;
					const preset = await loadPresetData(msg.name);
					if (!preset) return;
					if (msg.deck === 'A') deckA?.loadPreset(preset, 2.0);
					else deckB?.loadPreset(preset, 2.0);
				} else if (msg.type === 'crossfader') {
					gotState = true;
					crossfader = msg.value;
				} else if (msg.type === 'quality') {
					const settings = getQualitySettings(msg.tier as QualityTier);
					deckA?.applyQuality(settings);
					deckB?.applyQuality(settings);
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
				} else if (msg.type === 'source') {
					// If PCM frames are already flowing from the main window, skip the
					// independent re-capture (fragile on Linux — same device may be exclusive).
					if (audioAcquired) return;
					loopbackUnlisten?.();
					loopbackUnlisten = null;
					try {
						await audio!.resume();
						await audio!.connectDevice(msg.deviceId);
						deckA?.connectAudio(audio!.analyser);
						deckB?.connectAudio(audio!.analyser);
					} catch {
						// device capture failed — audio stays silent
					}
				} else if (msg.type === 'loopback') {
					// Same guard — PCM streaming takes priority over IPC loopback.
					if (audioAcquired) return;
					loopbackUnlisten?.();
					loopbackUnlisten = null;
					try {
						await audio!.resume();
						await audio!.connectLoopbackPcm();
						deckA?.connectAudio(audio!.analyser);
						deckB?.connectAudio(audio!.analyser);
						const eAPI = window.electronAPI;
						if (eAPI) {
							loopbackUnlisten = eAPI.onLoopbackData((data) => {
								audio?.pushLoopbackPcm(data);
							});
						}
					} catch {
						// loopback may not be available
					}
				}
			});

			// Émettre hello après listen() pour ne rater aucune réponse.
			// Retry jusqu'à réception du premier état du main (~12 × 700 ms ≈ 8 s max).
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

	onDestroy(() => {
		loopbackUnlisten?.();
		audioFrameUnlisten?.();
		if (beatTimer !== null) clearTimeout(beatTimer);
		if (helloTimer !== null) clearInterval(helloTimer);
		deckA?.destroy();
		deckB?.destroy();
		audio?.destroy();
		sync?.destroy();
	});

	function onResize() {
		if (!canvasA || !canvasB) return;
		deckA?.resize(canvasA.clientWidth, canvasA.clientHeight);
		deckB?.resize(canvasB.clientWidth, canvasB.clientHeight);
	}

</script>

<svelte:window onresize={onResize} />

<div class="output">
	<VideoLayer clip={videoEnabled ? videoClip : null} opacity={videoOpacity} {beat} playbackRate={videoPlaybackRate} flashOn={vrFlash} hueOn={vrHue} />
	<canvas bind:this={canvasA} class="layer" style="opacity:{opacityA}; mix-blend-mode:{videoEnabled ? 'screen' : 'normal'}"></canvas>
	<canvas bind:this={canvasB} class="layer layer-b" style="opacity:{opacityB}"></canvas>
	<OverlayLayer {overlays} {beat} />

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
	.layer-b { mix-blend-mode: screen; }

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
</style>
