<script lang="ts">
	import { onMount, onDestroy } from 'svelte'
	import type { PresetMeta } from '$lib/presets/index.js'

	interface Props {
		preset: PresetMeta
		slug: string
		thumbUrl?: string
		isFav: boolean
		inA: boolean
		inB: boolean
		isSelected: boolean
		onLoad: () => void
		onToggleFav: () => void
		onAddA: () => void
		onAddB: () => void
		onVisible: () => void
		onHidden: () => void
	}

	let { preset, slug, thumbUrl, isFav, inA, inB, isSelected, onLoad, onToggleFav, onAddA, onAddB, onVisible, onHidden }: Props = $props()

	onMount(() => onVisible())
	onDestroy(() => onHidden())
</script>

<div
	class="ptile"
	class:ptile--selected={isSelected}
	role="button"
	tabindex="0"
	onclick={onLoad}
	onkeydown={(e) => e.key === 'Enter' && onLoad()}
	title={preset.name}
>
	<!-- Vignette ou placeholder dégradé -->
	<div class="ptile__thumb">
		{#if thumbUrl}
			<img src={thumbUrl} alt={preset.name} class="ptile__img" />
		{:else}
			<div class="ptile__placeholder"></div>
		{/if}
	</div>

	<!-- Nom tronqué -->
	<div class="ptile__name" title={preset.name}>{preset.name.split('/').pop() ?? preset.name}</div>

	<!-- Étoile favori (coin haut-droit, stopPropagation) -->
	<button
		class="ptile__fav"
		class:ptile__fav--on={isFav}
		onclick={(e) => {
			e.stopPropagation()
			onToggleFav()
		}}
		type="button"
		aria-label={isFav ? 'Retirer des favoris' : 'Ajouter aux favoris'}
	>
		★
	</button>

	<!-- Pied A/B -->
	<div class="ptile__footer" onclick={(e) => e.stopPropagation()}>
		<button class="ptile__pl" class:ptile__pl--in={inA} onclick={onAddA} type="button">
			A
		</button>
		<button class="ptile__pl" class:ptile__pl--in={inB} onclick={onAddB} type="button">
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

	.ptile--selected {
		border-color: var(--accent);
		box-shadow: 0 0 10px var(--accent-glow);
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

	/* Placeholder dégradé + animation pulse quand pas de miniature */
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

	/* Étoile favori — positionnée en coin haut-droit */
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

	/* Pied A/B */
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
