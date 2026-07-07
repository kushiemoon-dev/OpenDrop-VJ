<script lang="ts">
	interface Props {
		isElectron: boolean;
		oscActive: boolean;
		oscPort: number;
		oscError: string;
		remoteActive: boolean;
		remoteUrl: string;
		remoteError: string;
		linkActive: boolean;
		linkPeers: number;
		linkError: string;
		onToggleOsc: () => void;
		onOscPortChange: (port: number) => void;
		onToggleRemote: () => void;
		onToggleLink: () => void;
	}

	let {
		isElectron, oscActive, oscPort, oscError, remoteActive, remoteUrl, remoteError,
		linkActive, linkPeers, linkError, onToggleOsc, onOscPortChange, onToggleRemote, onToggleLink,
	}: Props = $props();
</script>

{#if isElectron}
<div class="controls-section">
	<div class="pl-header">
		<span class="label">OSC</span>
		<button class="btn-sm" class:active={oscActive} onclick={onToggleOsc}>
			{oscActive ? 'Stop' : 'Start'}
		</button>
	</div>
	{#if oscActive}
		<span class="source-badge">Écoute UDP :{oscPort}</span>
		<span style="font-size:10px;color:#aaa">Adresse : /opendrop/&lt;commandId&gt; float32</span>
	{:else}
		<div class="midi-row" style="gap:6px;align-items:center">
			<span class="midi-label">Port</span>
			<input type="number" min="1024" max="65535" value={oscPort}
				oninput={(e) => onOscPortChange(+e.currentTarget.value)}
				style="width:70px;background:#1a1a1a;border:1px solid #333;border-radius:3px;color:#ccc;font-size:11px;padding:2px 4px" />
		</div>
	{/if}
	{#if oscError}<div style="font-size:10px;color:var(--error);margin-top:4px">{oscError}</div>{/if}
</div>
<div class="controls-section">
	<div class="pl-header">
		<span class="label">Remote</span>
		<button class="btn-sm" class:active={remoteActive} onclick={onToggleRemote}>
			{remoteActive ? 'Stop' : 'Démarrer'}
		</button>
	</div>
	{#if remoteActive && remoteUrl}
		<span style="font-size:10px;color:#aaa;word-break:break-all">{remoteUrl}</span>
		<a href={remoteUrl} target="_blank" rel="noopener" style="font-size:10px;color:var(--info);display:block;margin-top:4px">
			Ouvrir sur cet appareil ↗
		</a>
	{/if}
	{#if !remoteActive}
		<span style="font-size:10px;color:#666">Démarre un serveur WS local pour piloter OpenDrop depuis un téléphone sur le même réseau.</span>
	{/if}
	{#if remoteError}<div style="font-size:10px;color:var(--error);margin-top:4px">{remoteError}</div>{/if}
</div>
<div class="controls-section">
	<div class="pl-header">
		<span class="label">Ableton Link</span>
		<button class="btn-sm" class:active={linkActive} onclick={onToggleLink}>
			{linkActive ? 'Stop' : 'Démarrer'}
		</button>
	</div>
	{#if linkActive}
		<span class="source-badge">{linkPeers} pair{linkPeers !== 1 ? 's' : ''} connecté{linkPeers !== 1 ? 's' : ''}</span>
	{:else}
		<span style="font-size:10px;color:#666">Synchronise le tempo avec Ableton Live et autres apps Link sur le réseau local.</span>
	{/if}
	{#if linkError}<div style="font-size:10px;color:var(--error);margin-top:4px">{linkError}</div>{/if}
</div>
{/if}

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

	.source-badge { font-size: 11px; color: var(--cyan); }

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
