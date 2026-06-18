<script lang="ts">
	type OutputDevice = { id: number; name: string; maxInputChannels: number; maxOutputChannels: number; defaultSampleRate: number };

	interface Props {
		sourceLabel: string;
		status: 'idle' | 'running' | 'error';
		effectiveOS: string;
		vuLevel: number;
		sourceError: string;
		showSystemAudioHelp: boolean;
		showDevicePicker: boolean;
		audioDevices: MediaDeviceInfo[];
		outputDevices: OutputDevice[];
		loopbackSupported: boolean;
		audioElHasSrc: boolean;
		onConnectMic: () => void;
		onOpenDevicePicker: () => void;
		onCaptureSystemAudio: () => void;
		onConnectFile: () => void;
		onFileChange: (e: Event) => void;
		onConnectDevice: (device: MediaDeviceInfo) => void;
		onConnectLoopback: (device: OutputDevice) => void;
		onDismissSystemAudioHelp: () => void;
		onDismissDevicePicker: () => void;
	}

	let {
		sourceLabel,
		status,
		effectiveOS,
		vuLevel,
		sourceError,
		showSystemAudioHelp,
		showDevicePicker,
		audioDevices,
		outputDevices,
		loopbackSupported,
		audioElHasSrc,
		onConnectMic,
		onOpenDevicePicker,
		onCaptureSystemAudio,
		onConnectFile,
		onFileChange,
		onConnectDevice,
		onConnectLoopback,
		onDismissSystemAudioHelp,
		onDismissDevicePicker,
	}: Props = $props();
</script>

<div class="controls-section">
	<span class="label">Audio source</span>
	<div class="btn-row">
		<button class="btn-sm" class:active={sourceLabel === 'microphone'} onclick={onConnectMic} disabled={status !== 'running'}>Mic</button>
		<button class="btn-sm" onclick={onOpenDevicePicker} disabled={status !== 'running'}>Pick device</button>
		<button class="btn-sm" class:active={sourceLabel === 'system audio'} onclick={onCaptureSystemAudio} disabled={status !== 'running'} title="Capturer le son système">🔊 Audio système</button>
	</div>
	{#if showSystemAudioHelp}
		<div class="device-picker">
			{#if effectiveOS === 'darwin'}
				<span class="label">Audio système sur macOS</span>
				<p class="hint">Installer <strong>BlackHole</strong> (gratuit) :<br><code>brew install blackhole-2ch</code><br>Créer un Multi-Output Device dans Audio MIDI Setup,<br>puis <strong>Pick device</strong> → BlackHole.</p>
			{:else if effectiveOS === 'linux'}
				<span class="label">Audio système sur Linux</span>
				<p class="hint">Aucun périphérique monitor trouvé.<br>Utilisez <strong>Pick device</strong> → entrée se terminant par <code>.monitor</code> (sortie système).<br>Optionnel : <code>bash scripts/setup-audio.sh</code> pour un device nommé.</p>
			{:else}
				<span class="label">Audio système</span>
				<p class="hint">Dans Chrome/Edge : cliquer <strong>Audio système</strong> → choisir <strong>Écran entier</strong> → cocher <strong>"Partager l'audio système"</strong>.</p>
			{/if}
			<button class="btn-sm" onclick={onDismissSystemAudioHelp}>OK</button>
		</div>
	{/if}
	<div class="file-row">
		<label class="btn-sm file-label">
			File
			<input type="file" accept="audio/*" onchange={onFileChange} style="display:none" />
		</label>
		{#if audioElHasSrc && status === 'running'}
			<button class="btn-sm" class:active={sourceLabel === 'file'} onclick={onConnectFile}>▶ Play</button>
		{/if}
	</div>
	{#if sourceLabel !== 'none'}
		<span class="source-badge">▶ {sourceLabel}</span>
	{/if}
	{#if status === 'running'}
		<div class="vu-meter">
			<div class="vu-bar" style="width:{Math.round(vuLevel * 100)}%"></div>
		</div>
	{/if}
	{#if sourceError}
		<span class="source-error">⚠ {sourceError}</span>
	{/if}
	{#if showDevicePicker}
		<div class="device-picker">
			<span class="label">🎤 Inputs</span>
			{#each audioDevices as device}
				<button class="device-item" onclick={() => onConnectDevice(device)}>
					{device.label || `Device ${device.deviceId.slice(0, 8)}`}
				</button>
			{/each}
			{#if loopbackSupported && outputDevices.length > 0}
				<span class="label" style="margin-top:6px">🔊 Outputs (loopback)</span>
				{#each outputDevices as device}
					<button class="device-item" onclick={() => onConnectLoopback(device)}>
						{device.name}
					</button>
				{/each}
			{/if}
			<button class="btn-sm" onclick={onDismissDevicePicker}>Cancel</button>
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

	.btn-row, .file-row { display: flex; gap: 0.4rem; }

	.source-badge { font-size: 11px; color: #00e5ff; }
	.source-error { font-size: 11px; color: #ff6090; word-break: break-word; }

	.vu-meter {
		height: 5px; background: var(--bg-base); border-radius: var(--r-sm); overflow: hidden;
		border: 1px solid var(--border-subtle);
	}

	.vu-bar {
		height: 100%;
		background: linear-gradient(90deg, var(--accent), #b44fff 50%, #00e5ff);
		border-radius: var(--r-sm);
		transition: width 50ms linear;
		box-shadow: 0 0 8px var(--accent-glow);
	}

	.device-picker {
		display: flex; flex-direction: column; gap: 0.2rem;
		margin-top: 0.2rem; padding: 0.4rem;
		background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--r-md);
	}

	.device-item {
		display: block; width: 100%; text-align: left;
		background: none; border: none; color: var(--text-secondary);
		padding: 0.3rem 0.4rem; cursor: pointer; font-size: 11px;
		border-radius: var(--r-sm); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
		transition: all var(--t-fast);
	}

	.device-item:hover { background: var(--bg-hover); color: #fff; }

	.hint { margin: 0.2rem 0; font-size: 11px; color: var(--text-secondary); line-height: 1.5; }

	.hint code { background: var(--bg-hover); padding: 0.1rem 0.3rem; border-radius: var(--r-sm); font-size: 10px; }

	.hint strong { color: var(--text-primary); }

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

	.file-label { display: inline-block; cursor: pointer; }
</style>
