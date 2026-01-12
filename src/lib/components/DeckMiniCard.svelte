<script>
  import StatusIndicator from './StatusIndicator.svelte';

  /**
   * @type {{
   *   deckId?: number,
   *   running?: boolean,
   *   preset?: string | null,
   *   volume?: number,
   *   onStart?: () => void,
   *   onStop?: () => void,
   *   onFullscreen?: () => void,
   *   onVolumeChange?: (volume: number) => void,
   *   onSelect?: (deckId: number) => void,
   *   selected?: boolean
   * }}
   */
  let {
    deckId = 0,
    running = false,
    preset = null,
    volume = 1.0,
    onStart,
    onStop,
    onFullscreen,
    onVolumeChange,
    onSelect,
    selected = false
  } = $props();

  // Extract preset name from path
  let presetName = $derived(
    preset ? preset.split('/').pop()?.replace('.milk', '') || '' : 'No preset'
  );

  /** @param {Event} e */
  function handleVolumeInput(e) {
    const target = /** @type {HTMLInputElement} */ (e.target);
    onVolumeChange?.(parseFloat(target.value));
  }

  /**
   * @param {Event} e
   * @param {(() => void) | undefined} fn
   */
  function stopProp(e, fn) {
    e.stopPropagation();
    fn?.();
  }
</script>

<div
  class="deck-card"
  class:running
  class:selected
  onclick={() => onSelect?.(deckId)}
  onkeydown={(e) => e.key === 'Enter' && onSelect?.(deckId)}
  role="button"
  tabindex="0"
>
  <div class="deck-header">
    <div class="deck-title">
      <StatusIndicator active={running} size="sm" />
      <span class="deck-number">Deck {deckId + 1}</span>
    </div>
    <div class="deck-actions">
      {#if running}
        <button class="action-btn" onclick={(e) => stopProp(e, onFullscreen)} title="Fullscreen">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3" />
          </svg>
        </button>
      {/if}
    </div>
  </div>

  <div class="preview-area">
    {#if running}
      <div class="live-indicator">
        <div class="pulse"></div>
        <span>LIVE</span>
      </div>
    {:else}
      <div class="idle-indicator">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" opacity="0.3">
          <polygon points="5,3 19,12 5,21" />
        </svg>
      </div>
    {/if}
  </div>

  <div class="deck-info">
    <span class="preset-name" title={preset || 'No preset'}>{presetName}</span>
  </div>

  <div class="deck-controls">
    <div class="volume-control">
      <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" opacity="0.5">
        <path d="M11 5L6 9H2v6h4l5 4V5zM15.54 8.46a5 5 0 0 1 0 7.07" fill="none" stroke="currentColor" stroke-width="2" />
      </svg>
      <input
        type="range"
        min="0"
        max="1"
        step="0.01"
        value={volume}
        oninput={handleVolumeInput}
        onclick={(e) => e.stopPropagation()}
        class="volume-slider"
      />
    </div>

    <div class="control-buttons">
      {#if !running}
        <button class="btn start" onclick={(e) => stopProp(e, onStart)} title="Start deck" aria-label="Start deck">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
            <polygon points="5,3 19,12 5,21" />
          </svg>
        </button>
      {:else}
        <button class="btn stop" onclick={(e) => stopProp(e, onStop)} title="Stop deck" aria-label="Stop deck">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
            <rect x="6" y="6" width="12" height="12" />
          </svg>
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .deck-card {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    padding: var(--spacing-md);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .deck-card:hover {
    border-color: var(--border-medium);
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .deck-card.selected {
    border-color: var(--accent-cyan);
    box-shadow: 0 0 15px rgba(0, 240, 255, 0.2);
  }

  .deck-card.running {
    border-color: var(--status-active);
  }

  .deck-card.running.selected {
    border-color: var(--accent-cyan);
    box-shadow: 0 0 20px rgba(0, 240, 255, 0.3), 0 0 10px rgba(0, 255, 136, 0.2);
  }

  .deck-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .deck-title {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
  }

  .deck-number {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-secondary);
  }

  .deck-card.running .deck-number {
    color: var(--status-active);
  }

  .deck-actions {
    display: flex;
    gap: var(--spacing-xs);
  }

  .action-btn {
    padding: 4px;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    background: transparent;
  }

  .action-btn:hover {
    color: var(--text-primary);
    background: var(--bg-elevated);
  }

  .preview-area {
    aspect-ratio: 16/9;
    background: var(--bg-dark);
    border-radius: var(--radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    overflow: hidden;
    background:
      linear-gradient(135deg,
        var(--bg-dark) 0%,
        rgba(0, 240, 255, 0.03) 50%,
        var(--bg-dark) 100%
      );
  }

  .deck-card.running .preview-area {
    background:
      linear-gradient(135deg,
        var(--bg-dark) 0%,
        rgba(0, 255, 136, 0.05) 25%,
        rgba(0, 240, 255, 0.05) 75%,
        var(--bg-dark) 100%
      );
    animation: gradient-pulse 3s ease infinite;
  }

  @keyframes gradient-pulse {
    0%, 100% { opacity: 0.8; }
    50% { opacity: 1; }
  }

  .idle-indicator {
    color: var(--text-muted);
  }

  .live-indicator {
    display: flex;
    align-items: center;
    gap: var(--spacing-xs);
    color: var(--status-active);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 1px;
  }

  .pulse {
    width: 8px;
    height: 8px;
    background: var(--status-active);
    border-radius: 50%;
    animation: live-pulse 1s ease-in-out infinite;
  }

  @keyframes live-pulse {
    0%, 100% { opacity: 1; box-shadow: 0 0 0 0 rgba(0, 255, 136, 0.5); }
    50% { opacity: 0.7; box-shadow: 0 0 0 6px rgba(0, 255, 136, 0); }
  }

  .deck-info {
    min-height: 18px;
  }

  .preset-name {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: block;
  }

  .deck-controls {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    padding-top: var(--spacing-xs);
    border-top: 1px solid var(--border-subtle);
  }

  .volume-control {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--spacing-xs);
  }

  .volume-slider {
    flex: 1;
    height: 4px;
    -webkit-appearance: none;
    appearance: none;
    background: var(--bg-dark);
    border-radius: 2px;
    outline: none;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    background: var(--accent-cyan);
    border-radius: 50%;
    cursor: pointer;
    transition: transform 0.15s;
  }

  .volume-slider::-webkit-slider-thumb:hover {
    transform: scale(1.2);
  }

  .control-buttons {
    display: flex;
    gap: var(--spacing-xs);
  }

  .btn {
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s;
  }

  .btn.start {
    background: linear-gradient(135deg, var(--accent-cyan), #0096c7);
    color: white;
  }

  .btn.start:hover {
    box-shadow: 0 0 12px rgba(0, 240, 255, 0.5);
    transform: scale(1.05);
  }

  .btn.stop {
    background: linear-gradient(135deg, var(--status-error), #c44569);
    color: white;
  }

  .btn.stop:hover {
    box-shadow: 0 0 12px rgba(255, 71, 87, 0.5);
    transform: scale(1.05);
  }
</style>
