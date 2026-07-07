<script lang="ts">
	interface Props {
		shareSetName: string;
		shareCopyLabel: string;
		nonShareableOverlayCount: number;
		onNameChange: (name: string) => void;
		onCopyShareLink: () => void;
	}

	let { shareSetName, shareCopyLabel, nonShareableOverlayCount, onNameChange, onCopyShareLink }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">Share</span>
	</div>
	<input class="snap-name" type="text" value={shareSetName}
		oninput={(e) => onNameChange(e.currentTarget.value)}
		placeholder="My set"
		style="font-size:11px;margin-bottom:4px;width:100%" />
	<button class="btn-sm" onclick={onCopyShareLink}>{shareCopyLabel}</button>
	{#if nonShareableOverlayCount > 0}
		<span style="font-size:9px;color:#aaa;margin-top:4px;display:block">
			{nonShareableOverlayCount} media overlay(s) not shareable (local file)
		</span>
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

	.snap-name {
		background: var(--bg-elevated); color: var(--text-primary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.2rem 0.4rem;
	}
	.snap-name:focus { outline: none; border-color: var(--accent); }

	.btn-sm {
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: border-color var(--t-fast), color var(--t-fast);
	}

	.btn-sm:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }
</style>
