<script lang="ts">
	import type { DeckQVarParams } from '$lib/engine/q-vars.js';

	interface Props {
		mixerSelectedSlot: number;
		qvar: DeckQVarParams;
		onAddWatch: (n: number) => void;
		onUpdateValue: (n: number, value: number) => void;
		onRemoveWatch: (n: number) => void;
	}

	let { mixerSelectedSlot, qvar, onAddWatch, onUpdateValue, onRemoveWatch }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">Q-vars (slot {mixerSelectedSlot})</span>
	</div>
	<div class="midi-row" style="gap:6px;align-items:center">
		<select class="blendmode-select" style="flex:1"
			onchange={(e) => {
				const n = Number(e.currentTarget.value);
				if (n) onAddWatch(n);
				e.currentTarget.value = '';
			}}>
			<option value="">+ Add Q-var</option>
			{#each Array.from({ length: 32 }, (_, i) => i + 1).filter((n) => !qvar.enabled[n - 1]) as n (n)}
				<option value={n}>Q{n}</option>
			{/each}
		</select>
	</div>
	{#each Array.from({ length: 32 }, (_, i) => i + 1).filter((n) => qvar.enabled[n - 1]) as n (n)}
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label" style="width:48px">Q{n}</span>
			<input type="range" min="-2" max="2" step="0.01" value={qvar.value[n - 1]}
				oninput={(e) => onUpdateValue(n, +e.currentTarget.value)}
				style="flex:1" />
			<span style="font-size:9px;color:#aaa;width:28px;text-align:right">{qvar.value[n - 1]!.toFixed(2)}</span>
			<button class="btn-sm" onclick={() => onRemoveWatch(n)}>×</button>
		</div>
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

	.pl-header { display: flex; align-items: center; justify-content: space-between; }

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.midi-label { font-size: 10px; color: var(--text-muted); width: 80px; flex-shrink: 0; white-space: nowrap; }

	.blendmode-select {
		flex: 1; background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.4rem; font-size: 11px; cursor: pointer;
	}

	.btn-sm {
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: border-color var(--t-fast), color var(--t-fast);
	}

	.btn-sm:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }
</style>
