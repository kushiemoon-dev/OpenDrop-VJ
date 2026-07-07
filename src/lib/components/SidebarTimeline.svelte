<script lang="ts">
	import { timelineLoopDuration, type TimelineKeyframe } from '$lib/engine/timeline.js';
	import type { Snapshot } from '$lib/engine/snapshot.js';

	interface Props {
		timelinePlaying: boolean;
		timelineKeyframes: TimelineKeyframe[];
		snapshots: (Snapshot | null)[];
		onTogglePlay: () => void;
		onUpdateKeyframe: (index: number, patch: Partial<TimelineKeyframe>) => void;
		onRemoveKeyframe: (index: number) => void;
		onAddKeyframe: () => void;
	}

	let { timelinePlaying, timelineKeyframes, snapshots, onTogglePlay, onUpdateKeyframe, onRemoveKeyframe, onAddKeyframe }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">Timeline</span>
		<button class="btn-sm" class:active={timelinePlaying} disabled={timelineLoopDuration(timelineKeyframes) <= 0}
			onclick={onTogglePlay} title={timelinePlaying ? 'Pause' : 'Play'}>
			{timelinePlaying ? '⏸' : '▶'}
		</button>
	</div>
	{#each timelineKeyframes as kf, i}
		<div class="midi-row" style="gap:4px;align-items:center">
			<select class="blendmode-select" value={kf.slot}
				onchange={(e) => onUpdateKeyframe(i, { slot: +e.currentTarget.value })}
				style="font-size:11px">
				{#each [0, 1, 2, 3, 4, 5, 6, 7] as slot}
					<option value={slot} disabled={!snapshots[slot]}>
						{snapshots[slot]?.name ?? `Slot ${slot} (vide)`}
					</option>
				{/each}
			</select>
			<input type="number" min="0" step="0.5" value={kf.timeSec}
				oninput={(e) => onUpdateKeyframe(i, { timeSec: +e.currentTarget.value })}
				style="width:56px;background:#1a1a1a;border:1px solid #333;border-radius:3px;color:#ccc;font-size:11px;padding:2px 4px" />
			<span style="font-size:9px;color:#666">s</span>
			<button class="pl-remove" onclick={() => onRemoveKeyframe(i)} title="Retirer">×</button>
		</div>
	{/each}
	<button class="btn-sm" onclick={onAddKeyframe}>+ Point</button>
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

	.btn-sm:disabled { opacity: 0.3; cursor: not-allowed; }

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.blendmode-select {
		flex: 1; background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.4rem; font-size: 11px; cursor: pointer;
	}

	.pl-remove {
		background: none; border: none; color: var(--text-muted);
		cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; flex-shrink: 0;
		transition: color var(--t-fast);
	}

	.pl-remove:hover { color: var(--accent); }
</style>
