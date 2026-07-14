<script lang="ts">
	import type { ClipRef } from '$lib/engine/video-store.js';
	import { loadVideo } from '$lib/engine/video-store.js';

	interface Props {
		clip: ClipRef | null;
		opacity: number;
		beat: boolean;
		playbackRate: number;
		flashOn: boolean;
		hueOn: boolean;
	}

	let { clip, opacity, beat, playbackRate, flashOn, hueOn }: Props = $props();

	let videoEl: HTMLVideoElement | undefined = $state();
	let resolvedSrc = $state('');
	// Holds whichever external MediaStream is currently active — a live camera
	// (getUserMedia) or an NDI source (canvas.captureStream of received frames).
	let externalStream: MediaStream | null = $state(null);
	// Cache object URLs by id to avoid recreating on each beat-cut
	const urlCache = new Map<string, string>();
	const isStreamKind = (k: ClipRef['kind'] | undefined) => k === 'live' || k === 'ndi';

	$effect(() => {
		if (!clip || clip.kind === 'live' || clip.kind === 'ndi') { resolvedSrc = ''; return; }
		if (clip.kind === 'builtin') { resolvedSrc = clip.src; return; }
		const id = clip.id;
		if (urlCache.has(id)) { resolvedSrc = urlCache.get(id)!; return; }
		loadVideo(id).then((blob) => {
			if (!blob) return;
			const url = URL.createObjectURL(blob);
			urlCache.set(id, url);
			resolvedSrc = url;
		});
	});

	// Live camera has no URL to resolve — acquire a MediaStream by deviceId instead.
	// Re-acquires whenever `clip` switches to a different live device; the cleanup
	// stops the previous device's tracks (camera light off) on every switch/unmount.
	$effect(() => {
		if (clip?.kind !== 'live') return;
		const deviceId = clip.deviceId;
		let cancelled = false;
		navigator.mediaDevices.getUserMedia({ video: { deviceId: { exact: deviceId } } })
			.then((stream) => {
				if (cancelled) { stream.getTracks().forEach((t) => t.stop()); return; }
				externalStream = stream;
			})
			.catch(() => { /* permission denied or device gone — layer just stays blank */ });
		return () => {
			cancelled = true;
			externalStream?.getTracks().forEach((t) => t.stop());
			externalStream = null;
		};
	});

	// NDI source: this component only DRAWS frames — starting/stopping the
	// receiver in the main process is owned by +page.svelte (a single shared
	// receiver, unlike a camera device which each window can open on its own).
	// Frames are broadcast to every window, so the output window's VideoLayer
	// renders the same feed passively, purely from this listener.
	$effect(() => {
		if (clip?.kind !== 'ndi') return;
		const eAPI = window.electronAPI;
		if (!eAPI?.onNdiFrame) return; // web-only session — no Electron IPC, NDI unavailable
		let canvas: HTMLCanvasElement | null = null;
		let ctx: CanvasRenderingContext2D | null = null;
		const unlisten = eAPI.onNdiFrame((frame) => {
			try {
				if (!canvas || canvas.width !== frame.width || canvas.height !== frame.height) {
					// A resolution change orphans the previous captureStream's track —
					// this $effect only re-runs on unmount/clip switch, not on this
					// in-callback reassignment, so it must be stopped explicitly here.
					externalStream?.getTracks().forEach((t) => t.stop());
					canvas = document.createElement('canvas');
					canvas.width = frame.width;
					canvas.height = frame.height;
					ctx = canvas.getContext('2d');
					externalStream = canvas.captureStream(30);
				}
				const { width, height, lineStrideBytes, data } = frame;
				// Fast path when rows are tightly packed (the common case); otherwise
				// strip row padding so ImageData isn't built from a skewed buffer.
				const pixels = lineStrideBytes === width * 4
					? new Uint8ClampedArray(data.buffer, data.byteOffset, data.byteLength)
					: (() => {
						const out = new Uint8ClampedArray(width * height * 4);
						for (let y = 0; y < height; y++) {
							out.set(data.subarray(y * lineStrideBytes, y * lineStrideBytes + width * 4), y * width * 4);
						}
						return out;
					})();
				ctx?.putImageData(new ImageData(pixels, width, height), 0, 0);
			} catch { /* malformed frame (e.g. size mismatch) — drop it, keep the stream alive */ }
		});
		return () => {
			unlisten();
			externalStream?.getTracks().forEach((t) => t.stop());
			canvas = null;
			ctx = null;
			externalStream = null;
		};
	});

	// Bind/clear srcObject on the <video> element — an external stream takes
	// priority over `src`; switching away must null srcObject explicitly
	// (setting `src` alone does not clear a previously-assigned srcObject).
	$effect(() => {
		if (videoEl) videoEl.srcObject = isStreamKind(clip?.kind) ? externalStream : null;
	});

	$effect(() => {
		if (videoEl) videoEl.playbackRate = Math.max(0.5, Math.min(2.5, playbackRate));
	});

	$effect(() => {
		return () => {
			for (const url of urlCache.values()) URL.revokeObjectURL(url);
			urlCache.clear();
		};
	});

	const scale = $derived(beat && flashOn ? 1.04 : 1);
	const brightness = $derived(beat && flashOn ? 1.4 : 1);
	const hueRotate = $derived(beat && hueOn ? 35 : 0);
	const filterStr = $derived(`brightness(${brightness}) hue-rotate(${hueRotate}deg)`);
</script>

{#if resolvedSrc || isStreamKind(clip?.kind)}
	<video
		bind:this={videoEl}
		src={isStreamKind(clip?.kind) ? undefined : resolvedSrc}
		class="video-layer"
		style="opacity:{opacity}; transform:scale({scale}); filter:{filterStr};"
		loop
		muted
		autoplay
		playsinline
		onloadedmetadata={() => videoEl?.play().catch(() => {})}
	></video>
{/if}

<style>
	.video-layer {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		pointer-events: none;
		transition: transform 80ms ease-out, filter 80ms ease-out;
	}
</style>
