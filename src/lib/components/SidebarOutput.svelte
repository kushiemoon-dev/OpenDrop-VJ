<script lang="ts">
	type DisplayInfo = { id: number; label: string; isPrimary: boolean; bounds: { x: number; y: number; width: number; height: number } };

	interface Props {
		status: 'idle' | 'running' | 'error';
		isElectron: boolean;
		outputOpen: boolean;
		displays: DisplayInfo[];
		selectedDisplayId: number | null;
		showStreamPanel: boolean;
		platform: string;
		ndiActive: boolean;
		v4l2Active: boolean;
		spoutActive: boolean;
		recordActive: boolean;
		recordPath: string;
		ndiError: string;
		v4l2Error: string;
		spoutError: string;
		recordError: string;
		onOpenOutput: () => void;
		onOpenOutputFullscreen: () => void;
		onToggleStreamPanel: () => void;
		onSelectDisplay: (id: number) => void;
		onToggleNdi: () => void;
		onToggleV4l2: () => void;
		onToggleSpout: () => void;
		onToggleRecord: () => void;
	}

	let {
		status, isElectron, outputOpen, displays, selectedDisplayId, showStreamPanel, platform,
		ndiActive, v4l2Active, spoutActive, recordActive, recordPath, ndiError, v4l2Error, spoutError, recordError,
		onOpenOutput, onOpenOutputFullscreen, onToggleStreamPanel, onSelectDisplay,
		onToggleNdi, onToggleV4l2, onToggleSpout, onToggleRecord,
	}: Props = $props();
</script>

<div class="controls-section">
	<div class="output-row">
		<button class="btn-output" onclick={onOpenOutput} disabled={status !== 'running'}>
			⎋ Open output window
		</button>
		{#if isElectron && outputOpen}
			<button class="btn-stream" class:stream-active={ndiActive || v4l2Active || spoutActive || recordActive}
				onclick={onToggleStreamPanel}
				title="Stream output">
				⏏ Stream {ndiActive || v4l2Active || spoutActive || recordActive ? '●' : '○'}
			</button>
		{/if}
	</div>
	{#if isElectron && displays.length > 0}
		<div class="midi-row" style="gap:6px;align-items:center;margin-top:6px">
			<select
				style="flex:1;font-size:10px;background:#1a1a1a;border:1px solid #333;border-radius:3px;color:#ccc;padding:3px 4px"
				value={selectedDisplayId}
				onchange={(e) => onSelectDisplay(Number(e.currentTarget.value))}
			>
				{#each displays as d}
					<option value={d.id}>{d.label} ({d.bounds.width}×{d.bounds.height})</option>
				{/each}
			</select>
			<button class="btn-sm" onclick={onOpenOutputFullscreen} disabled={status !== 'running'} title="Open fullscreen on this screen">
				⛶ Fullscreen
			</button>
		</div>
	{:else if !isElectron}
		<button class="btn-sm" onclick={onOpenOutputFullscreen} disabled={status !== 'running'} style="margin-top:6px;width:100%" title="Fullscreen (press F to exit)">
			⛶ Fullscreen
		</button>
	{/if}
	{#if outputOpen && !isElectron}
		<span class="label" style="color:var(--info)">Output window open — use as OBS Browser Source</span>
	{/if}
	{#if showStreamPanel && isElectron}
		<div class="stream-panel">
			{#if platform === 'linux'}
				<button class="stream-btn" class:stream-btn--on={v4l2Active} onclick={onToggleV4l2}
					title={v4l2Active ? 'Stop V4L2' : 'Start V4L2 (virtual webcam)'}>
					V4L2 {v4l2Active ? '●' : '○'}
				</button>
			{/if}
			<button class="stream-btn stream-btn--ndi" class:stream-btn--on={ndiActive} onclick={onToggleNdi}
				title={ndiActive ? 'Stop NDI' : 'Start NDI'}>
				NDI {ndiActive ? '●' : '○'}
			</button>
			{#if platform === 'win32'}
				<button class="stream-btn stream-btn--spout" class:stream-btn--on={spoutActive} onclick={onToggleSpout}
					title={spoutActive ? 'Stop Spout' : 'Start Spout'}>
					SPOUT {spoutActive ? '●' : '○'}
				</button>
			{/if}
			<button class="stream-btn stream-btn--record" class:stream-btn--on={recordActive} onclick={onToggleRecord}
				title={recordActive ? 'Stop recording' : 'Record output to a local file'}>
				⏺ REC {recordActive ? '●' : '○'}
			</button>
			{#if v4l2Error}<div class="stream-error">{v4l2Error}</div>{/if}
			{#if ndiError}<div class="stream-error">{ndiError}</div>{/if}
			{#if spoutError}<div class="stream-error">{spoutError}</div>{/if}
			{#if recordError}<div class="stream-error">{recordError}</div>{/if}
			{#if recordActive && recordPath}<div class="stream-path" title={recordPath}>→ {recordPath}</div>{/if}
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

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.btn-sm {
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: border-color var(--t-fast), color var(--t-fast);
	}

	.btn-sm:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }
	.btn-sm:disabled { opacity: 0.3; cursor: not-allowed; }

	.btn-output {
		width: 100%;
		background: linear-gradient(135deg, rgba(0,229,255,0.08), rgba(180,79,255,0.08));
		color: var(--cyan); border: 1px solid #004455;
		border-radius: 6px; padding: 0.45rem; font-size: 12px; font-weight: 600;
		cursor: pointer; letter-spacing: 0.03em;
		transition: all 0.15s;
		box-shadow: 0 0 12px rgba(0,229,255,0.1);
	}

	.btn-output:hover:not(:disabled) {
		background: linear-gradient(135deg, rgba(0,229,255,0.14), rgba(180,79,255,0.14));
		box-shadow: 0 0 20px rgba(0,229,255,0.25);
		border-color: var(--cyan);
	}

	.btn-output:disabled { opacity: 0.3; cursor: not-allowed; }

	.output-row {
		display: flex;
		gap: 6px;
		align-items: center;
	}

	.btn-stream {
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 700;
		padding: 4px 8px;
		cursor: pointer;
		transition: all 0.15s;
	}
	.btn-stream:hover { border-color: #aaa; color: #aaa; }
	.btn-stream.stream-active { border-color: var(--info); color: var(--info); }

	.stream-panel {
		margin-top: 6px;
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		align-items: center;
	}

	.stream-btn {
		background: rgba(0,0,0,0.4);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 700;
		letter-spacing: 0.05em;
		padding: 4px 10px;
		cursor: pointer;
		transition: all 0.15s;
	}
	.stream-btn:hover { border-color: #aaa; color: #aaa; }
	.stream-btn--on { border-color: currentColor; }
	.stream-btn--ndi { color: var(--text-muted); }
	.stream-btn--ndi:hover, .stream-btn--ndi.stream-btn--on { border-color: var(--warn); color: var(--warn); background: var(--warn-dim, rgba(255,140,0,0.1)); }
	.stream-btn--spout { color: var(--text-muted); }
	.stream-btn--spout:hover, .stream-btn--spout.stream-btn--on { border-color: var(--violet); color: var(--violet); background: var(--violet-dim); }
	.stream-btn.stream-btn--on:not(.stream-btn--ndi):not(.stream-btn--spout):not(.stream-btn--record) { border-color: var(--cyan); color: var(--cyan); background: var(--cyan-dim); }
	.stream-btn--record { color: var(--text-muted); }
	.stream-btn--record:hover { border-color: var(--error); color: var(--error); background: rgba(255,0,0,0.1); }
	/* A color-only change is too easy to miss for "is it actually recording" —
	   blink so it's unmistakable at a glance. */
	.stream-btn--record.stream-btn--on {
		border-color: var(--error); color: var(--error); background: rgba(255,0,0,0.15);
		animation: rec-blink 1s step-start infinite;
	}
	@keyframes rec-blink {
		50% { background: rgba(255,0,0,0.4); box-shadow: 0 0 10px rgba(255,0,0,0.6); }
	}

	.stream-error {
		width: 100%;
		font-size: 10px;
		color: var(--error);
	}

	.stream-path {
		width: 100%;
		font-size: 10px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
