<script lang="ts">
	import type { Snapshot } from '$lib/engine/snapshot.js';

	interface Props {
		snapshotRecallDuration: number;
		snapshots: (Snapshot | null)[];
		onDurationChange: (v: number) => void;
		onRenameSnapshot: (slot: number, name: string) => void;
		onSaveSnapshot: (slot: number) => void;
		onRecallSnapshot: (slot: number) => void;
		onClearSnapshot: (slot: number) => void;
	}

	let { snapshotRecallDuration, snapshots, onDurationChange, onRenameSnapshot, onSaveSnapshot, onRecallSnapshot, onClearSnapshot }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">Snapshots</span>
	</div>
	<div class="midi-row" style="gap:6px;align-items:center">
		<span class="midi-label" style="width:48px">Duration</span>
		<input type="range" min="0.1" max="10" step="0.1" value={snapshotRecallDuration}
			oninput={(e) => onDurationChange(+e.currentTarget.value)} style="flex:1" />
		<span style="font-size:9px;color:#aaa;width:28px;text-align:right">{snapshotRecallDuration.toFixed(1)}s</span>
	</div>
	{#each [0, 1, 2, 3, 4, 5, 6, 7] as slot}
		{@const snap = snapshots[slot]}
		<div class="midi-row" style="gap:4px;align-items:center">
			<input class="snap-name" type="text" value={snap?.name ?? ''} placeholder="—"
				disabled={!snap}
				oninput={(e) => onRenameSnapshot(slot, e.currentTarget.value)}
				style="flex:1;min-width:0;font-size:11px" />
			<button class="btn-sm" onclick={() => onSaveSnapshot(slot)} title="Capture the current state">Save</button>
			<button class="btn-sm" disabled={!snap} onclick={() => onRecallSnapshot(slot)} title="Recall this snapshot">▶</button>
			<button class="pl-remove" disabled={!snap} onclick={() => onClearSnapshot(slot)} title="Clear">×</button>
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

	.snap-name {
		background: var(--bg-elevated); color: var(--text-primary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.2rem 0.4rem;
	}
	.snap-name:focus { outline: none; border-color: var(--accent); }
	.snap-name:disabled { color: var(--text-muted); cursor: not-allowed; }

	.btn-sm {
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: border-color var(--t-fast), color var(--t-fast);
	}

	.btn-sm:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }
	.btn-sm:disabled { opacity: 0.3; cursor: not-allowed; }

	.pl-remove {
		background: none; border: none; color: var(--text-muted);
		cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; flex-shrink: 0;
		transition: color var(--t-fast);
	}

	.pl-remove:hover { color: var(--accent); }
	.pl-remove:disabled { opacity: 0.3; cursor: not-allowed; }
	.pl-remove:disabled:hover { color: var(--text-muted); }
</style>
