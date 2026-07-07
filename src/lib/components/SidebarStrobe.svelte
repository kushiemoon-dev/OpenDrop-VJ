<script lang="ts">
	interface Props {
		strobeOn: boolean;
		strobeRate: number;
		strobeIntensity: number;
		strobeColor: string;
		onToggleStrobe: () => void;
		onRateChange: (r: number) => void;
		onIntensityChange: (v: number) => void;
		onColorChange: (c: string) => void;
	}

	let { strobeOn, strobeRate, strobeIntensity, strobeColor, onToggleStrobe, onRateChange, onIntensityChange, onColorChange }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">Strobe</span>
		<button class="btn-sm" class:active={strobeOn} onclick={onToggleStrobe}>
			{strobeOn ? 'ON' : 'OFF'}
		</button>
	</div>
	{#if strobeOn}
		<div class="midi-row" style="gap:4px;flex-wrap:wrap">
			<span class="midi-label">Rate</span>
			{#each [0.25, 0.5, 1, 2, 4] as r}
				<button class="btn-sm" class:active={strobeRate === r}
					onclick={() => onRateChange(r)}>
					{r < 1 ? `1/${Math.round(1/r)}` : `${r}×`}
				</button>
			{/each}
		</div>
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label">Intensité</span>
			<input type="range" min="0" max="1" step="0.05" value={strobeIntensity}
				oninput={(e) => onIntensityChange(+e.currentTarget.value)} style="flex:1" />
			<span style="font-size:10px;color:#aaa">{Math.round(strobeIntensity*100)}%</span>
		</div>
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label">Couleur</span>
			<input type="color" value={strobeColor}
				oninput={(e) => onColorChange(e.currentTarget.value)}
				style="width:32px;height:20px;padding:0;border:none;background:none;cursor:pointer" />
		</div>
	{/if}
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
