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

	// spin/drift = animations CSS pures (pas de RAF).
	// ponytail: amplitude/vitesse approximées pour un rendu VJ convaincant,
	// pas une simulation physique — suffisant tant que non mesuré insuffisant.
	function spinStyle(spin: number): string {
		if (!spin) return '';
		const dur = 360 / Math.abs(spin);
		return `animation: od-spin ${dur}s linear infinite ${spin < 0 ? 'reverse' : 'normal'};`;
	}

	function driftStyle(driftX: number, driftY: number): string {
		if (!driftX && !driftY) return '';
		const speed = Math.max(Math.abs(driftX), Math.abs(driftY), 0.05);
		const dur = 1 / speed;
		return `--drift-x: ${(driftX * 60).toFixed(0)}px; --drift-y: ${(driftY * 60).toFixed(0)}px; animation: od-drift ${dur}s ease-in-out infinite alternate;`;
	}
</script>

{#each overlays as ov (ov.id)}
	{@const pulse = beat && ov.beatReactive}
	<div class="overlay-anchor" style="left:{ov.x * 100}%; top:{ov.y * 100}%; {spinStyle(ov.spin)}">
		<div class="overlay-drift" style={driftStyle(ov.driftX, ov.driftY)}>
			{#if ov.kind === 'text'}
				<div
					class="overlay-text"
					class:beat-pulse={pulse}
					style="
						transform: translate(-50%, -50%) scale({pulse ? ov.scale * ov.beatScale : ov.scale});
						font-size: {ov.fontSize}vh;
						font-family: var(--od-font-{ov.fontFamily});
						color: {ov.color};
						opacity: {ov.opacity};
						mix-blend-mode: {ov.blendMode};
					"
				>{ov.text}</div>
			{:else if srcs[ov.id]}
				{#if ov.video}
					<video
						src={srcs[ov.id]}
						class="overlay-media"
						class:beat-pulse={pulse}
						autoplay
						loop
						muted
						playsinline
						style="
							transform: translate(-50%, -50%) scale({pulse ? ov.scale * ov.beatScale : ov.scale}) rotate({ov.rotation}deg);
							opacity: {ov.opacity};
							mix-blend-mode: {ov.blendMode};
						"
					></video>
				{:else}
					<img
						src={srcs[ov.id]}
						alt={ov.name}
						class="overlay-media"
						class:beat-pulse={pulse}
						style="
							transform: translate(-50%, -50%) scale({pulse ? ov.scale * ov.beatScale : ov.scale}) rotate({ov.rotation}deg);
							opacity: {ov.opacity};
							mix-blend-mode: {ov.blendMode};
						"
					/>
				{/if}
			{/if}
		</div>
	</div>
{/each}

<style>
	.overlay-anchor {
		position: absolute;
		transform-origin: 0 0;
	}

	.overlay-drift {
		position: relative;
		transform-origin: 0 0;
	}

	.overlay-media {
		position: absolute;
		pointer-events: none;
		/* vw/vh plutôt que % : le containing block ici est le wrapper spin/drift
		   (taille 0, nécessaire pour que la rotation pivote autour du point d'ancrage),
		   pas le visualizer — % résoudrait à 0. Les 2 usages (stage, output) sont plein écran. */
		max-width: 80vw;
		max-height: 80vh;
		transition: transform 80ms ease-out;
		user-select: none;
	}

	.overlay-text {
		position: absolute;
		pointer-events: none;
		white-space: pre-wrap;
		text-align: center;
		max-width: 90vw;
		max-height: 80vh;
		overflow: hidden;
		transform-origin: 0 0;
		transition: transform 80ms ease-out;
		user-select: none;
		font-weight: 700;
		text-shadow: 0 0.15vh 0.4vh rgba(0, 0, 0, 0.85), 0 0 1vh rgba(0, 0, 0, 0.6);
		--od-font-sans: system-ui, -apple-system, 'Segoe UI', sans-serif;
		--od-font-serif: Georgia, 'Times New Roman', serif;
		--od-font-mono: 'Courier New', Consolas, monospace;
		--od-font-impact: Impact, 'Arial Black', sans-serif;
		--od-font-comic: 'Comic Sans MS', 'Comic Sans', cursive;
	}

	@keyframes od-spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	@keyframes od-drift {
		from { transform: translate(0, 0); }
		to { transform: translate(var(--drift-x, 0), var(--drift-y, 0)); }
	}
</style>
