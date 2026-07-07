<script lang="ts">
	import type { QualityTier, InvisibleMode } from '$lib/engine/quality.js';

	interface Props {
		quality: QualityTier;
		targetFps: number;
		invisibleMode: InvisibleMode;
		status: 'idle' | 'running' | 'error';
		fps: number;
		onQualityChange: (q: QualityTier) => void;
		onTargetFpsChange: (n: number) => void;
		onInvisibleModeChange: (m: InvisibleMode) => void;
	}

	let { quality, targetFps, invisibleMode, status, fps, onQualityChange, onTargetFpsChange, onInvisibleModeChange }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">Qualité rendu</span>
		{#if status === 'running' && fps > 0}
			<span class="label" style="color:var(--info)">{fps} fps</span>
		{/if}
	</div>
	<div class="btn-row">
		<button class="btn-sm" class:active={quality === 'low'} onclick={() => onQualityChange('low')} disabled={status !== 'running'}>Low</button>
		<button class="btn-sm" class:active={quality === 'medium'} onclick={() => onQualityChange('medium')} disabled={status !== 'running'}>Med</button>
		<button class="btn-sm" class:active={quality === 'high'} onclick={() => onQualityChange('high')} disabled={status !== 'running'}>High</button>
	</div>
	<div class="btn-row" style="margin-top:6px">
		<button class="btn-sm" class:active={targetFps === 30} onclick={() => onTargetFpsChange(30)} disabled={status !== 'running'}>30 fps</button>
		<button class="btn-sm" class:active={targetFps === 45} onclick={() => onTargetFpsChange(45)} disabled={status !== 'running'}>45 fps</button>
		<button class="btn-sm" class:active={targetFps === 60} onclick={() => onTargetFpsChange(60)} disabled={status !== 'running'}>60 fps</button>
	</div>
	<div class="btn-row" style="margin-top:4px">
		<button class="btn-sm" class:active={invisibleMode === 'eco'} onclick={() => onInvisibleModeChange('eco')} disabled={status !== 'running'} title="Decks cachés à ~8 fps">Éco</button>
		<button class="btn-sm" class:active={invisibleMode === 'pause'} onclick={() => onInvisibleModeChange('pause')} disabled={status !== 'running'} title="Decks cachés pausés">Pause</button>
		<button class="btn-sm" class:active={invisibleMode === 'off'} onclick={() => onInvisibleModeChange('off')} disabled={status !== 'running'} title="Tous les decks à plein régime">Off</button>
	</div>
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

	.btn-row { display: flex; gap: 0.4rem; }

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

	.btn-sm:disabled { opacity: 0.3; cursor: not-allowed; }
</style>
