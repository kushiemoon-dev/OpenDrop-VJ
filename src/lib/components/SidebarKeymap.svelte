<script lang="ts">
	import type { CommandId, CommandRegistry } from '$lib/engine/commands.js';
	import { formatKey } from '$lib/engine/keymap.js';

	interface Props {
		learningKey: CommandId | null;
		keyById: Map<CommandId, string>;
		registry: CommandRegistry;
		onResetKeymap: () => void;
		onToggleLearnKey: (id: CommandId) => void;
		onClearKeyBinding: (id: CommandId) => void;
	}

	let { learningKey, keyById, registry, onResetKeymap, onToggleLearnKey, onClearKeyBinding }: Props = $props();
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">Keyboard</span>
		<button class="btn-sm" onclick={onResetKeymap}>Reset</button>
	</div>
	{#if learningKey !== null}
		<span style="font-size:11px;color:var(--warn)">Press the key to assign… (Esc = cancel)</span>
	{/if}
	<div class="midi-list">
		{#each registry.all() as cmd}
			{@const assignedKey = keyById.get(cmd.id)}
			<div class="midi-row">
				<span class="midi-label">{cmd.label}</span>
				<span class="midi-binding" class:midi-learning={learningKey === cmd.id}>
					{assignedKey ? formatKey(assignedKey) : '—'}
				</span>
				<button class="btn-sm pl-btn" class:active={learningKey === cmd.id}
					onclick={() => onToggleLearnKey(cmd.id)}>
					{learningKey === cmd.id ? '…' : 'Learn'}
				</button>
				{#if assignedKey}
					<button class="pl-remove" onclick={() => onClearKeyBinding(cmd.id)}>×</button>
				{/if}
			</div>
		{/each}
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

	.pl-btn { padding: 0.22rem 0.4rem; font-size: 11px; }

	.pl-remove {
		background: none; border: none; color: var(--text-muted);
		cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; flex-shrink: 0;
		transition: color var(--t-fast);
	}

	.pl-remove:hover { color: var(--accent); }

	.midi-list {
		display: flex; flex-direction: column; gap: 2px;
		max-height: 160px; overflow-y: auto;
	}

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.midi-label { font-size: 10px; color: var(--text-muted); width: 80px; flex-shrink: 0; white-space: nowrap; }

	.midi-binding {
		flex: 1; font-size: 10px; color: var(--text-muted); font-family: 'Courier New', monospace;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.midi-binding.midi-learning { color: var(--warn); animation: blink 0.6s step-end infinite; }

	@keyframes blink { 50% { opacity: 0; } }
</style>
