<script lang="ts">
	import type { LfoSlot } from '$lib/engine/lfo.js';
	import type { CommandRegistry } from '$lib/engine/commands.js';

	interface Props {
		lfoSlots: LfoSlot[];
		registry: CommandRegistry;
	}

	let { lfoSlots, registry }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header"><span class="label">LFO</span></div>
	{#each lfoSlots as slot, i}
		<div style="margin-bottom:6px;font-size:11px">
			<div class="midi-row" style="gap:4px;flex-wrap:wrap">
				<input type="checkbox" bind:checked={slot.enabled} />
				<span class="midi-label">LFO {i+1}</span>
				{#each (['sine','saw','square','sh'] as const) as shape}
					<button class="btn-sm" class:active={slot.shape === shape}
						onclick={() => { slot.shape = shape; }}>
						{shape}
					</button>
				{/each}
			</div>
			{#if slot.enabled}
				<div class="midi-row" style="gap:6px;align-items:center;margin-top:3px">
					<span class="midi-label">Cible</span>
					<select style="flex:1;font-size:10px;background:#222;color:#ccc;border:1px solid #444;border-radius:3px"
						value={slot.target ?? ''}
						onchange={(e) => { slot.target = (e.currentTarget.value || null) as typeof slot.target; }}>
						<option value="">—</option>
						{#each registry.all().filter(c => c.kind === 'range') as cmd}
							<option value={cmd.id}>{cmd.label}</option>
						{/each}
					</select>
				</div>
				<div class="midi-row" style="gap:6px;align-items:center;margin-top:2px">
					<span class="midi-label">Rate</span>
					<input type="range" min="0.25" max="4" step="0.25" bind:value={slot.rate} style="flex:1" />
					<span style="font-size:10px;color:#aaa">{slot.rate}×</span>
				</div>
				<div class="midi-row" style="gap:6px;align-items:center;margin-top:2px">
					<span class="midi-label">Amount</span>
					<input type="range" min="0" max="1" step="0.05" bind:value={slot.amount} style="flex:1" />
				</div>
			{/if}
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

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.midi-label { font-size: 10px; color: var(--text-muted); width: 80px; flex-shrink: 0; white-space: nowrap; }
</style>
