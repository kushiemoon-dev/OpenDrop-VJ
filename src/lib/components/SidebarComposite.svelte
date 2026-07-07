<script lang="ts">
	import { type SlotComposite, DEFAULT_SLOT_COMPOSITE } from '$lib/engine/sync.js';

	interface Props {
		mixerSelectedSlot: number;
		composite: SlotComposite;
		onUpdate: (patch: Partial<SlotComposite>) => void;
	}

	let { mixerSelectedSlot, composite, onUpdate }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">Composite (slot {mixerSelectedSlot})</span>
		<button class="btn-sm" onclick={() => onUpdate({ ...DEFAULT_SLOT_COMPOSITE })}>↺</button>
	</div>
	<div class="midi-row" style="gap:6px;align-items:center">
		<span class="midi-label" style="width:48px">Blend</span>
		<select class="blendmode-select" style="flex:1" value={composite.blend}
			onchange={(e) => onUpdate({ blend: e.currentTarget.value as SlotComposite['blend'] })}>
			<option value="normal">Normal</option>
			<option value="additive">Additive</option>
			<option value="screen">Screen</option>
			<option value="multiply">Multiply</option>
		</select>
	</div>
	<div class="midi-row" style="gap:6px;align-items:center">
		<label class="midi-label" style="width:auto;display:flex;align-items:center;gap:4px;cursor:pointer">
			<input type="checkbox" checked={composite.lumaKey} onchange={(e) => onUpdate({ lumaKey: e.currentTarget.checked })} />
			Luma Key
		</label>
	</div>
	{#if composite.lumaKey}
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label" style="width:48px">Black</span>
			<input type="range" min="0" max="1" step="0.01" value={composite.lumaBlack}
				oninput={(e) => onUpdate({ lumaBlack: +e.currentTarget.value })} style="flex:1" />
		</div>
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label" style="width:48px">White</span>
			<input type="range" min="0" max="1" step="0.01" value={composite.lumaWhite}
				oninput={(e) => onUpdate({ lumaWhite: +e.currentTarget.value })} style="flex:1" />
		</div>
	{/if}
	<div class="midi-row" style="gap:6px;align-items:center">
		<label class="midi-label" style="width:auto;display:flex;align-items:center;gap:4px;cursor:pointer">
			<input type="checkbox" checked={composite.colorKey} onchange={(e) => onUpdate({ colorKey: e.currentTarget.checked })} />
			Color Key
		</label>
	</div>
	{#if composite.colorKey}
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label" style="width:48px">Hue</span>
			<input type="range" min="0" max="1" step="0.01" value={composite.colorHue}
				oninput={(e) => onUpdate({ colorHue: +e.currentTarget.value })} style="flex:1" />
		</div>
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label" style="width:48px">Tol</span>
			<input type="range" min="0" max="1" step="0.01" value={composite.colorTol}
				oninput={(e) => onUpdate({ colorTol: +e.currentTarget.value })} style="flex:1" />
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

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.midi-label { font-size: 10px; color: var(--text-muted); width: 80px; flex-shrink: 0; white-space: nowrap; }

	.blendmode-select {
		flex: 1; background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.4rem; font-size: 11px; cursor: pointer;
	}
</style>
