<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Deck } from '$lib/engine/deck.js';
	import { AudioEngine } from '$lib/engine/audio.js';
	import { OutputSync } from '$lib/engine/sync.js';
	import { loadPresetData } from '$lib/presets/index.js';

	let canvasA: HTMLCanvasElement | undefined = $state();
	let canvasB: HTMLCanvasElement | undefined = $state();
	let crossfader = $state(0);
	let status = $state<'initializing' | 'ready' | 'error'>('initializing');
	let errorMsg = $state('');

	let deckA: Deck | null = null;
	let deckB: Deck | null = null;
	let audio: AudioEngine | null = null;
	let sync: OutputSync | null = null;

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

			sync = new OutputSync();
			sync.listen(async (msg) => {
				if (msg.type === 'preset') {
					const preset = await loadPresetData(msg.name);
					if (!preset) return;
					if (msg.deck === 'A') deckA?.loadPreset(preset, 2.0);
					else deckB?.loadPreset(preset, 2.0);
				} else if (msg.type === 'crossfader') {
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

			status = 'ready';
		} catch (e) {
			status = 'error';
			errorMsg = e instanceof Error ? e.message : String(e);
		}
	});

	onDestroy(() => {
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
