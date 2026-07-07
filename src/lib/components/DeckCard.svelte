<script lang="ts">
  interface Props {
    letter: string
    canvas: HTMLCanvasElement | undefined
    presetName: string
    isActive: boolean
    isLive: boolean
    onSelect: () => void
    bus?: 'A' | 'B' | 'off'
    running?: boolean
    onCycleBus?: () => void
    onToggleRun?: () => void
  }

  let { letter, canvas, presetName, isActive, isLive, onSelect, bus, running, onCycleBus, onToggleRun }: Props = $props()

  const BUS_LABELS: Record<'A' | 'B' | 'off', string> = { A: '● A', B: '● B', off: '○ —' }
  const BUS_COLORS: Record<'A' | 'B' | 'off', string> = { A: 'var(--accent)', B: 'var(--live)', off: 'var(--text-muted)' }

  let videoEl: HTMLVideoElement | undefined = $state()

  $effect(() => {
    if (!canvas || !videoEl) return
    try {
      const stream = (canvas as any).captureStream(10) as MediaStream
      videoEl.srcObject = stream
    } catch {
      // captureStream not available (Firefox, Safari) — silent fallback
    }
  })
</script>

<div
  class="deck-card"
  class:deck-card--active={isActive}
  onclick={onSelect}
  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onSelect() } }}
  role="button"
  tabindex="0"
>
  <div class="deck-card__header">
    <span class="deck-card__letter">{letter}</span>
    {#if isLive}
      <span class="deck-card__live">●</span>
    {/if}
  </div>

  <div class="deck-card__preview">
    <!-- svelte-ignore a11y_media_has_caption -->
    <video
      bind:this={videoEl}
      class="deck-card__video"
      muted
      autoplay
      playsinline
    ></video>
    {#if !presetName}
      <span class="deck-card__empty">No preset</span>
    {/if}
  </div>

  <div class="deck-card__name" title={presetName || ''}>
    {presetName ? presetName.split('/').at(-1) ?? presetName : '—'}
  </div>

  {#if onCycleBus !== undefined}
    <div class="deck-card__footer">
      <button
        class="deck-card__bus"
        style:color={BUS_COLORS[bus ?? 'off']}
        onclick={(e) => { e.stopPropagation(); onCycleBus?.() }}
        type="button"
        title="Bus: {bus ?? 'off'} — click to change"
      >{BUS_LABELS[bus ?? 'off']}</button>
      <button
        class="deck-card__run"
        class:deck-card__run--stop={running}
        onclick={(e) => { e.stopPropagation(); onToggleRun?.() }}
        type="button"
        aria-label={running ? 'Stop deck' : 'Start deck'}
      >{running ? '■' : '▶'}</button>
    </div>
  {/if}
</div>

<style>
  .deck-card {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 6px);
    cursor: pointer;
    padding: 0;
    overflow: hidden;
    transition: border-color var(--t-fast, 150ms ease);
    text-align: left;
  }

  .deck-card:hover {
    border-color: var(--accent);
  }

  .deck-card--active {
    border-color: var(--accent);
    box-shadow: 0 0 12px var(--accent-glow), inset 0 0 8px rgba(255, 45, 120, 0.06);
  }

  .deck-card__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-1) var(--sp-2) 2px;
  }

  .deck-card__letter {
    font-size: 14px;
    font-weight: 800;
    color: var(--accent);
  }

  .deck-card__live {
    font-size: 8px;
    color: var(--live);
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  .deck-card__preview {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    background: var(--bg-base);
    overflow: hidden;
  }

  .deck-card__video {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .deck-card__empty {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .deck-card__name {
    padding: var(--sp-1) var(--sp-2) 6px;
    font-size: 9px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .deck-card__footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 3px 6px;
    border-top: 1px solid var(--border-subtle);
  }

  .deck-card__bus {
    font-size: 9px;
    font-weight: 700;
    background: none;
    border: none;
    cursor: pointer;
    letter-spacing: 0.06em;
    padding: 0;
    transition: opacity var(--t-fast);
  }

  .deck-card__bus:hover { opacity: 0.7; }

  .deck-card__run {
    font-size: 9px;
    background: none;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    color: var(--live);
    width: 20px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    cursor: pointer;
    transition: border-color var(--t-fast), color var(--t-fast);
  }

  .deck-card__run:hover { border-color: var(--live); }
  .deck-card__run--stop:hover { border-color: var(--accent); color: var(--accent); }
</style>
