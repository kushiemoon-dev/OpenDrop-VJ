<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Deck } from '$lib/engine/deck.js';
	import { AudioEngine } from '$lib/engine/audio.js';
	import { OutputSync } from '$lib/engine/sync.js';
	import { buildPresetList, loadPresetData } from '$lib/presets/index.js';

	let canvasA: HTMLCanvasElement | undefined = $state();
	let canvasB: HTMLCanvasElement | undefined = $state();
	let crossfader = $state(0);
	let status = $state<'initializing' | 'ready' | 'error'>('initializing');
	let errorMsg = $state('');

	let deckA: Deck | null = null;
	let deckB: Deck | null = null;
	let audio: AudioEngine | null = null;
	let sync: OutputSync | null = null;
	let helloTimer: ReturnType<typeof setInterval> | null = null;

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
			await deckA.init(audio.ctx, { width: w, height: h });
			await deckB.init(audio.ctx, { width: w, height: h });

			deckA.startRenderLoop();
			deckB.startRenderLoop();

			// Charger les presets par défaut (mêmes indices 0/1 que le main) pour ne jamais
			// être noir si le handshake de sync tarde ou est absent.
			const list = buildPresetList();
			if (list[0]) { const d = await loadPresetData(list[0].name); if (d) deckA.loadPreset(d, 0.0); }
			if (list[1]) { const d = await loadPresetData(list[1].name); if (d) deckB.loadPreset(d, 0.0); }

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
				} else if (msg.type === 'source') {
					try {
						await audio!.resume();
						await audio!.connectDevice(msg.deviceId);
						deckA?.connectAudio(audio!.gainNode);
						deckB?.connectAudio(audio!.gainNode);
					} catch {
						// device may not be available in output window context
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
	<canvas bind:this={canvasA} class="layer" style="opacity:{opacityA}"></canvas>
	<canvas bind:this={canvasB} class="layer" style="opacity:{opacityB}"></canvas>

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
	}

	.layer {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		display: block;
	}

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
