<script lang="ts">
	import type { Overlay } from '$lib/engine/overlay.js';
	import { loadAsset } from '$lib/engine/overlay.js';

	interface Props {
		overlays: Overlay[];
		beat: boolean;
	}

	let { overlays, beat }: Props = $props();

	// id → data URL, chargé depuis IndexedDB
	let srcs = $state<Record<string, string>>({});

	$effect(() => {
		for (const ov of overlays) {
			if (!(ov.id in srcs)) {
				loadAsset(ov.id).then((url) => {
					if (url) srcs = { ...srcs, [ov.id]: url };
				});
			}
		}
	});
</script>

{#each overlays as ov (ov.id)}
	{#if srcs[ov.id]}
		{@const pulse = beat && ov.beatReactive}
		<img
			src={srcs[ov.id]}
			alt={ov.name}
			class="overlay-img"
			class:beat-pulse={pulse}
			style="
				left: {ov.x * 100}%;
				top: {ov.y * 100}%;
				transform: translate(-50%, -50%) scale({pulse ? ov.scale * ov.beatScale : ov.scale}) rotate({ov.rotation}deg);
				opacity: {ov.opacity};
				mix-blend-mode: {ov.blendMode};
			"
		/>
	{/if}
{/each}

<style>
	.overlay-img {
		position: absolute;
		pointer-events: none;
		max-width: 80%;
		max-height: 80%;
		transition: transform 80ms ease-out;
		user-select: none;
	}
</style>
