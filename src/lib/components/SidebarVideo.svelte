<script lang="ts">
	import type { VideoClipMeta } from '$lib/engine/video-store.js';

	interface Props {
		videoEnabled: boolean;
		videoOpacity: number;
		videoAdvance: 'shuffle' | 'sequential' | 'manual';
		videoBeatsPerCut: number;
		videoSeparatorBeats: number;
		vrCut: boolean;
		vrFlash: boolean;
		vrWarp: boolean;
		vrHue: boolean;
		currentClipIndex: number;
		allClips: VideoClipMeta[];
		selectedClipKeys: string[];
		clipKey: (clip: VideoClipMeta) => string;
		liveActive: boolean;
		liveLabel: string;
		ndiActive: boolean;
		ndiSourceLabel: string;
		ndiSources: Array<{ name: string; urlAddress: string }>;
		onToggleVideo: () => void;
		onToggleLiveCamera: () => void;
		onFindNdiSources: () => void;
		onSelectNdiSource: (name: string, urlAddress: string) => void;
		onClearNdiSource: () => void;
		onOpacityChange: (v: number) => void;
		onAdvanceChange: (v: 'shuffle' | 'sequential' | 'manual') => void;
		onBeatsPerCutChange: (v: number) => void;
		onSeparatorBeatsChange: (v: number) => void;
		onToggleVrCut: () => void;
		onToggleVrFlash: () => void;
		onToggleVrWarp: () => void;
		onToggleVrHue: () => void;
		onSelectClip: (i: number) => void;
		onRemoveClip: (index: number) => void;
		onAddVideo: (e: Event) => void;
		onToggleClipSelection: (key: string) => void;
		onClearClipSelection: () => void;
		onSelectAllClips: () => void;
		onRemoveSelectedClips: () => void;
	}

	let {
		videoEnabled,
		videoOpacity,
		videoAdvance,
		videoBeatsPerCut,
		videoSeparatorBeats,
		vrCut,
		vrFlash,
		vrWarp,
		vrHue,
		currentClipIndex,
		allClips,
		selectedClipKeys,
		clipKey,
		liveActive,
		liveLabel,
		ndiActive,
		ndiSourceLabel,
		ndiSources,
		onToggleVideo,
		onToggleLiveCamera,
		onFindNdiSources,
		onSelectNdiSource,
		onClearNdiSource,
		onOpacityChange,
		onAdvanceChange,
		onBeatsPerCutChange,
		onSeparatorBeatsChange,
		onToggleVrCut,
		onToggleVrFlash,
		onToggleVrWarp,
		onToggleVrHue,
		onSelectClip,
		onRemoveClip,
		onAddVideo,
		onToggleClipSelection,
		onClearClipSelection,
		onSelectAllClips,
		onRemoveSelectedClips,
	}: Props = $props();
</script>

<!-- Video loops -->
<div class="controls-section">
	<div class="pl-header">
		<span class="label">Video ({allClips.length})</span>
		<button class="btn-sm pl-btn" class:active={videoEnabled} onclick={onToggleVideo}>
			{videoEnabled ? 'ON' : 'OFF'}
		</button>
	</div>
	{#if videoEnabled}
		<div class="crossfader-row">
			<span class="cf-label">α</span>
			<input class="crossfader" type="range" min="0" max="1" step="0.01"
				value={videoOpacity}
				oninput={(e) => onOpacityChange(+(e.target as HTMLInputElement).value)} />
			<span class="cf-label bright">{Math.round(videoOpacity * 100)}%</span>
		</div>
		<div class="btn-row">
			<button class="btn-sm" class:active={videoAdvance === 'shuffle'} onclick={() => onAdvanceChange('shuffle')} disabled={liveActive || ndiActive}>Shuffle</button>
			<button class="btn-sm" class:active={videoAdvance === 'sequential'} onclick={() => onAdvanceChange('sequential')} disabled={liveActive || ndiActive}>Seq</button>
			<button class="btn-sm" class:active={videoAdvance === 'manual'} onclick={() => onAdvanceChange('manual')} disabled={liveActive || ndiActive}>Manual</button>
			<select class="beats-select" value={videoBeatsPerCut} onchange={(e) => onBeatsPerCutChange(+(e.target as HTMLSelectElement).value)} disabled={videoAdvance === 'manual' || liveActive || ndiActive}>
				<option value={4}>4</option>
				<option value={8}>8</option>
				<option value={16}>16</option>
				<option value={32}>32</option>
				<option value={64}>64</option>
			</select>
		</div>
		<div class="btn-row">
			<span class="cf-label bright" title="Flash the layer every N beats — marks each loop/cut restart">Sep</span>
			<select class="beats-select" value={videoSeparatorBeats} onchange={(e) => onSeparatorBeatsChange(+(e.target as HTMLSelectElement).value)}>
				<option value={0}>0</option>
				<option value={1}>1</option>
				<option value={4}>4</option>
				<option value={8}>8</option>
				<option value={16}>16</option>
				<option value={32}>32</option>
				<option value={64}>64</option>
			</select>
		</div>
		<div class="btn-row">
			<button class="btn-sm pl-btn" class:active={vrCut} onclick={onToggleVrCut} disabled={videoAdvance === 'manual' || liveActive || ndiActive} title="Clip cut on the beat">✂ Cut</button>
			<button class="btn-sm pl-btn" class:active={vrFlash} onclick={onToggleVrFlash} title="Flash brightness on the beat">✦ Flash</button>
			<button class="btn-sm pl-btn" class:active={vrWarp} onclick={onToggleVrWarp} disabled={liveActive || ndiActive} title="Speed warp on the bass">⏩ Warp</button>
			<button class="btn-sm pl-btn" class:active={vrHue} onclick={onToggleVrHue} title="Hue rotate on the beat">🌈 Hue</button>
		</div>
	{/if}
	<div class="btn-row" style="margin-top:0.2rem">
		<button class="btn-sm pl-btn" class:active={liveActive} onclick={onToggleLiveCamera} disabled={ndiActive} title={liveActive ? `Live: ${liveLabel}` : 'Use a live camera as the video layer'}>
			📷 {liveActive ? (liveLabel || 'Camera') : 'Camera'}
		</button>
		<button class="btn-sm pl-btn" class:active={ndiActive} onclick={() => ndiActive ? onClearNdiSource() : onFindNdiSources()} disabled={liveActive} title={ndiActive ? `NDI: ${ndiSourceLabel}` : 'Use an NDI source as the video layer'}>
			🛰 {ndiActive ? (ndiSourceLabel || 'NDI') : 'NDI'}
		</button>
	</div>
	{#if !ndiActive && ndiSources.length > 0}
		<ul class="overlay-list">
			{#each ndiSources as s (s.name)}
				<li class="overlay-item">
					<div class="overlay-row">
						<button class="overlay-name" onclick={() => onSelectNdiSource(s.name, s.urlAddress)}>{s.name}</button>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
	<div class="pl-header" style="margin-top:0.2rem">
		<label class="btn-sm file-label" title="Add a video">
			+ Video
			<input type="file" accept="video/*" multiple onchange={onAddVideo} style="display:none" />
		</label>
	</div>
	{#if allClips.length === 0 && !liveActive && !ndiActive}
		<p class="hint">Drag a video onto the visualizer or click + Video</p>
	{/if}
	{#if allClips.length > 0}
		<div class="pl-header">
			<span class="hint" style="margin:0">{selectedClipKeys.length} in rotation</span>
			<button class="btn-sm pl-btn" onclick={onSelectAllClips}>Select All</button>
			<button class="btn-sm pl-btn" onclick={onClearClipSelection} disabled={selectedClipKeys.length === 0}>Clear</button>
			<button class="btn-sm pl-btn" onclick={onRemoveSelectedClips} disabled={selectedClipKeys.length === 0} title="Delete the checked clips from your library">🗑 Remove Selected</button>
		</div>
	{/if}
	<ul class="overlay-list">
		{#each allClips as clip, i (clip.ref.kind === 'user' ? clip.ref.id : clip.ref.src)}
			<li class="overlay-item">
				<div class="overlay-row">
					<input
						type="checkbox"
						class="pl-checkbox"
						checked={selectedClipKeys.includes(clipKey(clip))}
						onchange={() => onToggleClipSelection(clipKey(clip))}
						title="Include in auto-cut rotation"
					/>
					<button
						class="overlay-name"
						class:pl-active={i === currentClipIndex % allClips.length}
						onclick={() => onSelectClip(i)}
						title={clip.ref.kind === 'builtin' ? 'Built-in' : 'User'}
					>
						{clip.ref.kind === 'builtin' ? '📦 ' : ''}{clip.name}
					</button>
					{#if clip.ref.kind === 'user'}
						<button class="pl-remove" onclick={() => onRemoveClip(i)} title="Delete">×</button>
					{/if}
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

	.btn-row { display: flex; gap: 0.4rem; }

	.pl-header { display: flex; align-items: center; justify-content: space-between; }

	.crossfader-row { display: flex; align-items: center; gap: 0.4rem; }

	.cf-label {
		font-size: 11px; font-weight: 700; color: #33335a;
		width: 12px; text-align: center; transition: color 0.15s;
	}

	.cf-label.bright { color: var(--accent); text-shadow: 0 0 8px rgba(255,45,120,0.8); }

	.crossfader { flex: 1; accent-color: var(--accent); cursor: pointer; }

	.btn-sm {
		background: var(--bg-base); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: 5px;
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: all 0.12s;
	}

	.btn-sm:hover:not(:disabled) { background: #141436; color: #ddddf5; border-color: #3a3a6a; }

	.btn-sm.active {
		background: #1a0822; border-color: var(--accent); color: var(--accent);
		box-shadow: 0 0 8px rgba(255,45,120,0.25);
	}

	.btn-sm:disabled { opacity: 0.3; cursor: not-allowed; }

	.file-label { display: inline-block; cursor: pointer; }

	.pl-btn { padding: 0.22rem 0.4rem; font-size: 11px; }

	.pl-remove {
		background: none; border: none; color: #33335a;
		cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; flex-shrink: 0;
		transition: color 0.1s;
	}

	.pl-remove:hover { color: var(--accent); }

	.beats-select {
		background: var(--bg-base); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: 5px;
		padding: 0.2rem 0.3rem; font-size: 10px; cursor: pointer;
		-webkit-appearance: none; appearance: none;
		flex: 1; min-width: 0;
	}

	.beats-select:focus { outline: none; border-color: var(--violet); }

	/* Clip list (reuses overlay panel styles) */
	.overlay-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 2px;
		max-height: 200px;
		overflow-y: auto;
		scrollbar-width: thin;
		scrollbar-color: #2a2a5a transparent;
	}

	.overlay-item {
		background: var(--bg-surface);
		border: 1px solid #161640;
		border-radius: 5px;
		overflow: hidden;
		flex-shrink: 0;
	}

	.overlay-row {
		display: flex;
		align-items: center;
		gap: 3px;
		padding: 2px 4px;
	}

	.overlay-name {
		flex: 1;
		background: none;
		border: none;
		color: var(--text-secondary);
		font-size: 11px;
		cursor: pointer;
		text-align: left;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		padding: 2px 0;
		transition: color 0.1s;
	}
	.overlay-name:hover { color: var(--violet); }
	.overlay-name.pl-active { color: var(--cyan); text-shadow: 0 0 6px rgba(0,229,255,0.5); }

	.pl-checkbox { flex-shrink: 0; cursor: pointer; accent-color: var(--accent); }
</style>
