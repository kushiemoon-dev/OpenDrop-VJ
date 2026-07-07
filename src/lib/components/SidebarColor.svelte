<script lang="ts">
	import { type ColorParams, DEFAULT_COLOR_PARAMS } from '$lib/engine/sync.js';

	interface Props {
		colorParamsA: ColorParams;
		colorParamsB: ColorParams;
		onUpdateA: (p: ColorParams) => void;
		onUpdateB: (p: ColorParams) => void;
	}

	let { colorParamsA, colorParamsB, onUpdateA, onUpdateB }: Props = $props();
</script>

{#snippet colorDeck(label: string, params: ColorParams, onUpdate: (p: ColorParams) => void, mt?: string)}
	<div class="pl-header" style={mt}>
		<span class="label">Color {label}</span>
		<button class="btn-sm" onclick={() => onUpdate({ ...DEFAULT_COLOR_PARAMS })}>↺</button>
	</div>
	{#each ([['Hue', 'hueRotate', 0, 1, '°', 360], ['Sat', 'saturate', 0, 1, '%', 200], ['Bright', 'brightness', 0, 1, '%', 200], ['Contrast', 'contrast', 0, 1, '%', 200], ['Invert', 'invert', 0, 1, '%', 100]] as const) as [lbl, key, min, max, unit, scale]}
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label" style="width:48px">{lbl}</span>
			<input type="range" {min} {max} step="0.01" value={params[key]}
				oninput={(e) => { onUpdate({ ...params, [key]: +e.currentTarget.value }); }}
				style="flex:1" />
			<span style="font-size:9px;color:#aaa;width:28px;text-align:right">{Math.round(params[key] * scale)}{unit}</span>
		</div>
	{/each}
{/snippet}

<div class="controls-section">
	{@render colorDeck('A', colorParamsA, onUpdateA)}
	{@render colorDeck('B', colorParamsB, onUpdateB, 'margin-top:6px')}
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
