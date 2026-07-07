<script lang="ts">
	import { onMount, onDestroy } from 'svelte'
	import type { PresetMeta } from '$lib/presets/index.js'
	import { FAV_COLORS } from '$lib/presets/favorites.js'

	interface Props {
		preset: PresetMeta
		slug: string
		thumbUrl?: string
		favColor: number
		inA: boolean
		inB: boolean
		onLoad: () => void
		onSetFavColor: (color: number) => void
		onAddA: () => void
		onAddB: () => void
		onVisible: () => void
		onHidden: () => void
	}

	let { preset, slug, thumbUrl, favColor, inA, inB, onLoad, onSetFavColor, onAddA, onAddB, onVisible, onHidden }: Props = $props()

	onMount(() => onVisible())
	onDestroy(() => onHidden())
</script>

<div
	class="ptile"
	role="button"
	tabindex="0"
	onclick={onLoad}
	onkeydown={(e) => e.key === 'Enter' && onLoad()}
	title={preset.name}
>
	<!-- Thumbnail or gradient placeholder -->
	<div class="ptile__thumb">
		{#if thumbUrl}
			<img src={thumbUrl} alt={preset.name} class="ptile__img" />
		{:else}
			<div class="ptile__placeholder"></div>
		{/if}
	</div>

	<!-- Truncated name -->
	<div class="ptile__name" title={preset.name}>{preset.name.split('/').pop() ?? preset.name}</div>

	<!-- Favorite swatch (top-right corner) — click cycles 0→1→2→3→4→5→0 -->
	<button
		class="ptile__fav"
		class:ptile__fav--on={favColor > 0}
		style:color={FAV_COLORS[favColor] || undefined}
		onclick={(e) => {
			e.stopPropagation()
			onSetFavColor((favColor + 1) % 6)
		}}
		type="button"
		aria-label={favColor > 0 ? 'Change favorite color' : 'Add to favorites'}
	>
		★
	</button>

	<!-- A/B footer -->
	<div class="ptile__footer">
		<button class="ptile__pl" class:ptile__pl--in={inA} onclick={(e) => { e.stopPropagation(); onAddA(); }} type="button">
			A
		</button>
		<button class="ptile__pl" class:ptile__pl--in={inB} onclick={(e) => { e.stopPropagation(); onAddB(); }} type="button">
			B
		</button>
	</div>
</div>

<style>
	.ptile {
		position: relative;
		display: flex;
		flex-direction: column;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		overflow: hidden;
		cursor: pointer;
		transition: border-color var(--t-fast), transform var(--t-fast);
		user-select: none;
	}

	.ptile:hover {
		border-color: var(--accent);
		transform: translateY(-1px);
	}

	.ptile__thumb {
		width: 100%;
		aspect-ratio: 16 / 9;
		background: var(--bg-base);
		overflow: hidden;
		position: relative;
	}

	.ptile__img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	/* Gradient placeholder + pulse animation while there is no thumbnail */
	.ptile__placeholder {
		position: absolute;
		inset: 0;
		background: linear-gradient(135deg, var(--bg-elevated), var(--bg-surface));
		animation: ptile-pulse 1.8s ease-in-out infinite;
	}

	@keyframes ptile-pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.4;
		}
	}

	.ptile__name {
		padding: 2px 6px;
		font-size: 9px;
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-height: 16px;
	}

	/* Favorite star — positioned in the top-right corner */
	.ptile__fav {
		position: absolute;
		top: 2px;
		right: 2px;
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 10px;
		padding: 1px;
		line-height: 1;
		opacity: 0;
		transition: opacity var(--t-fast), color var(--t-fast);
		z-index: 1;
	}

	.ptile:hover .ptile__fav {
		opacity: 1;
	}

	.ptile__fav--on {
		color: var(--accent);
		opacity: 1 !important;
	}

	/* A/B footer */
	.ptile__footer {
		display: flex;
		gap: 3px;
		padding: 2px 4px;
		border-top: 1px solid var(--border-subtle);
	}

	.ptile__pl {
		flex: 1;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text-muted);
		font-size: 8px;
		font-weight: 700;
		padding: 1px 0;
		cursor: pointer;
		transition: all var(--t-fast);
	}

	.ptile__pl:hover {
		border-color: var(--accent);
		color: var(--accent);
	}

	.ptile__pl--in {
		border-color: var(--accent);
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 10%, transparent);
	}
</style>
