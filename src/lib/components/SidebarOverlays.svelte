<script lang="ts">
	import type { Overlay } from '$lib/engine/overlay.js';
	import type { BeatTriggerConfig } from '$lib/engine/beat-trigger.js';
	import { clampBeatsPerChange, clampOffset } from '$lib/engine/beat-trigger.js';
	import type { PlaylistMode } from '$lib/engine/playlist.js';

	interface Props {
		overlays: Overlay[];
		onAddOverlays: (e: Event) => void;
		onAddText: () => string;
		onRemoveOverlay: (id: string) => void;
		onUpdateOverlay: (id: string, patch: Partial<Overlay>) => void;
		overlayQueueEnabled: boolean;
		overlayQueueMode: PlaylistMode;
		overlayQueueTrigger: BeatTriggerConfig;
		onToggleOverlayQueue: () => void;
		onOverlayQueueModeChange: (mode: PlaylistMode) => void;
		onOverlayQueueTriggerChange: (patch: Partial<BeatTriggerConfig>) => void;
		onOverlayQueueNext: () => void;
		onOverlayQueuePrev: () => void;
	}

	let {
		overlays, onAddOverlays, onAddText, onRemoveOverlay, onUpdateOverlay,
		overlayQueueEnabled, overlayQueueMode, overlayQueueTrigger,
		onToggleOverlayQueue, onOverlayQueueModeChange, onOverlayQueueTriggerChange,
		onOverlayQueueNext, onOverlayQueuePrev,
	}: Props = $props();

	let expandedOverlayId = $state<string | null>(null);

	const BLEND_MODES = ['screen', 'normal', 'plus-lighter', 'multiply', 'overlay', 'hard-light'];
	const FONT_FAMILIES = [
		['sans', 'Sans'], ['serif', 'Serif'], ['mono', 'Mono'], ['impact', 'Impact'], ['comic', 'Comic'],
	] as const;

	function handleAddText() {
		expandedOverlayId = onAddText();
	}
</script>

<!-- Overlays -->
<div class="controls-section">
	<div class="pl-header">
		<span class="label">Overlays ({overlays.length})</span>
		<div style="display:flex; gap:4px">
			<button class="btn-sm" onclick={handleAddText} title="Add a text overlay">+ Text</button>
			<label class="btn-sm file-label" title="Add an image or video (sprite)">
				+ Sprite
				<input type="file" accept="image/*,video/*" multiple onchange={onAddOverlays} style="display:none" />
			</label>
		</div>
	</div>
	{#if overlays.length === 0}
		<p class="hint">Drag an image onto the visualizer, or click + Sprite / + Text</p>
	{/if}
	<ul class="overlay-list">
		{#each overlays as ov (ov.id)}
			<li class="overlay-item">
				<div class="overlay-row">
					<button class="overlay-name" onclick={() => expandedOverlayId = expandedOverlayId === ov.id ? null : ov.id}>
						{ov.name}
					</button>
					<button class="btn-sm pl-btn" class:active={ov.beatReactive} onclick={() => onUpdateOverlay(ov.id, { beatReactive: !ov.beatReactive })} title="Beat reactive">♩</button>
					<button class="btn-sm pl-btn" class:active={ov.inQueue} onclick={() => onUpdateOverlay(ov.id, { inQueue: !ov.inQueue })} title="Include in the auto-cycling queue">▤</button>
					<button class="pl-remove" onclick={() => onRemoveOverlay(ov.id)} title="Delete">×</button>
				</div>
				{#if expandedOverlayId === ov.id}
					<div class="overlay-controls">
						{#if ov.kind === 'text'}
							<label class="ov-label ov-label--stack">Content
								<textarea class="ov-textarea" rows="2" value={ov.text} oninput={(e) => onUpdateOverlay(ov.id, { text: (e.target as HTMLTextAreaElement).value })}></textarea>
							</label>
							<label class="ov-label">Font
								<select class="ov-select" value={ov.fontFamily} onchange={(e) => onUpdateOverlay(ov.id, { fontFamily: (e.target as HTMLSelectElement).value as Overlay['fontFamily'] })}>
									{#each FONT_FAMILIES as [value, label]}
										<option {value}>{label}</option>
									{/each}
								</select>
							</label>
							<label class="ov-label">Color
								<input type="color" class="ov-color" value={ov.color} oninput={(e) => onUpdateOverlay(ov.id, { color: (e.target as HTMLInputElement).value })} />
							</label>
							<label class="ov-label">Size
								<input type="range" min="2" max="20" step="0.5" value={ov.fontSize} oninput={(e) => onUpdateOverlay(ov.id, { fontSize: +(e.target as HTMLInputElement).value })} />
							</label>
						{/if}
						<label class="ov-label">Opacity
							<input type="range" min="0" max="1" step="0.01" value={ov.opacity} oninput={(e) => onUpdateOverlay(ov.id, { opacity: +(e.target as HTMLInputElement).value })} />
						</label>
						<label class="ov-label">Scale
							<input type="range" min="0.05" max="4" step="0.05" value={ov.scale} oninput={(e) => onUpdateOverlay(ov.id, { scale: +(e.target as HTMLInputElement).value })} />
						</label>
						<label class="ov-label">X
							<input type="range" min="0" max="1" step="0.01" value={ov.x} oninput={(e) => onUpdateOverlay(ov.id, { x: +(e.target as HTMLInputElement).value })} />
						</label>
						<label class="ov-label">Y
							<input type="range" min="0" max="1" step="0.01" value={ov.y} oninput={(e) => onUpdateOverlay(ov.id, { y: +(e.target as HTMLInputElement).value })} />
						</label>
						{#if ov.kind !== 'text'}
							<label class="ov-label">Rotation
								<input type="range" min="-180" max="180" step="1" value={ov.rotation} oninput={(e) => onUpdateOverlay(ov.id, { rotation: +(e.target as HTMLInputElement).value })} />
							</label>
						{/if}
						<label class="ov-label">Spin
							<input type="range" min="-180" max="180" step="1" value={ov.spin} oninput={(e) => onUpdateOverlay(ov.id, { spin: +(e.target as HTMLInputElement).value })} />
						</label>
						<label class="ov-label">Drift X
							<input type="range" min="-1" max="1" step="0.05" value={ov.driftX} oninput={(e) => onUpdateOverlay(ov.id, { driftX: +(e.target as HTMLInputElement).value })} />
						</label>
						<label class="ov-label">Drift Y
							<input type="range" min="-1" max="1" step="0.05" value={ov.driftY} oninput={(e) => onUpdateOverlay(ov.id, { driftY: +(e.target as HTMLInputElement).value })} />
						</label>
						<label class="ov-label">Blend
							<select class="ov-select" value={ov.blendMode} onchange={(e) => onUpdateOverlay(ov.id, { blendMode: (e.target as HTMLSelectElement).value })}>
								{#each BLEND_MODES as mode}
									<option value={mode}>{mode}</option>
								{/each}
							</select>
						</label>
					</div>
				{/if}
			</li>
		{/each}
	</ul>
	<div class="beat-trigger-row">
		<button class="btn-sm pl-btn" class:active={overlayQueueEnabled} onclick={onToggleOverlayQueue} title="Play/pause the overlay queue">{overlayQueueEnabled ? '⏸' : '▶'}</button>
		<button class="btn-sm" onclick={onOverlayQueuePrev} title="Previous overlay">◀</button>
		<button class="btn-sm" onclick={onOverlayQueueNext} title="Next overlay">▶</button>
		<select class="beats-select" value={overlayQueueMode} onchange={(e) => onOverlayQueueModeChange((e.target as HTMLSelectElement).value as PlaylistMode)}>
			<option value="sequential">Sequential</option>
			<option value="shuffle">Shuffle</option>
		</select>
	</div>
	<div class="beat-trigger-row">
		<button class="btn-sm" onclick={() => onOverlayQueueTriggerChange({ beatsPerChange: clampBeatsPerChange(overlayQueueTrigger.beatsPerChange / 2) })}>÷2</button>
		<input class="bt-num" type="number" min="1" max="64"
			value={overlayQueueTrigger.beatsPerChange}
			onchange={(e) => onOverlayQueueTriggerChange({ beatsPerChange: clampBeatsPerChange(+(e.target as HTMLInputElement).value) })} />
		<button class="btn-sm" onclick={() => onOverlayQueueTriggerChange({ beatsPerChange: clampBeatsPerChange(overlayQueueTrigger.beatsPerChange * 2) })}>×2</button>
		<span class="bt-sep">off</span>
		<input class="bt-num" type="number" min="0" max={overlayQueueTrigger.beatsPerChange - 1}
			value={overlayQueueTrigger.offset}
			onchange={(e) => onOverlayQueueTriggerChange({ offset: clampOffset(+(e.target as HTMLInputElement).value, overlayQueueTrigger.beatsPerChange) })} />
		<button class="btn-sm" class:active={overlayQueueTrigger.mode === 'volume-peak'}
			onclick={() => onOverlayQueueTriggerChange({ mode: overlayQueueTrigger.mode === 'beat' ? 'volume-peak' : 'beat' })}
			title="Toggle Beat / Volume trigger">{overlayQueueTrigger.mode === 'beat' ? '♩' : '🔊'}</button>
		{#if overlayQueueTrigger.mode === 'volume-peak'}
			<input class="bt-range" type="range" min="0" max="1" step="0.01"
				value={overlayQueueTrigger.sensitivity}
				oninput={(e) => onOverlayQueueTriggerChange({ sensitivity: +(e.target as HTMLInputElement).value })} />
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

	.hint { margin: 0.2rem 0; font-size: 11px; color: #aaaacc; line-height: 1.5; }

	.pl-header { display: flex; align-items: center; justify-content: space-between; }

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

	.file-label { display: inline-block; cursor: pointer; }

	.pl-btn { padding: 0.22rem 0.4rem; font-size: 11px; }

	.pl-remove {
		background: none; border: none; color: #33335a;
		cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; flex-shrink: 0;
		transition: color 0.1s;
	}

	.pl-remove:hover { color: var(--accent); }

	/* Overlay panel */
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

	.overlay-controls {
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 4px 6px 5px;
		border-top: 1px solid #161640;
		background: #06061a;
	}

	.ov-label {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		font-size: 10px;
		color: var(--text-muted);
	}

	.ov-label--stack {
		flex-direction: column;
		align-items: stretch;
		gap: 2px;
	}

	.ov-label input[type="range"] {
		flex: 1;
		height: 3px;
		accent-color: var(--violet);
		cursor: pointer;
	}

	.ov-textarea {
		background: var(--bg-base);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 11px;
		padding: 3px 5px;
		resize: vertical;
		font-family: inherit;
	}
	.ov-textarea:focus { outline: none; border-color: var(--violet); }

	.ov-color {
		width: 28px;
		height: 18px;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: none;
		cursor: pointer;
	}

	.ov-select {
		flex: 1;
		background: var(--bg-base);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 10px;
		padding: 1px 3px;
		cursor: pointer;
	}
	.ov-select:focus { outline: none; border-color: var(--violet); }

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

	.bt-sep { font-size: 9px; color: var(--text-muted); text-transform: uppercase; }

	.bt-num {
		background: var(--bg-base); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: 5px;
		padding: 0.15rem 0.25rem; font-size: 11px; width: 36px;
	}

	.bt-range { flex: 1; min-width: 40px; accent-color: var(--violet); cursor: pointer; }
</style>
