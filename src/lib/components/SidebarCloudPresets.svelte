<!-- src/lib/components/SidebarCloudPresets.svelte -->
<script lang="ts">
	import { CLOUD_PRESET_PREFIX, type CloudPresetEntry } from '$lib/engine/cloud-presets.js';

	interface Props {
		presets: CloudPresetEntry[];
		token: string;
		error: string | null;
		onUploadFile: (e: Event) => void;
		onCopyToken: () => void;
		copyLabel: string;
		onLinkDevice: (token: string) => void;
		onLoadPreset: (name: string) => void;
		onRename: (id: string, name: string) => void;
		onDelete: (id: string) => void;
	}

	let {
		presets, token, error, onUploadFile, onCopyToken, copyLabel, onLinkDevice, onLoadPreset, onRename, onDelete,
	}: Props = $props();

	let linkTokenInput = $state('');
	let renamingId = $state<string | null>(null);
	let renameValue = $state('');

	function startRename(entry: CloudPresetEntry) {
		renamingId = entry.id;
		renameValue = entry.name.replace(CLOUD_PRESET_PREFIX, '');
	}

	function confirmRename() {
		if (renamingId) onRename(renamingId, renameValue);
		renamingId = null;
	}

	function handleLink() {
		if (!linkTokenInput.trim()) return;
		onLinkDevice(linkTokenInput.trim());
		linkTokenInput = '';
	}

	function handleDelete(entry: CloudPresetEntry) {
		if (window.confirm(`Delete "${entry.name}"?`)) onDelete(entry.id);
	}
</script>

<div class="controls-section">
	<div class="pl-header">
		<span class="label">My presets ({presets.length})</span>
		<label class="btn-sm file-label" title="Upload a JSON preset (Butterchurn format)">
			+ Upload
			<input type="file" accept=".json,application/json" onchange={onUploadFile} style="display:none" />
		</label>
	</div>
	{#if error}
		<p class="hint hint-error">{error}</p>
	{/if}
	<div class="cloud-token-row">
		<button class="btn-sm" onclick={onCopyToken}>{copyLabel}</button>
		<input class="cloud-token-input" type="text" placeholder="Link another device (paste the token)"
			bind:value={linkTokenInput} onkeydown={(e) => { if (e.key === 'Enter') handleLink(); }} />
		<button class="btn-sm" onclick={handleLink}>Link</button>
	</div>
	{#if presets.length === 0}
		<p class="hint">No custom presets yet. Upload a JSON file in Butterchurn format.</p>
	{/if}
	<ul class="overlay-list">
		{#each presets as entry (entry.id)}
			<li class="overlay-item">
				<div class="overlay-row">
					{#if renamingId === entry.id}
						<input class="cloud-token-input" type="text" bind:value={renameValue}
							onkeydown={(e) => { if (e.key === 'Enter') confirmRename(); }} />
						<button class="btn-sm pl-btn" onclick={confirmRename}>✓</button>
					{:else}
						<button class="overlay-name" onclick={() => onLoadPreset(entry.name)} title="Load onto the active deck">
							{entry.name}
						</button>
						<button class="btn-sm pl-btn" onclick={() => startRename(entry)} title="Rename">✎</button>
					{/if}
					<button class="pl-remove" onclick={() => handleDelete(entry)} title="Delete">×</button>
				</div>
			</li>
		{/each}
	</ul>
</div>

<style>
	.controls-section {
		padding: 0.7rem 0.75rem;
		border-bottom: 1px solid var(--border-subtle);
		display: flex; flex-direction: column; gap: 0.4rem;
	}

	.label {
		font-size: 10px; text-transform: uppercase; letter-spacing: 0.1em;
		color: var(--text-muted); font-weight: 600;
	}

	.hint { margin: 0.2rem 0; font-size: 11px; color: #aaaacc; line-height: 1.5; }
	.hint-error { color: var(--accent); }

	.pl-header { display: flex; align-items: center; justify-content: space-between; }

	.btn-sm {
		background: var(--bg-base); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: 5px;
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: all 0.12s;
	}
	.btn-sm:hover:not(:disabled) { background: #141436; color: #ddddf5; border-color: #3a3a6a; }

	.file-label { display: inline-block; cursor: pointer; }

	.pl-btn { padding: 0.22rem 0.4rem; font-size: 11px; }

	.pl-remove {
		background: none; border: none; color: #33335a;
		cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; flex-shrink: 0;
		transition: color 0.1s;
	}
	.pl-remove:hover { color: var(--accent); }

	.cloud-token-row { display: flex; gap: 4px; align-items: center; }

	.cloud-token-input {
		flex: 1; min-width: 0;
		background: var(--bg-base); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: 5px;
		padding: 0.25rem 0.5rem; font-size: 11px;
	}
	.cloud-token-input:focus { outline: none; border-color: var(--violet); }

	.overlay-list {
		list-style: none;
		display: flex; flex-direction: column; gap: 2px;
		max-height: 200px; overflow-y: auto;
		scrollbar-width: thin; scrollbar-color: #2a2a5a transparent;
	}

	.overlay-item {
		background: var(--bg-surface);
		border: 1px solid #161640;
		border-radius: 5px;
		overflow: hidden;
	}

	.overlay-row { display: flex; align-items: center; gap: 3px; padding: 2px 4px; }

	.overlay-name {
		flex: 1;
		background: none; border: none; color: var(--text-secondary);
		font-size: 11px; cursor: pointer; text-align: left;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
		padding: 2px 0; transition: color 0.1s;
	}
	.overlay-name:hover { color: var(--violet); }
</style>
