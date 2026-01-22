<script>
  import StatusIndicator from './StatusIndicator.svelte';
  import { Play, Square, Maximize } from 'lucide-svelte';

  let {
    running = false,
    currentPreset = '',
    onStart,
    onStop,
    onFullscreen
  } = $props();

  // Extract preset name from path
  let presetName = $derived(
    currentPreset ? currentPreset.split('/').pop()?.replace('.milk', '') || '' : 'No preset loaded'
  );
</script>

<div class="deck-panel">
  <div class="preview-area" class:active={running}>
    {#if running}
      <div class="running-indicator">
        <div class="pulse-ring"></div>
        <div class="pulse-ring delay"></div>
        <span class="text">LIVE</span>
      </div>
    {:else}
      <div class="placeholder">
        <Play size={48} strokeWidth={1.5} />
        <span>Click Start to begin visualization</span>
      </div>
    {/if}
  </div>

  <div class="controls-bar">
    <div class="preset-info">
      <StatusIndicator active={running} size="sm" />
      <span class="preset-name" title={currentPreset}>{presetName}</span>
    </div>

    <div class="buttons">
      {#if !running}
        <button class="btn primary" onclick={onStart}>
          <Play size={16} fill="currentColor" />
          Start
        </button>
      {:else}
        <button class="btn danger" onclick={onStop}>
          <Square size={16} fill="currentColor" />
          Stop
        </button>
        <button class="btn secondary" onclick={onFullscreen} title="Fullscreen (F11)">
          <Maximize size={16} />
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .deck-panel {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-md);
    height: 100%;
  }

  .preview-area {
    flex: 1;
    min-height: 200px;
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    overflow: hidden;

    /* Animated gradient background */
    background:
      linear-gradient(135deg,
        var(--bg-dark) 0%,
        rgba(0, 240, 255, 0.05) 25%,
        var(--bg-dark) 50%,
        rgba(139, 92, 246, 0.05) 75%,
        var(--bg-dark) 100%
      );
    background-size: 400% 400%;
    animation: gradient-shift 15s ease infinite;
  }

  .preview-area.active {
    border-color: var(--accent-cyan);
    box-shadow:
      inset 0 0 30px rgba(0, 240, 255, 0.1),
      0 0 20px rgba(0, 240, 255, 0.2);
  }

  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--spacing-md);
    color: var(--text-muted);
  }

  .placeholder :global(svg) {
    opacity: 0.3;
  }

  .placeholder span {
    font-size: 13px;
  }

  .running-indicator {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .pulse-ring {
    position: absolute;
    width: 80px;
    height: 80px;
    border: 2px solid var(--accent-cyan);
    border-radius: 50%;
    animation: pulse-expand 2s ease-out infinite;
    opacity: 0;
  }

  .pulse-ring.delay {
    animation-delay: 1s;
  }

  @keyframes pulse-expand {
    0% {
      transform: scale(0.5);
      opacity: 0.8;
    }
    100% {
      transform: scale(2);
      opacity: 0;
    }
  }

  .running-indicator .text {
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 2px;
    color: var(--accent-cyan);
    text-shadow: 0 0 10px var(--accent-cyan);
    z-index: 1;
  }

  .controls-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-md) var(--spacing-lg);
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
  }

  .preset-info {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    min-width: 0;
    flex: 1;
  }

  .preset-name {
    font-size: 13px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .buttons {
    display: flex;
    gap: var(--spacing-sm);
    flex-shrink: 0;
  }

  .btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-sm) var(--spacing-lg);
    border-radius: var(--radius-md);
    font-size: 13px;
    font-weight: 600;
    transition: var(--transition-fast);
    min-width: 80px;
  }

  .btn.primary {
    background: linear-gradient(135deg, var(--accent-cyan), #0096c7);
    color: white;
  }

  .btn.primary:hover {
    background: linear-gradient(135deg, #00ffff, #00b4d8);
    box-shadow: 0 0 20px rgba(0, 240, 255, 0.5);
    transform: translateY(-1px);
  }

  .btn.secondary {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    color: var(--text-primary);
    min-width: auto;
    padding: var(--spacing-sm);
  }

  .btn.secondary:hover {
    background: var(--bg-hover);
    border-color: var(--border-medium);
  }

  .btn.danger {
    background: linear-gradient(135deg, var(--status-error), #c44569);
    color: white;
  }

  .btn.danger:hover {
    background: linear-gradient(135deg, #ff6b7a, #d65d7a);
    box-shadow: 0 0 20px rgba(255, 71, 87, 0.5);
    transform: translateY(-1px);
  }

  @keyframes gradient-shift {
    0% { background-position: 0% 50%; }
    50% { background-position: 100% 50%; }
    100% { background-position: 0% 50%; }
  }
</style>
