<script lang="ts">
	import type { DeckTimeParams } from '$lib/engine/time-params.js';

	interface Props {
		mixerSelectedSlot: number;
		timeParams: DeckTimeParams;
		onUpdate: (patch: Partial<DeckTimeParams>) => void;
		onReset: () => void;
	}

	let { mixerSelectedSlot, timeParams, onUpdate, onReset }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">Time (slot {mixerSelectedSlot})</span>
		<button class="btn-sm" onclick={onReset}>↺</button>
	</div>
	{#each ([['Speed', 'speedMult'], ['Zoom', 'zoomMult'], ['Rotation', 'rotMult'], ['Wrap', 'warpMult'], ['Horizontal', 'dxMult'], ['Vertical', 'dyMult'], ['Stretch', 'stretchMult'], ['Wave', 'waveMult']] as const) as [lbl, field]}
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label" style="width:48px">{lbl}</span>
			<input type="range" min="0" max="2" step="0.01" value={timeParams[field]}
				oninput={(e) => onUpdate({ [field]: +e.currentTarget.value } as Partial<DeckTimeParams>)}
				style="flex:1" />
			<span style="font-size:9px;color:#aaa;width:28px;text-align:right">{timeParams[field].toFixed(2)}</span>
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

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.midi-label { font-size: 10px; color: var(--text-muted); width: 80px; flex-shrink: 0; white-space: nowrap; }
</style>
