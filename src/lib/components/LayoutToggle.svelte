<script lang="ts">
	interface Props {
		layout: 'stage' | 'mixer'
		onToggle: (l: 'stage' | 'mixer') => void
	}
	let { layout, onToggle }: Props = $props()
</script>

<div class="layout-toggle" role="group" aria-label="Choose layout">
	<button
		class="toggle-btn"
		class:active={layout === 'mixer'}
		onclick={() => onToggle('mixer')}
		type="button"
	>
		⊞ Mixer
	</button>
	<button
		class="toggle-btn"
		class:active={layout === 'stage'}
		onclick={() => onToggle('stage')}
		type="button"
	>
		◼ Stage
	</button>
</div>

<style>
	.layout-toggle {
		display: flex;
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		overflow: hidden;
	}

	.toggle-btn {
		flex: 1;
		background: var(--bg-elevated);
		border: none;
		border-right: 1px solid var(--border);
		color: var(--text-muted);
		font-size: 10px;
		font-weight: 700;
		letter-spacing: 0.06em;
		padding: 0.3rem 0.6rem;
		cursor: pointer;
		transition: background var(--t-fast), color var(--t-fast);
		text-transform: uppercase;
	}

	.toggle-btn:last-child {
		border-right: none;
	}

	.toggle-btn:hover:not(.active) {
		background: var(--bg-hover);
		color: var(--text-secondary);
	}

	.toggle-btn.active {
		background: var(--accent-dim);
		color: var(--accent);
		border-color: var(--accent);
	}
</style>
