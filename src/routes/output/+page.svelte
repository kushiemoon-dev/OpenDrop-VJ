<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Deck } from '$lib/engine/deck.js';
	import { AudioEngine } from '$lib/engine/audio.js';
	import { OutputSync } from '$lib/engine/sync.js';
	import { initPresets, buildPresetList, loadPresetData } from '$lib/presets/index.js';
	import { getQualitySettings, DEFAULT_TIER, type QualityTier } from '$lib/engine/quality.js';
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
	let ndiActive = $state(false);
	let ndiError = $state('');
	let v4l2Active = $state(false);
	let v4l2Error = $state('');

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
	let audioAcquired = false;

	const opacityA = $derived(1 - crossfader);
	const opacityB = $derived(crossfader);

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
						// device may not be available in output window context
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

	const eAPI = typeof window !== 'undefined' ? window.electronAPI : undefined;

	async function toggleV4l2() {
		v4l2Error = '';
		if (v4l2Active) {
			await eAPI?.v4l2Stop();
			v4l2Active = false;
		} else {
			const res = await eAPI?.v4l2Start();
			if (res?.ok) {
				v4l2Active = true;
			} else {
				v4l2Error = res?.error ?? 'Erreur v4l2 inconnue.';
			}
		}
	}

	async function toggleNdi() {
		ndiError = '';
		if (ndiActive) {
			await eAPI?.ndiStop();
			ndiActive = false;
		} else {
			const w = window.innerWidth;
			const h = window.innerHeight;
			const res = await eAPI?.ndiStart('OpenDrop VJ', w, h);
			if (res?.ok) {
				ndiActive = true;
			} else {
				ndiError = res?.error ?? 'NDI SDK non trouvé — installez le NDI SDK depuis ndi.video puis relancez.';
			}
		}
	}
</script>

<svelte:window onresize={onResize} />

<div class="output">
	<VideoLayer clip={videoEnabled ? videoClip : null} opacity={videoOpacity} {beat} playbackRate={videoPlaybackRate} flashOn={vrFlash} hueOn={vrHue} />
	<canvas bind:this={canvasA} class="layer" style="opacity:{opacityA}; mix-blend-mode:{videoEnabled ? 'screen' : 'normal'}"></canvas>
	<canvas bind:this={canvasB} class="layer layer-b" style="opacity:{opacityB}"></canvas>
	<OverlayLayer {overlays} {beat} />

	{#if eAPI}
		<button class="v4l2-btn" class:v4l2-on={v4l2Active} onclick={toggleV4l2} title={v4l2Active ? 'Stop V4L2' : 'Start V4L2 (webcam virtuelle)'}>
			V4L2 {v4l2Active ? '●' : '○'}
		</button>
		<button class="ndi-btn" class:ndi-on={ndiActive} onclick={toggleNdi} title={ndiActive ? 'Stop NDI' : 'Start NDI'}>
			NDI {ndiActive ? '●' : '○'}
		</button>
	{/if}
	{#if v4l2Error}
		<div class="notice error" style="font-size:11px;padding:0.5rem 1rem;">{v4l2Error}</div>
	{/if}
	{#if ndiError}
		<div class="notice error" style="font-size:11px;padding:0.5rem 1rem;">{ndiError}</div>
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

	.ndi-btn, .v4l2-btn {
		position: absolute;
		bottom: 12px;
		z-index: 30;
		background: rgba(0,0,0,0.6);
		border: 1px solid #333;
		border-radius: 6px;
		color: #555;
		font-size: 11px;
		font-family: 'Courier New', monospace;
		font-weight: 700;
		letter-spacing: 0.05em;
		padding: 4px 10px;
		cursor: pointer;
		transition: all 0.15s;
	}

	.ndi-btn { right: 12px; }
	.v4l2-btn { right: 80px; }

	.ndi-btn:hover { border-color: #ff6600; color: #ff6600; }
	.ndi-btn.ndi-on { border-color: #ff6600; color: #ff6600; background: rgba(255,102,0,0.12); }

	.v4l2-btn:hover { border-color: #00c8ff; color: #00c8ff; }
	.v4l2-btn.v4l2-on { border-color: #00c8ff; color: #00c8ff; background: rgba(0,200,255,0.12); }
</style>
