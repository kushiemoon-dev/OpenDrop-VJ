<script lang="ts">
	type NdiSlot = { active: boolean; error: string };

	interface Props {
		slots: NdiSlot[];
		slotLabels: string[];
		toggleNdiDeck: (slot: number) => void;
	}

	let { slots, slotLabels, toggleNdiDeck }: Props = $props();
</script>

<div class="controls-section">
	<span class="label">NDI per deck</span>
	{#each slots as slot, i (i)}
		<div class="midi-row">
			<span class="midi-label">{slotLabels[i]}</span>
			<button class="btn-sm" class:active={slot.active} onclick={() => toggleNdiDeck(i)}>
				{slot.active ? 'Stop' : 'Start'}
			</button>
		</div>
		{#if slot.error}<div class="ndi-error">{slot.error}</div>{/if}
	{/each}
</div>

<style>
	.controls-section {
		padding: var(--sp-3);
		border-bottom: 1px solid var(--border-subtle);
		display: flex; flex-direction: column; gap: 0.4rem;
	}

	.label {
		font-size: 10px; text-transform: uppercase; letter-spacing: 0.08em;
		color: var(--accent); font-weight: 600;
	}

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.midi-label { font-size: 10px; color: var(--text-muted); width: 80px; flex-shrink: 0; white-space: nowrap; }

	.btn-sm {
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: border-color var(--t-fast), color var(--t-fast);
	}

	.btn-sm:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }

	.btn-sm.active {
		background: var(--accent-dim); border-color: var(--accent); color: var(--accent);
		box-shadow: 0 0 8px var(--accent-dim);
	}

	.ndi-error {
		font-size: 10px;
		color: var(--error);
		margin-top: 2px;
	}
</style>
