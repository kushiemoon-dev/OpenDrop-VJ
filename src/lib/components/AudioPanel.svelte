<script>
  import { onDestroy } from 'svelte';
  import { invoke } from "@tauri-apps/api/core";
  import VuMeter from './VuMeter.svelte';
  import StatusIndicator from './StatusIndicator.svelte';
  import { RefreshCw, Play, Square, Mic, Speaker, Monitor } from 'lucide-svelte';

  /**
   * @typedef {{
   *   name: string,
   *   description: string,
   *   is_default: boolean,
   *   is_monitor: boolean,
   *   device_type: 'input' | 'output' | 'monitor'
   * }} AudioDevice
   */

  /**
   * @type {{
   *   devices?: AudioDevice[],
   *   selectedDevice?: string,
   *   running?: boolean,
   *   onStart?: () => void,
   *   onStop?: () => void,
   *   onRefresh?: () => void
   * }}
   */
  let { devices = [], selectedDevice = $bindable(''), running = false, onStart, onStop, onRefresh } = $props();

  /**
   * Get icon component based on device type
   * @param {string} deviceType
   */
  function getDeviceIcon(deviceType) {
    switch (deviceType) {
      case 'output': return Speaker;
      case 'monitor': return Monitor;
      default: return Mic;
    }
  }

  /**
   * Format device display name
   * @param {AudioDevice} device
   */
  function formatDeviceName(device) {
    // Use description if available, otherwise fall back to name
    const displayName = device.description || device.name;
    const maxLen = 35;
    return displayName.length > maxLen ? displayName.slice(0, maxLen) + '...' : displayName;
  }

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
      {#if devices.length === 0}
        <option value="">No devices found</option>
      {:else}
        {#each devices as device}
          <option value={device.name}>
            {device.device_type === 'output' ? '🔊 ' : device.device_type === 'monitor' ? '📺 ' : '🎤 '}
            {formatDeviceName(device)}
            {device.is_default ? ' ★' : ''}
          </option>
        {/each}
      {/if}
    </select>
    <button class="icon-btn" onclick={onRefresh} title="Refresh devices">
      <RefreshCw size={14} />
    </button>
  </div>

  {#if devices.length > 0}
    {@const selected = devices.find(d => d.name === selectedDevice)}
    {#if selected?.device_type === 'output'}
      <div class="device-hint loopback">
        Loopback mode - captures system audio output
      </div>
    {:else if selected?.device_type === 'monitor'}
      <div class="device-hint monitor">
        Monitor device - captures what you hear
      </div>
    {/if}
  {/if}

  <div class="controls">
    {#if !running}
      <button class="btn primary" onclick={onStart}>
        <Play size={14} fill="currentColor" />
        Start
      </button>
    {:else}
      <button class="btn danger" onclick={onStop}>
        <Square size={14} fill="currentColor" />
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

  .device-hint {
    font-size: 0.75em;
    padding: var(--spacing-xs) var(--spacing-sm);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    background: var(--bg-dark);
    border-left: 2px solid var(--border-subtle);
  }

  .device-hint.loopback {
    border-left-color: var(--accent-cyan);
    color: var(--accent-cyan);
  }

  .device-hint.monitor {
    border-left-color: var(--accent-green);
    color: var(--accent-green);
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
