<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from 'svelte';
  import StatusIndicator from './StatusIndicator.svelte';
  import { showToast } from "$lib/stores/toast";
  import { Monitor, RefreshCw, Video, Square, AlertCircle, Cast } from 'lucide-svelte';

  /**
   * @type {{
   *   deckId?: number,
   *   onStatusChange?: () => void
   * }}
   */
  let { deckId = 0, onStatusChange } = $props();

  /** @type {Array<{path: string, name: string}>} */
  let devices = $state([]);
  let selectedDevice = $state('');
  let enabled = $state(false);
  let loading = $state(false);
  let error = $state('');

  /** @type {Array<{index: number, name: string, width: number, height: number, is_primary: boolean}>} */
  let monitors = $state([]);
  let selectedMonitor = $state(0);

  // NDI state
  let ndiAvailable = $state(false);
  let ndiEnabled = $state(false);
  let ndiName = $state('');

  onMount(async () => {
    await Promise.all([refreshDevices(), refreshMonitors(), checkNdiAvailable()]);
  });

  // Refresh when deck changes
  $effect(() => {
    // Reset state when deck changes
    void deckId; // Track dependency
    enabled = false;
    ndiEnabled = false;
  });

  async function refreshDevices() {
    loading = true;
    error = '';
    try {
      /** @type {string[]} */
      const rawDevices = await invoke('list_video_outputs');
      // Parse format: "/dev/video10:OpenDrop"
      devices = rawDevices.map(d => {
        const [path, name] = d.split(':');
        return { path, name: name || path };
      });
      if (devices.length > 0 && !selectedDevice) {
        selectedDevice = devices[0].path;
      }
    } catch (e) {
      error = String(e);
      devices = [];
    }
    loading = false;
  }

  async function refreshMonitors() {
    try {
      /** @type {Array<{index: number, name: string, width: number, height: number, is_primary: boolean}>} */
      monitors = await invoke('list_monitors');
    } catch (e) {
      showToast("Failed to list monitors", "error");
      monitors = [];
    }
  }

  async function toggleVideoOutput() {
    loading = true;
    error = '';
    try {
      const newEnabled = !enabled;
      await invoke('set_deck_video_output', {
        deckId,
        enabled: newEnabled,
        devicePath: newEnabled ? selectedDevice : null
      });
      enabled = newEnabled;
      onStatusChange?.();
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  async function checkNdiAvailable() {
    try {
      ndiAvailable = await invoke('is_ndi_available');
    } catch (e) {
      showToast("Failed to check NDI availability", "error");
      ndiAvailable = false;
    }
  }

  async function toggleNdiOutput() {
    loading = true;
    error = '';
    try {
      const newEnabled = !ndiEnabled;
      await invoke('set_deck_ndi_output', {
        deckId,
        enabled: newEnabled,
        name: newEnabled && ndiName ? ndiName : null
      });
      ndiEnabled = newEnabled;
      onStatusChange?.();
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }
</script>

<div class="video-panel">
  <div class="panel-header">
    <h3>Video Output</h3>
    <div class="status-indicators">
      {#if enabled}<StatusIndicator active={true} size="sm" label="v4l2" />{/if}
      {#if ndiEnabled}<StatusIndicator active={true} size="sm" label="NDI" />{/if}
      {#if !enabled && !ndiEnabled}<StatusIndicator active={false} size="sm" />{/if}
    </div>
  </div>

  <div class="deck-indicator">
    Deck {deckId + 1}
  </div>

  {#if devices.length === 0 && !loading}
    <div class="no-devices">
      <Monitor size={24} strokeWidth={1.5} class="no-device-icon" />
      <span>No v4l2 devices found</span>
      <small>Install v4l2loopback-dkms, then run:</small>
      <code>sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="OpenDrop"</code>
    </div>
  {:else}
    <div class="device-select">
      <select bind:value={selectedDevice} disabled={enabled || loading}>
        {#each devices as device}
          <option value={device.path}>
            {device.name} ({device.path})
          </option>
        {/each}
      </select>
      <button class="icon-btn" onclick={refreshDevices} disabled={loading} title="Refresh devices">
        <span class:spinning={loading}><RefreshCw size={14} /></span>
      </button>
    </div>

    {#if monitors.length > 1}
      <div class="monitor-select">
        <label for="monitor-select-{deckId}">Fullscreen Monitor</label>
        <select id="monitor-select-{deckId}" bind:value={selectedMonitor} disabled={loading}>
          {#each monitors as monitor}
            <option value={monitor.index}>
              {monitor.name} ({monitor.width}x{monitor.height}){monitor.is_primary ? ' [Primary]' : ''}
            </option>
          {/each}
        </select>
      </div>
    {/if}

    <div class="controls">
      {#if !enabled}
        <button class="btn primary" onclick={toggleVideoOutput} disabled={loading || !selectedDevice}>
          <Video size={14} />
          Enable Output
        </button>
      {:else}
        <button class="btn danger" onclick={toggleVideoOutput} disabled={loading}>
          <Square size={14} fill="currentColor" />
          Disable
        </button>
      {/if}
    </div>

    {#if enabled}
      <div class="output-info">
        <div class="info-row">
          <span class="label">Streaming to</span>
          <span class="value">{selectedDevice}</span>
        </div>
        <div class="info-row">
          <span class="label">Format</span>
          <span class="value">YUYV 1280x720</span>
        </div>
      </div>
    {/if}
  {/if}

  {#if error}
    <div class="error">
      {error}
    </div>
  {/if}

  <div class="help-text">
    Output to OBS, VLC, or any v4l2 app
  </div>

  <!-- NDI Section -->
  <div class="section-divider"></div>

  <div class="ndi-section">
    <div class="section-header">
      <h4>NDI Network</h4>
      <StatusIndicator active={ndiEnabled} size="sm" />
    </div>

    {#if !ndiAvailable}
      <div class="ndi-unavailable">
        <AlertCircle size={18} strokeWidth={1.5} class="ndi-alert-icon" />
        <span>NDI runtime not installed</span>
        <a href="https://ndi.video/tools/" target="_blank" rel="noopener">Get NDI Tools</a>
      </div>
    {:else}
      <div class="ndi-name-input">
        <input
          type="text"
          placeholder="Source name (default: OpenDrop Deck X)"
          bind:value={ndiName}
          disabled={ndiEnabled || loading}
        />
      </div>

      <div class="controls">
        {#if !ndiEnabled}
          <button class="btn ndi" onclick={toggleNdiOutput} disabled={loading}>
            <Cast size={14} />
            Enable NDI
          </button>
        {:else}
          <button class="btn danger" onclick={toggleNdiOutput} disabled={loading}>
            <Square size={14} fill="currentColor" />
            Disable NDI
          </button>
        {/if}
      </div>

      {#if ndiEnabled}
        <div class="output-info ndi-info">
          <div class="info-row">
            <span class="label">Source name</span>
            <span class="value">{ndiName || `OpenDrop Deck ${deckId + 1}`}</span>
          </div>
          <div class="info-row">
            <span class="label">Protocol</span>
            <span class="value">NDI HX</span>
          </div>
        </div>
      {/if}
    {/if}

    <div class="help-text">
      Stream to NDI-compatible apps on network
    </div>
  </div>
</div>

<style>
  .video-panel {
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
    color: var(--accent-magenta);
  }

  .deck-indicator {
    font-size: 11px;
    color: var(--text-muted);
    padding: var(--spacing-xs) var(--spacing-sm);
    background: var(--bg-dark);
    border-radius: var(--radius-sm);
    text-align: center;
  }

  .no-devices {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-lg);
    color: var(--text-muted);
    text-align: center;
  }

  .no-devices :global(.no-device-icon) {
    opacity: 0.5;
  }

  .no-devices small {
    font-size: 10px;
    margin-top: var(--spacing-sm);
  }

  .no-devices code {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: var(--spacing-xs) var(--spacing-sm);
    background: var(--bg-dark);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
  }

  .device-select {
    display: flex;
    gap: var(--spacing-sm);
  }

  .device-select select {
    flex: 1;
    font-size: 11px;
    padding: var(--spacing-sm) var(--spacing-md);
  }

  .monitor-select {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xs);
  }

  .monitor-select label {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .monitor-select select {
    font-size: 11px;
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

  .icon-btn:hover:not(:disabled) {
    background: var(--bg-elevated);
    color: var(--text-primary);
    border-color: var(--border-medium);
  }

  .icon-btn:disabled {
    opacity: 0.5;
  }

  .spinning {
    display: inline-flex;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
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
    background: linear-gradient(135deg, var(--accent-magenta), #c44569);
    color: white;
  }

  .btn.primary:hover:not(:disabled) {
    background: linear-gradient(135deg, #ff00cc, #d65d7a);
    box-shadow: 0 0 15px rgba(255, 0, 170, 0.4);
  }

  .btn.primary:disabled {
    opacity: 0.5;
  }

  .btn.danger {
    background: linear-gradient(135deg, var(--status-error), #c44569);
    color: white;
  }

  .btn.danger:hover:not(:disabled) {
    background: linear-gradient(135deg, #ff6b7a, #d65d7a);
    box-shadow: 0 0 15px rgba(255, 71, 87, 0.4);
  }

  .output-info {
    padding: var(--spacing-md);
    background: var(--bg-dark);
    border-radius: var(--radius-md);
    border: 1px solid rgba(255, 0, 170, 0.3);
  }

  .info-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-xs) 0;
  }

  .info-row .label {
    font-size: 11px;
    color: var(--text-muted);
  }

  .info-row .value {
    font-size: 11px;
    color: var(--accent-magenta);
    font-family: var(--font-mono);
  }

  .error {
    padding: var(--spacing-sm) var(--spacing-md);
    background: rgba(255, 71, 87, 0.1);
    border: 1px solid var(--status-error);
    border-radius: var(--radius-md);
    color: var(--status-error);
    font-size: 11px;
  }

  .help-text {
    font-size: 10px;
    color: var(--text-muted);
    text-align: center;
  }

  /* Status indicators */
  .status-indicators {
    display: flex;
    gap: var(--spacing-xs);
    align-items: center;
  }

  /* Section divider */
  .section-divider {
    height: 1px;
    background: var(--border-subtle);
    margin: var(--spacing-md) 0;
  }

  /* NDI Section */
  .ndi-section {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .section-header h4 {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent-cyan, #00d4ff);
    margin: 0;
  }

  .ndi-unavailable {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--spacing-xs);
    padding: var(--spacing-md);
    color: var(--text-muted);
    text-align: center;
    font-size: 11px;
  }

  .ndi-unavailable :global(.ndi-alert-icon) {
    opacity: 0.5;
  }

  .ndi-unavailable a {
    color: var(--accent-cyan, #00d4ff);
    text-decoration: none;
    font-size: 10px;
  }

  .ndi-unavailable a:hover {
    text-decoration: underline;
  }

  .ndi-name-input input {
    width: 100%;
    font-size: 11px;
    padding: var(--spacing-sm) var(--spacing-md);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
  }

  .ndi-name-input input::placeholder {
    color: var(--text-muted);
  }

  .ndi-name-input input:focus {
    border-color: var(--accent-cyan, #00d4ff);
    outline: none;
  }

  .ndi-name-input input:disabled {
    opacity: 0.5;
  }

  .btn.ndi {
    background: linear-gradient(135deg, var(--accent-cyan, #00d4ff), #0099cc);
    color: white;
  }

  .btn.ndi:hover:not(:disabled) {
    background: linear-gradient(135deg, #00e5ff, #00b3e6);
    box-shadow: 0 0 15px rgba(0, 212, 255, 0.4);
  }

  .btn.ndi:disabled {
    opacity: 0.5;
  }

  .ndi-info {
    border-color: rgba(0, 212, 255, 0.3);
  }

  .ndi-info .value {
    color: var(--accent-cyan, #00d4ff);
  }
</style>
