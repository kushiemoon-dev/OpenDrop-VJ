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
	// Cache object URLs by id to avoid recreating on each beat-cut
	const urlCache = new Map<string, string>();

	$effect(() => {
		if (!clip) { resolvedSrc = ''; return; }
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

{#if resolvedSrc}
	<video
		bind:this={videoEl}
		src={resolvedSrc}
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
