<script lang="ts">
  interface Props {
    letter: 'A' | 'B'
    canvas: HTMLCanvasElement | undefined
    presetName: string
    isActive: boolean
    onSelect: () => void
  }

  let { letter, canvas, presetName, isActive, onSelect }: Props = $props()

  let videoEl: HTMLVideoElement | undefined = $state()

  $effect(() => {
    if (!canvas || !videoEl) return
    try {
      const stream = (canvas as any).captureStream(10) as MediaStream
      videoEl.srcObject = stream
    } catch {
      // captureStream non disponible (Firefox, Safari) — fallback silencieux
    }
  })
</script>

<button
  class="deck-card"
  class:deck-card--active={isActive}
  onclick={onSelect}
  type="button"
>
  <div class="deck-card__header">
    <span class="deck-card__letter">{letter}</span>
    {#if presetName}
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
</button>

<style>
  .deck-card {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0;
    background: #0e0e26;
    border: 1px solid #1e1e48;
    border-radius: var(--r-md, 6px);
    cursor: pointer;
    padding: 0;
    overflow: hidden;
    transition: border-color var(--t-fast, 150ms ease);
    text-align: left;
  }

  .deck-card:hover {
    border-color: #ff2d78;
  }

  .deck-card--active {
    border-color: #ff2d78;
    box-shadow: 0 0 12px rgba(255, 45, 120, 0.3), inset 0 0 8px rgba(255, 45, 120, 0.06);
  }

  .deck-card__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px 2px;
  }

  .deck-card__letter {
    font-size: 14px;
    font-weight: 800;
    color: #ff2d78;
  }

  .deck-card__live {
    font-size: 8px;
    color: #ff2d78;
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
    background: #080818;
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
    color: #33335a;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .deck-card__name {
    padding: 4px 8px 6px;
    font-size: 9px;
    color: #8888bb;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
