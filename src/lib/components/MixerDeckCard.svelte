<!-- src/lib/components/MixerDeckCard.svelte -->
<script lang="ts">
  interface Props {
    slot: number
    canvas: HTMLCanvasElement | undefined
    presetName: string
    running: boolean
    bus: 'A' | 'B' | 'off'
    isSelected: boolean
    onStart: () => void
    onStop: () => void
    onSelect: () => void
    onCycleBus: () => void
  }

  let { slot, canvas, presetName, running, bus, isSelected, onStart, onStop, onSelect, onCycleBus }: Props = $props()

  let videoEl: HTMLVideoElement | undefined = $state()

  $effect(() => {
    if (!canvas || !videoEl) return
    try {
      const stream = (canvas as HTMLCanvasElement & { captureStream: (fps: number) => MediaStream }).captureStream(10)
      videoEl.srcObject = stream
    } catch {
      // captureStream non supporté — preview absente
    }
  })

  const BUS_LABELS: Record<'A' | 'B' | 'off', string> = { A: '● A', B: '● B', off: '○ —' }
  const BUS_COLORS: Record<'A' | 'B' | 'off', string> = { A: '#ff2d78', B: '#00e096', off: '#44447a' }
</script>

<div
  class="mdeck"
  class:mdeck--selected={isSelected}
  onclick={onSelect}
  role="button"
  tabindex="0"
  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') onSelect() }}
>
  <div class="mdeck__header">
    <span class="mdeck__num">DECK {slot + 1}</span>
    <span class="mdeck__live" class:mdeck__live--on={running}>
      {running ? '● LIVE' : '○'}
    </span>
  </div>

  <div class="mdeck__preview">
    {#if canvas}
      <video
        bind:this={videoEl}
        class="mdeck__video"
        autoplay
        muted
        playsinline
      ></video>
    {:else}
      <div class="mdeck__idle">no signal</div>
    {/if}
  </div>

  <div class="mdeck__name" title={presetName || '—'}>
    {#if presetName}
      {presetName}
    {:else}
      <span class="mdeck__empty">— empty —</span>
    {/if}
  </div>

  <div class="mdeck__footer">
    <button
      class="mdeck__bus-btn"
      style:color={BUS_COLORS[bus]}
      onclick={(e) => { e.stopPropagation(); onCycleBus() }}
      type="button"
      title="Bus: {bus} → cliquer pour changer"
    >
      {BUS_LABELS[bus]}
    </button>
    <button
      class="mdeck__play"
      class:mdeck__play--stop={running}
      onclick={(e) => { e.stopPropagation(); running ? onStop() : onStart() }}
      type="button"
      aria-label={running ? 'Stop deck' : 'Start deck'}
    >
      {running ? '■' : '▶'}
    </button>
  </div>
</div>

<style>
  .mdeck {
    display: flex;
    flex-direction: column;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    overflow: hidden;
    cursor: pointer;
    transition: border-color var(--t-fast);
    user-select: none;
  }

  .mdeck:hover { border-color: var(--accent); }

  .mdeck--selected {
    border-color: var(--accent);
    box-shadow: 0 0 10px var(--accent-glow);
  }

  .mdeck__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px 2px;
  }

  .mdeck__num {
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.1em;
    color: var(--text-secondary);
    text-transform: uppercase;
  }

  .mdeck__live {
    font-size: 7px;
    color: var(--text-muted);
    transition: color var(--t-fast);
  }

  .mdeck__live--on {
    color: var(--live);
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.3; } }

  .mdeck__preview {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    background: var(--bg-base);
    overflow: hidden;
  }

  .mdeck__video {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .mdeck__idle {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    color: var(--text-muted);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .mdeck__name {
    padding: 2px 8px;
    font-size: 10px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-height: 18px;
  }

  .mdeck__empty {
    color: var(--text-muted);
    font-style: italic;
  }

  .mdeck__footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px;
    border-top: 1px solid var(--border-subtle);
  }

  .mdeck__bus-btn {
    font-size: 9px;
    font-weight: 700;
    background: none;
    border: none;
    cursor: pointer;
    letter-spacing: 0.06em;
    padding: 0;
    transition: opacity var(--t-fast);
  }

  .mdeck__bus-btn:hover { opacity: 0.7; }

  .mdeck__play {
    font-size: 10px;
    background: none;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    color: var(--live);
    width: 22px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2px 8px;
    cursor: pointer;
    transition: border-color var(--t-fast), color var(--t-fast);
  }

  .mdeck__play:hover { border-color: var(--live); color: var(--live); }
  .mdeck__play--stop:hover { border-color: var(--accent); color: var(--accent); }
</style>
