<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from 'svelte';
  import StatusIndicator from './StatusIndicator.svelte';
  import { showToast } from "$lib/stores/toast";
  import { Sliders, RefreshCw, Plug, X } from 'lucide-svelte';

  /**
   * @type {{
   *   onMidiEvent?: (action: string, value: number) => void
   * }}
   */
  let { onMidiEvent } = $props();

  /** @type {Array<{index: number, name: string}>} */
  let ports = $state([]);
  let selectedPort = $state(-1);
  let connected = $state(false);
  let learning = $state(false);
  let loading = $state(false);
  let error = $state('');

  /** @type {Array<{id: string, name: string, midi_type: string, action: string, enabled: boolean}>} */
  let mappings = $state([]);

  /** @type {Array<{name: string, description: string, controller: string, mapping_count: number}>} */
  let builtinPresets = $state([]);

  let learnAction = $state('');
  let learnName = $state('');
  let learnDeck = $state(0);

  /** @type {ReturnType<typeof setInterval> | undefined} */
  let refreshInterval;

  onMount(async () => {
    await Promise.all([
      refreshPorts(),
      refreshStatus(),
      loadBuiltinPresets()
    ]);

    // Poll status periodically when connected
    refreshInterval = setInterval(async () => {
      if (connected) {
        await refreshStatus();
      }
    }, 1000);
  });

  onDestroy(() => {
    if (refreshInterval) {
      clearInterval(refreshInterval);
    }
  });

  async function refreshPorts() {
    loading = true;
    error = '';
    try {
      /** @type {Array<{index: number, name: string}>} */
      ports = await invoke('list_midi_ports');
      if (ports.length > 0 && selectedPort === -1) {
        selectedPort = ports[0].index;
      }
    } catch (e) {
      error = String(e);
      ports = [];
    }
    loading = false;
  }

  async function refreshStatus() {
    try {
      /** @type {{connected: boolean, learning: boolean, port_name: string|null, mapping_count: number}} */
      const status = await invoke('midi_get_status');
      connected = status.connected;
      learning = status.learning;

      // Also refresh mappings
      mappings = await invoke('midi_get_mappings');
    } catch (e) {
      // Silently fail status refresh
    }
  }

  async function loadBuiltinPresets() {
    try {
      builtinPresets = await invoke('midi_list_builtin_presets');
    } catch (e) {
      showToast("Failed to load MIDI presets", "error");
    }
  }

  async function connect() {
    if (selectedPort < 0) return;
    loading = true;
    error = '';
    try {
      await invoke('midi_connect', { portIndex: selectedPort });
      connected = true;
      await refreshStatus();
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  async function disconnect() {
    loading = true;
    error = '';
    try {
      await invoke('midi_disconnect');
      connected = false;
      learning = false;
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  async function startLearn() {
    if (!learnAction || !learnName) {
      error = 'Please enter mapping name and select action';
      return;
    }
    loading = true;
    error = '';
    try {
      await invoke('midi_start_learn', {
        action: learnAction,
        name: learnName,
        deckId: learnDeck
      });
      learning = true;
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  async function cancelLearn() {
    loading = true;
    try {
      await invoke('midi_cancel_learn');
      learning = false;
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  /** @param {string} id */
  async function removeMapping(id) {
    try {
      await invoke('midi_remove_mapping', { mappingId: id });
      await refreshStatus();
    } catch (e) {
      error = String(e);
    }
  }

  async function clearAllMappings() {
    if (!confirm('Remove all MIDI mappings?')) return;
    try {
      await invoke('midi_clear_mappings');
      await refreshStatus();
    } catch (e) {
      error = String(e);
    }
  }

  /** @param {string} name */
  async function loadPreset(name) {
    loading = true;
    error = '';
    try {
      await invoke('midi_load_builtin_preset', { presetName: name });
      await refreshStatus();
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  const actions = [
    { value: 'deck_volume', label: 'Deck Volume' },
    { value: 'deck_toggle', label: 'Deck Play/Stop' },
    { value: 'next_preset', label: 'Next Preset' },
    { value: 'previous_preset', label: 'Previous Preset' },
    { value: 'random_preset', label: 'Random Preset' },
    { value: 'crossfader', label: 'Crossfader' },
    { value: 'beat_sensitivity', label: 'Beat Sensitivity' },
    { value: 'playlist_next', label: 'Playlist Next' },
    { value: 'playlist_previous', label: 'Playlist Previous' },
  ];
</script>

<div class="midi-panel">
  <div class="panel-header">
    <h3>MIDI Control</h3>
    <StatusIndicator active={connected} size="sm" />
  </div>

  <!-- Port Selection -->
  <div class="section">
    <div class="section-header">Controller</div>
    {#if ports.length === 0 && !loading}
      <div class="no-devices">
        <Sliders size={20} strokeWidth={1.5} class="no-device-icon" />
        <span>No MIDI devices found</span>
        <small>Connect a MIDI controller</small>
      </div>
    {:else}
      <div class="port-select">
        <select bind:value={selectedPort} disabled={connected || loading}>
          {#each ports as port}
            <option value={port.index}>{port.name}</option>
          {/each}
        </select>
        <button class="icon-btn" onclick={refreshPorts} disabled={loading} title="Refresh">
          <span class:spinning={loading}><RefreshCw size={14} /></span>
        </button>
      </div>

      <div class="controls">
        {#if !connected}
          <button class="btn primary" onclick={connect} disabled={loading || selectedPort < 0}>
            <Plug size={14} />
            Connect
          </button>
        {:else}
          <button class="btn danger" onclick={disconnect} disabled={loading}>
            <X size={14} />
            Disconnect
          </button>
        {/if}
      </div>
    {/if}
  </div>

  {#if connected}
    <!-- Quick Presets -->
    <div class="section">
      <div class="section-header">Quick Presets</div>
      <div class="preset-grid">
        {#each builtinPresets as preset}
          <button class="preset-btn" onclick={() => loadPreset(preset.name)} title={preset.description}>
            {preset.controller === 'Generic' ? 'Generic' :
             preset.controller === 'Akai APC Mini' ? 'APC Mini' :
             preset.controller === 'Novation Launchpad' ? 'Launchpad' :
             preset.controller === 'Korg nanoKONTROL2' ? 'nanoKONTROL' :
             preset.name}
          </button>
        {/each}
      </div>
    </div>

    <!-- Learn Mode -->
    <div class="section">
      <div class="section-header">Learn Mapping</div>
      {#if learning}
        <div class="learn-active">
          <div class="learn-pulse"></div>
          <span>Move a MIDI control...</span>
          <button class="btn-small" onclick={cancelLearn}>Cancel</button>
        </div>
      {:else}
        <div class="learn-form">
          <input type="text" bind:value={learnName} placeholder="Mapping name" class="input-sm" />
          <select bind:value={learnAction} class="input-sm">
            <option value="">Select action...</option>
            {#each actions as action}
              <option value={action.value}>{action.label}</option>
            {/each}
          </select>
          <select bind:value={learnDeck} class="input-sm deck-select">
            <option value={0}>Deck 1</option>
            <option value={1}>Deck 2</option>
            <option value={2}>Deck 3</option>
            <option value={3}>Deck 4</option>
          </select>
          <button class="btn-small primary" onclick={startLearn} disabled={!learnAction || !learnName}>
            Learn
          </button>
        </div>
      {/if}
    </div>

    <!-- Current Mappings -->
    <div class="section">
      <div class="section-header">
        <span>Mappings ({mappings.length})</span>
        {#if mappings.length > 0}
          <button class="btn-tiny" onclick={clearAllMappings}>Clear All</button>
        {/if}
      </div>
      {#if mappings.length === 0}
        <div class="no-mappings">
          No mappings configured. Load a preset or use Learn mode.
        </div>
      {:else}
        <div class="mapping-list">
          {#each mappings as mapping}
            <div class="mapping-item">
              <div class="mapping-info">
                <span class="mapping-name">{mapping.name}</span>
                <span class="mapping-action">{mapping.action}</span>
              </div>
              <button class="btn-remove" onclick={() => removeMapping(mapping.id)} title="Remove">
                <X size={12} />
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if error}
    <div class="error">{error}</div>
  {/if}

  <div class="help-text">
    Map MIDI controls to deck functions
  </div>
</div>

<style>
  .midi-panel {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    padding: var(--spacing-lg);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-md);
    max-height: clamp(200px, 35vh, 400px); /* Adaptive height based on viewport */
    overflow-y: auto;
    overflow-x: hidden;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    position: sticky;
    top: 0;
    background: var(--bg-panel);
    z-index: 1;
    margin: calc(-1 * var(--spacing-lg));
    margin-bottom: 0;
    padding: var(--spacing-lg);
    padding-bottom: var(--spacing-sm);
  }

  .panel-header h3 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent-primary);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-sm);
  }

  .section-header {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .no-devices {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--spacing-xs);
    padding: var(--spacing-md);
    color: var(--text-muted);
    text-align: center;
    font-size: 11px;
  }

  .no-devices :global(.no-device-icon) {
    opacity: 0.5;
  }

  .no-devices small {
    font-size: 10px;
  }

  .port-select {
    display: flex;
    gap: var(--spacing-sm);
  }

  .port-select select {
    flex: 1;
    font-size: 11px;
    padding: var(--spacing-sm) var(--spacing-md);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
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
    background: linear-gradient(135deg, var(--accent-primary), #0077b6);
    color: white;
  }

  .btn.primary:hover:not(:disabled) {
    background: linear-gradient(135deg, #00e5ff, #0096c7);
    box-shadow: 0 0 15px rgba(0, 209, 255, 0.4);
  }

  .btn.danger {
    background: linear-gradient(135deg, var(--status-error), #c44569);
    color: white;
  }

  .btn.danger:hover:not(:disabled) {
    background: linear-gradient(135deg, #ff6b7a, #d65d7a);
  }

  .btn:disabled {
    opacity: 0.5;
  }

  .preset-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: var(--spacing-xs);
  }

  .preset-btn {
    padding: var(--spacing-sm);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 10px;
    color: var(--text-secondary);
    transition: var(--transition-fast);
  }

  .preset-btn:hover {
    background: var(--bg-elevated);
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .learn-active {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-md);
    background: rgba(0, 209, 255, 0.1);
    border: 1px solid var(--accent-primary);
    border-radius: var(--radius-md);
    font-size: 11px;
    color: var(--accent-primary);
  }

  .learn-pulse {
    width: 8px;
    height: 8px;
    background: var(--accent-primary);
    border-radius: 50%;
    animation: pulse 1s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.5; transform: scale(1.2); }
  }

  .learn-form {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--spacing-xs);
  }

  .input-sm {
    padding: var(--spacing-xs) var(--spacing-sm);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 10px;
    color: var(--text-primary);
  }

  .input-sm::placeholder {
    color: var(--text-muted);
  }

  .deck-select {
    grid-column: 1;
  }

  .btn-small {
    padding: var(--spacing-xs) var(--spacing-sm);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 10px;
    color: var(--text-secondary);
  }

  .btn-small.primary {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: white;
  }

  .btn-small:hover:not(:disabled) {
    background: var(--bg-elevated);
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .btn-small.primary:hover:not(:disabled) {
    background: #00e5ff;
  }

  .btn-tiny {
    padding: 2px var(--spacing-xs);
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 9px;
    color: var(--text-muted);
  }

  .btn-tiny:hover {
    border-color: var(--status-error);
    color: var(--status-error);
  }

  .no-mappings {
    padding: var(--spacing-md);
    text-align: center;
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-dark);
    border-radius: var(--radius-md);
  }

  .mapping-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: clamp(100px, 15vh, 150px); /* Adaptive height based on viewport */
    overflow-y: auto;
  }

  .mapping-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-xs) var(--spacing-sm);
    background: var(--bg-dark);
    border-radius: var(--radius-sm);
  }

  .mapping-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .mapping-name {
    font-size: 10px;
    color: var(--text-primary);
  }

  .mapping-action {
    font-size: 9px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .btn-remove {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    opacity: 0.5;
    transition: var(--transition-fast);
  }

  .btn-remove:hover {
    opacity: 1;
    color: var(--status-error);
    background: rgba(255, 71, 87, 0.1);
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
</style>
