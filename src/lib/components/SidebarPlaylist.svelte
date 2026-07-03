<script lang="ts">
	import type { PlaylistMode } from '$lib/engine/playlist.js';
	import type { BeatTriggerConfig } from '$lib/engine/beat-trigger.js';
	import { clampBeatsPerChange, clampOffset } from '$lib/engine/beat-trigger.js';

	interface Props {
		playlistMode: PlaylistMode;
		playlistIntervalSec: number;
		beatSyncA: boolean;
		beatSyncB: boolean;
		autoXfade: boolean;
		beatsPerChange: number;
		beatTriggerA: BeatTriggerConfig;
		beatTriggerB: BeatTriggerConfig;
		detectedBpm: number;
		manualBpm: number;
		playlistAItems: string[];
		playlistBItems: string[];
		playlistAPlaying: boolean;
		playlistBPlaying: boolean;
		audioRunning: boolean;
		presetA: string;
		presetB: string;
		lockA: boolean;
		lockB: boolean;
		onModeChange: (m: PlaylistMode) => void;
		onIntervalChange: (s: number) => void;
		onBeatsPerChangeChange: (n: number) => void;
		onBeatTriggerAChange: (patch: Partial<BeatTriggerConfig>) => void;
		onBeatTriggerBChange: (patch: Partial<BeatTriggerConfig>) => void;
		onTapTempo: () => void;
		onClearManualBpm: () => void;
		onToggleBeatSyncA: () => void;
		onToggleBeatSyncB: () => void;
		onToggleAutoXfade: () => void;
		onTogglePlaylistA: () => void;
		onTogglePlaylistB: () => void;
		onPlaylistNext: (deck: 'A' | 'B') => void;
		onPlaylistPrev: (deck: 'A' | 'B') => void;
		onRemoveFromPlaylistA: (name: string) => void;
		onRemoveFromPlaylistB: (name: string) => void;
		onToggleLockA: () => void;
		onToggleLockB: () => void;
		onExportPlaylists: () => void;
		onImportPlaylists: (e: Event) => void;
	}

	let {
		playlistMode,
		playlistIntervalSec,
		beatSyncA,
		beatSyncB,
		autoXfade,
		beatsPerChange,
		beatTriggerA,
		beatTriggerB,
		detectedBpm,
		manualBpm,
		playlistAItems,
		playlistBItems,
		playlistAPlaying,
		playlistBPlaying,
		audioRunning,
		presetA,
		presetB,
		lockA,
		lockB,
		onModeChange,
		onIntervalChange,
		onBeatsPerChangeChange,
		onBeatTriggerAChange,
		onBeatTriggerBChange,
		onTapTempo,
		onClearManualBpm,
		onToggleBeatSyncA,
		onToggleBeatSyncB,
		onToggleAutoXfade,
		onTogglePlaylistA,
		onTogglePlaylistB,
		onPlaylistNext,
		onPlaylistPrev,
		onRemoveFromPlaylistA,
		onRemoveFromPlaylistB,
		onToggleLockA,
		onToggleLockB,
		onExportPlaylists,
		onImportPlaylists,
	}: Props = $props();
</script>

<!-- Playlist -->
<div class="controls-section pl-section">
	<div class="pl-header">
		<span class="label">Playlist</span>
		<div class="btn-row">
			<button class="btn-sm" class:active={playlistMode === 'sequential'} onclick={() => onModeChange('sequential')}>Seq</button>
			<button class="btn-sm" class:active={playlistMode === 'shuffle'} onclick={() => onModeChange('shuffle')}>Shuffle</button>
			<button class="btn-sm" onclick={onExportPlaylists} title="Exporter les playlists">⬇</button>
			<label class="btn-sm file-label" title="Importer des playlists">⬆<input type="file" accept=".json" onchange={onImportPlaylists} style="display:none" /></label>
		</div>
	</div>
	<div class="crossfader-row">
		<span class="cf-label">⏱</span>
		<input class="crossfader" type="range" min="2" max="120" step="1"
			value={playlistIntervalSec}
			oninput={(e) => onIntervalChange(+(e.target as HTMLInputElement).value)} />
		<span class="cf-label bright">{playlistIntervalSec}s</span>
	</div>

	<!-- Beat sync -->
	{#if audioRunning}
		<div class="beat-sync-row">
			<span class="bpm-display" class:manual={manualBpm > 0}>♩ {manualBpm > 0 ? manualBpm : detectedBpm > 0 ? detectedBpm : '—'}</span>
			<button class="btn-sm tap-btn" onclick={onTapTempo} title="Tap tempo">TAP</button>
			{#if manualBpm > 0}
				<button class="btn-sm" onclick={onClearManualBpm} title="Clear manual BPM">✕</button>
			{/if}
			<select class="beats-select" value={beatsPerChange} onchange={(e) => onBeatsPerChangeChange(+(e.target as HTMLSelectElement).value)}>
				<option value={4}>4</option>
				<option value={8}>8</option>
				<option value={16}>16</option>
				<option value={32}>32</option>
			</select>
			<button class="btn-sm pl-btn" class:active={beatSyncA} onclick={onToggleBeatSyncA} title="Beat-sync Deck A">A</button>
			<button class="btn-sm pl-btn" class:active={beatSyncB} onclick={onToggleBeatSyncB} title="Beat-sync Deck B">B</button>
			<button class="btn-sm pl-btn" class:active={autoXfade} onclick={onToggleAutoXfade} title="Auto-cut crossfader on beat">⇄</button>
		</div>
		{#if beatSyncA}
			<div class="beat-trigger-row">
				<span class="bt-label">A</span>
				<button class="btn-sm" onclick={() => onBeatTriggerAChange({ beatsPerChange: clampBeatsPerChange(beatTriggerA.beatsPerChange / 2) })}>÷2</button>
				<input class="bt-num" type="number" min="1" max="64"
					value={beatTriggerA.beatsPerChange}
					onchange={(e) => onBeatTriggerAChange({ beatsPerChange: clampBeatsPerChange(+(e.target as HTMLInputElement).value) })} />
				<button class="btn-sm" onclick={() => onBeatTriggerAChange({ beatsPerChange: clampBeatsPerChange(beatTriggerA.beatsPerChange * 2) })}>×2</button>
				<span class="bt-sep">off</span>
				<input class="bt-num" type="number" min="0" max={beatTriggerA.beatsPerChange - 1}
					value={beatTriggerA.offset}
					onchange={(e) => onBeatTriggerAChange({ offset: clampOffset(+(e.target as HTMLInputElement).value, beatTriggerA.beatsPerChange) })} />
				<button class="btn-sm" class:active={beatTriggerA.mode === 'volume-peak'}
					onclick={() => onBeatTriggerAChange({ mode: beatTriggerA.mode === 'beat' ? 'volume-peak' : 'beat' })}
					title="Toggle Beat / Volume trigger">{beatTriggerA.mode === 'beat' ? '♩' : '🔊'}</button>
				{#if beatTriggerA.mode === 'volume-peak'}
					<input class="bt-range" type="range" min="0" max="1" step="0.01"
						value={beatTriggerA.sensitivity}
						oninput={(e) => onBeatTriggerAChange({ sensitivity: +(e.target as HTMLInputElement).value })} />
				{/if}
			</div>
		{/if}
		{#if beatSyncB}
			<div class="beat-trigger-row">
				<span class="bt-label">B</span>
				<button class="btn-sm" onclick={() => onBeatTriggerBChange({ beatsPerChange: clampBeatsPerChange(beatTriggerB.beatsPerChange / 2) })}>÷2</button>
				<input class="bt-num" type="number" min="1" max="64"
					value={beatTriggerB.beatsPerChange}
					onchange={(e) => onBeatTriggerBChange({ beatsPerChange: clampBeatsPerChange(+(e.target as HTMLInputElement).value) })} />
				<button class="btn-sm" onclick={() => onBeatTriggerBChange({ beatsPerChange: clampBeatsPerChange(beatTriggerB.beatsPerChange * 2) })}>×2</button>
				<span class="bt-sep">off</span>
				<input class="bt-num" type="number" min="0" max={beatTriggerB.beatsPerChange - 1}
					value={beatTriggerB.offset}
					onchange={(e) => onBeatTriggerBChange({ offset: clampOffset(+(e.target as HTMLInputElement).value, beatTriggerB.beatsPerChange) })} />
				<button class="btn-sm" class:active={beatTriggerB.mode === 'volume-peak'}
					onclick={() => onBeatTriggerBChange({ mode: beatTriggerB.mode === 'beat' ? 'volume-peak' : 'beat' })}
					title="Toggle Beat / Volume trigger">{beatTriggerB.mode === 'beat' ? '♩' : '🔊'}</button>
				{#if beatTriggerB.mode === 'volume-peak'}
					<input class="bt-range" type="range" min="0" max="1" step="0.01"
						value={beatTriggerB.sensitivity}
						oninput={(e) => onBeatTriggerBChange({ sensitivity: +(e.target as HTMLInputElement).value })} />
				{/if}
			</div>
		{/if}
	{/if}

	<!-- Deck A playlist -->
	<div class="pl-deck">
		<div class="pl-deck-header">
			<span class="pl-deck-label">A</span>
			<span class="label">{playlistAItems.length} preset{playlistAItems.length !== 1 ? 's' : ''}</span>
			<div class="pl-transport">
				<button class="btn-sm pl-btn" onclick={() => onPlaylistPrev('A')} disabled={!audioRunning || playlistAItems.length === 0}>⏮</button>
				<button class="btn-sm pl-btn" class:active={playlistAPlaying} onclick={onTogglePlaylistA} disabled={!audioRunning || playlistAItems.length === 0}>
					{playlistAPlaying ? '⏹' : '▶'}
				</button>
				<button class="btn-sm pl-btn" onclick={() => onPlaylistNext('A')} disabled={!audioRunning || playlistAItems.length === 0}>⏭</button>
				<button class="btn-sm pl-btn lock-btn" class:locked={lockA} onclick={onToggleLockA} title={lockA ? 'Unlock deck A' : 'Lock deck A'}>🔒</button>
			</div>
		</div>
		{#if playlistAItems.length > 0}
			<ul class="pl-items">
				{#each playlistAItems as name (name)}
					<li class="pl-item">
						<span class="pl-item-name" class:pl-active={name === presetA}>{name}</span>
						<button class="pl-remove" onclick={() => onRemoveFromPlaylistA(name)}>×</button>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="pl-empty">Use +A in the preset list below</p>
		{/if}
	</div>

	<!-- Deck B playlist -->
	<div class="pl-deck">
		<div class="pl-deck-header">
			<span class="pl-deck-label">B</span>
			<span class="label">{playlistBItems.length} preset{playlistBItems.length !== 1 ? 's' : ''}</span>
			<div class="pl-transport">
				<button class="btn-sm pl-btn" onclick={() => onPlaylistPrev('B')} disabled={!audioRunning || playlistBItems.length === 0}>⏮</button>
				<button class="btn-sm pl-btn" class:active={playlistBPlaying} onclick={onTogglePlaylistB} disabled={!audioRunning || playlistBItems.length === 0}>
					{playlistBPlaying ? '⏹' : '▶'}
				</button>
				<button class="btn-sm pl-btn" onclick={() => onPlaylistNext('B')} disabled={!audioRunning || playlistBItems.length === 0}>⏭</button>
				<button class="btn-sm pl-btn lock-btn" class:locked={lockB} onclick={onToggleLockB} title={lockB ? 'Unlock deck B' : 'Lock deck B'}>🔒</button>
			</div>
		</div>
		{#if playlistBItems.length > 0}
			<ul class="pl-items">
				{#each playlistBItems as name (name)}
					<li class="pl-item">
						<span class="pl-item-name" class:pl-active={name === presetB}>{name}</span>
						<button class="pl-remove" onclick={() => onRemoveFromPlaylistB(name)}>×</button>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="pl-empty">Use +B in the preset list below</p>
		{/if}
	</div>
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

	.btn-row { display: flex; gap: 0.4rem; }

	.tap-btn { font-weight: 700; letter-spacing: 0.05em; }
	.bpm-display.manual { color: var(--violet); text-shadow: 0 0 8px rgba(180,79,255,0.5); }
	.lock-btn { opacity: 0.35; }
	.lock-btn.locked { opacity: 1; color: var(--accent); }

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

	.pl-section { gap: 0.5rem; }

	.pl-header { display: flex; align-items: center; justify-content: space-between; }

	.pl-deck {
		background: var(--bg-surface); border: 1px solid #161640;
		border-radius: 6px; padding: 0.4rem;
		display: flex; flex-direction: column; gap: 0.3rem;
	}

	.pl-deck-header { display: flex; align-items: center; gap: 0.4rem; }

	.pl-deck-label {
		font-size: 13px; font-weight: 800; width: 14px;
		color: var(--accent); text-shadow: 0 0 8px rgba(255,45,120,0.7);
	}

	.pl-transport { display: flex; gap: 0.25rem; margin-left: auto; }

	.pl-items {
		list-style: none; max-height: 80px; overflow-y: auto;
		display: flex; flex-direction: column; gap: 1px;
	}

	.pl-item { display: flex; align-items: center; gap: 0.25rem; }

	.pl-item-name {
		flex: 1; font-size: 11px; color: #666690;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.pl-item-name.pl-active { color: var(--cyan); text-shadow: 0 0 6px rgba(0,229,255,0.5); }

	.pl-remove {
		background: none; border: none; color: #33335a;
		cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; flex-shrink: 0;
		transition: color 0.1s;
	}

	.pl-remove:hover { color: var(--accent); }

	.pl-empty { font-size: 10px; color: #2a2a50; font-style: italic; }

	/* Beat sync */
	.beat-sync-row {
		display: flex; align-items: center; gap: 0.4rem;
		padding: 0.3rem 0.5rem;
		background: #08081e; border: 1px solid #141440;
		border-radius: 6px;
	}

	.bpm-display {
		font-size: 12px; font-weight: 700; color: var(--violet);
		text-shadow: 0 0 10px rgba(180,79,255,0.6);
		min-width: 48px; font-family: 'Courier New', monospace;
		flex-shrink: 0;
	}

	.beats-select {
		background: var(--bg-base); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: 5px;
		padding: 0.2rem 0.3rem; font-size: 10px; cursor: pointer;
		-webkit-appearance: none; appearance: none;
		flex: 1; min-width: 0;
	}

	.beats-select:focus { outline: none; border-color: var(--violet); }

	.beat-trigger-row {
		display: flex; align-items: center; gap: 0.3rem;
		padding: 0.25rem 0.5rem;
		background: #050514; border: 1px solid #101034;
		border-radius: 6px;
	}

	.bt-label {
		font-size: 11px; font-weight: 800; width: 12px;
		color: var(--accent); text-shadow: 0 0 6px rgba(255,45,120,0.5);
	}

	.bt-sep { font-size: 9px; color: var(--text-muted); text-transform: uppercase; }

	.bt-num {
		background: var(--bg-base); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: 5px;
		padding: 0.15rem 0.25rem; font-size: 11px; width: 36px;
	}

	.bt-range { flex: 1; min-width: 40px; accent-color: var(--violet); cursor: pointer; }
</style>
