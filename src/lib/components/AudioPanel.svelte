<script>
  import { onDestroy } from 'svelte';
  import { invoke } from "@tauri-apps/api/core";
  import VuMeter from './VuMeter.svelte';
  import StatusIndicator from './StatusIndicator.svelte';

  /**
   * @type {{
   *   devices?: Array<{name: string, is_default: boolean}>,
   *   selectedDevice?: string,
   *   running?: boolean,
   *   onStart?: () => void,
   *   onStop?: () => void,
   *   onRefresh?: () => void
   * }}
   */
  let { devices = [], selectedDevice = $bindable(''), running = false, onStart, onStop, onRefresh } = $props();

  // Real audio levels from backend
  let levelL = $state(0);
  let levelR = $state(0);
  /** @type {number | null} */
  let animationId = null;

  // Fetch real audio levels when running
  $effect(() => {
    if (running) {
      startLevelPolling();
    } else {
      stopLevelPolling();
      levelL = 0;
      levelR = 0;
    }
  });

  function startLevelPolling() {
    if (animationId) return;

    async function pollLevels() {
      try {
        const [left, right] = await invoke('get_audio_levels');
        // Apply some smoothing and scaling for better visualization
        levelL = Math.min(1, left * 3); // Scale up for visibility
        levelR = Math.min(1, right * 3);
      } catch (e) {
        // Ignore errors, levels stay at previous value
      }
      animationId = requestAnimationFrame(pollLevels);
    }
    pollLevels();
  }

  function stopLevelPolling() {
    if (animationId) {
      cancelAnimationFrame(animationId);
      animationId = null;
    }
  }

  onDestroy(() => {
    stopLevelPolling();
  });
</script>

<div class="audio-panel">
  <div class="panel-header">
    <h3>Audio Input</h3>
    <StatusIndicator active={running} size="sm" />
  </div>

  <div class="meters">
    <VuMeter level={levelL} label="L" />
    <VuMeter level={levelR} label="R" />
  </div>

  <div class="device-select">
    <select bind:value={selectedDevice} disabled={running}>
      {#each devices as device}
        <option value={device.name}>
          {device.name.length > 30 ? device.name.slice(0, 30) + '...' : device.name}
          {device.is_default ? ' (Default)' : ''}
        </option>
      {/each}
    </select>
    <button class="icon-btn" onclick={onRefresh} title="Refresh devices">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
        <path d="M21 3v5h-5" />
      </svg>
    </button>
  </div>

  <div class="controls">
    {#if !running}
      <button class="btn primary" onclick={onStart}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
          <polygon points="5,3 19,12 5,21" />
        </svg>
        Start
      </button>
    {:else}
      <button class="btn danger" onclick={onStop}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
          <rect x="6" y="6" width="12" height="12" />
        </svg>
        Stop
      </button>
    {/if}
  </div>
</div>

<style>
  .audio-panel {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    padding: var(--spacing-lg);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-md);
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .panel-header h3 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent-cyan);
  }

  .meters {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
    padding: var(--spacing-md);
    background: var(--bg-dark);
    border-radius: var(--radius-md);
    border: 1px solid var(--border-subtle);
  }

  .device-select {
    display: flex;
    gap: var(--spacing-sm);
  }

  .device-select select {
    flex: 1;
    font-size: 12px;
    padding: var(--spacing-sm) var(--spacing-md);
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
  }

  .icon-btn:hover {
    background: var(--bg-elevated);
    color: var(--text-primary);
    border-color: var(--border-medium);
  }

  .controls {
    display: flex;
    gap: var(--spacing-sm);
  }

  .btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-sm) var(--spacing-md);
    border-radius: var(--radius-md);
    font-size: 12px;
    font-weight: 600;
    transition: var(--transition-fast);
  }

  .btn.primary {
    background: linear-gradient(135deg, var(--accent-cyan), #0096c7);
    color: white;
  }

  .btn.primary:hover {
    background: linear-gradient(135deg, #00ffff, #00b4d8);
    box-shadow: 0 0 15px rgba(0, 240, 255, 0.4);
  }

  .btn.danger {
    background: linear-gradient(135deg, var(--status-error), #c44569);
    color: white;
  }

  .btn.danger:hover {
    background: linear-gradient(135deg, #ff6b7a, #d65d7a);
    box-shadow: 0 0 15px rgba(255, 71, 87, 0.4);
  }
</style>
