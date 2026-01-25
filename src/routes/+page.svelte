<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";
  import { fly, fade, slide } from "svelte/transition";
  import '../app.css';

  // Components
  import Header from '$lib/components/Header.svelte';
  import DeckMiniCard from '$lib/components/DeckMiniCard.svelte';
  import AudioPanel from '$lib/components/AudioPanel.svelte';
  import PresetBrowser from '$lib/components/PresetBrowser.svelte';
  import PlaylistPanel from '$lib/components/PlaylistPanel.svelte';
  import CrossfaderPanel from '$lib/components/CrossfaderPanel.svelte';
  import VideoOutputPanel from '$lib/components/VideoOutputPanel.svelte';
  import MidiPanel from '$lib/components/MidiPanel.svelte';
  import SettingsPanel from '$lib/components/SettingsPanel.svelte';

  // Lucide icons for sidebar toggle
  import { PanelRightOpen, PanelRightClose, X } from 'lucide-svelte';

  // Toast store - sync with global store for child components
  import { toast as globalToast } from '$lib/stores/toast';

  // Settings store for custom preset paths
  import { settings } from '$lib/stores/settings.svelte';

  // Sidebar collapsed state
  let sidebarCollapsed = $state(false);
  let sidebarMobileOpen = $state(false);

  // Settings panel state
  let settingsOpen = $state(false);

  /**
   * @typedef {{ name: string, path: string }} Preset
   * @typedef {{ name: string, path: string }} PlaylistItem
   * @typedef {{ name: string, items: PlaylistItem[], current_index: number, shuffle: boolean, auto_cycle: boolean, cycle_duration_secs: number }} Playlist
   * @typedef {{ id: number, running: boolean, preset: string | null, volume: number, beat_sensitivity: number, playlist: Playlist }} DeckInfo
   * @typedef {{ position: number, side_a: number[], side_b: number[], curve: string, enabled: boolean }} CrossfaderInfo
   * @typedef {{ name: string, description: string, is_default: boolean, is_monitor: boolean, device_type: 'input' | 'output' | 'monitor' }} AudioDevice
   */

  // Multi-deck state
  /** @type {{ decks: DeckInfo[], audio_running: boolean, preset_dir: string, crossfader: CrossfaderInfo, compositor: any }} */
  let multiDeckStatus = $state({
    decks: [
      { id: 0, running: false, preset: null, volume: 1.0, beat_sensitivity: 1.0, playlist: { name: '', items: [], current_index: 0, shuffle: false, auto_cycle: false, cycle_duration_secs: 30 } },
      { id: 1, running: false, preset: null, volume: 1.0, beat_sensitivity: 1.0, playlist: { name: '', items: [], current_index: 0, shuffle: false, auto_cycle: false, cycle_duration_secs: 30 } },
      { id: 2, running: false, preset: null, volume: 1.0, beat_sensitivity: 1.0, playlist: { name: '', items: [], current_index: 0, shuffle: false, auto_cycle: false, cycle_duration_secs: 30 } },
      { id: 3, running: false, preset: null, volume: 1.0, beat_sensitivity: 1.0, playlist: { name: '', items: [], current_index: 0, shuffle: false, auto_cycle: false, cycle_duration_secs: 30 } },
    ],
    audio_running: false,
    preset_dir: "",
    crossfader: {
      position: 0.5,
      side_a: [0, 1],
      side_b: [2, 3],
      curve: 'equal_power',
      enabled: false
    },
    compositor: {
      enabled: false,
      output_width: 1920,
      output_height: 1080,
      link_to_crossfader: true,
      deck_settings: {}
    }
  });

  // Selected deck for preset browser
  let selectedDeckId = $state(0);

  // Audio & UI state
  /** @type {AudioDevice[]} */
  let audioDevices = $state([]);
  let selectedDevice = $state("");
  let projectmVersion = $state("");
  /** @type {Preset[]} */
  let presets = $state([]);
  let loadingPresets = $state(false);
  let audioPumpActive = $state(false);
  /** @type {number | null} */
  let audioPumpId = null;

  // Toast notifications - local state synced with global store
  let toast = $state({ message: '', type: 'info', visible: false });

  // Sync global toast to local state for child components
  $effect(() => {
    if (globalToast.visible && globalToast.message !== toast.message) {
      toast = { ...globalToast };
    }
  });

  // Derived state
  let anyDeckRunning = $derived(multiDeckStatus.decks.some(d => d.running));
  let selectedDeck = $derived(multiDeckStatus.decks.find(d => d.id === selectedDeckId));
  let runningDecksCount = $derived(multiDeckStatus.decks.filter(d => d.running).length);

  // Audio pump loop - sends audio to all active decks
  let audioPumpErrorCount = $state(0);
  const AUDIO_PUMP_ERROR_THRESHOLD = 5;

  async function audioPumpLoop() {
    if (!audioPumpActive) return;
    try {
      await invoke("pump_audio");
      audioPumpErrorCount = 0; // Reset on success
    } catch (e) {
      audioPumpErrorCount++;
      if (audioPumpErrorCount === AUDIO_PUMP_ERROR_THRESHOLD) {
        showToast("Audio capture failing - check device connection", "error");
      }
    }
    audioPumpId = requestAnimationFrame(audioPumpLoop);
  }

  function startAudioPump() {
    if (!audioPumpActive) {
      audioPumpActive = true;
      audioPumpLoop();
    }
  }

  function stopAudioPump() {
    audioPumpActive = false;
    if (audioPumpId) {
      cancelAnimationFrame(audioPumpId);
      audioPumpId = null;
    }
  }

  // Start/stop audio pump based on any deck running
  $effect(() => {
    if (anyDeckRunning && multiDeckStatus.audio_running) {
      startAudioPump();
    } else {
      stopAudioPump();
    }
  });

  onMount(async () => {
    await refreshMultiDeckStatus();
    await loadAudioDevices();
    await loadPresets();
    projectmVersion = await invoke("get_projectm_version");
  });

  onDestroy(() => {
    stopAudioPump();
  });

  // API calls
  async function refreshMultiDeckStatus() {
    try {
      multiDeckStatus = await invoke("get_multi_deck_status");
    } catch (e) {
      showToast("Error: " + e, "error");
    }
  }

  async function loadAudioDevices() {
    try {
      audioDevices = await invoke("list_audio_devices");
      const defaultDevice = audioDevices.find(d => d.is_default);
      if (defaultDevice) {
        selectedDevice = defaultDevice.name;
      }
    } catch (e) {
      showToast("Error loading devices: " + e, "error");
    }
  }

  async function loadPresets() {
    loadingPresets = true;
    try {
      // Pass custom preset paths to backend if configured
      const customPaths = settings.customPresetPaths;
      if (customPaths.length > 0) {
        // Backend will scan both defaults + custom paths when dirs is provided
        presets = await invoke("list_presets", { dirs: customPaths });
      } else {
        // Use defaults only
        presets = await invoke("list_presets", { dirs: null });
      }
    } catch (e) {
      showToast("Error loading presets: " + e, "error");
    }
    loadingPresets = false;
  }

  /**
   * @param {string} message
   * @param {string} [type="info"]
   */
  function showToast(message, type = "info") {
    toast = { message, type, visible: true };
    setTimeout(() => {
      toast.visible = false;
    }, 3000);
  }

  // Deck actions
  /** @param {number} deckId */
  async function startDeck(deckId) {
    try {
      const deck = multiDeckStatus.decks.find(d => d.id === deckId);
      const result = await invoke("start_deck", {
        deckId,
        width: 1280,
        height: 720,
        fullscreen: false,
        presetPath: deck?.preset || null
      });
      showToast(/** @type {string} */ (result), "success");
      await refreshMultiDeckStatus();
    } catch (e) {
      showToast("Error: " + e, "error");
    }
  }

  /** @param {number} deckId */
  async function stopDeck(deckId) {
    try {
      const result = await invoke("stop_deck", { deckId });
      showToast(/** @type {string} */ (result), "success");
      await refreshMultiDeckStatus();
    } catch (e) {
      showToast("Error: " + e, "error");
    }
  }

  /** @param {number} deckId */
  async function toggleFullscreen(deckId) {
    try {
      await invoke("toggle_fullscreen", { deckId });
    } catch (e) {
      showToast("Error: " + e, "error");
    }
  }

  /**
   * @param {number} deckId
   * @param {number} volume
   */
  async function setDeckVolume(deckId, volume) {
    try {
      await invoke("set_deck_volume", { deckId, volume });
      // Update local state immediately for responsiveness
      const deck = multiDeckStatus.decks.find(d => d.id === deckId);
      if (deck) deck.volume = volume;
    } catch (e) {
      showToast("Error: " + e, "error");
    }
  }

  // Audio actions
  async function startAudio() {
    try {
      const result = await invoke("start_audio", {
        deviceName: selectedDevice || null
      });
      showToast(result, "success");
      await refreshMultiDeckStatus();
    } catch (e) {
      showToast("Error: " + e, "error");
    }
  }

  async function stopAudio() {
    try {
      const result = await invoke("stop_audio");
      showToast(result, "success");
      await refreshMultiDeckStatus();
    } catch (e) {
      showToast("Error: " + e, "error");
    }
  }

  // Preset actions
  /**
   * @param {string} path
   * @param {number} [deckId]
   */
  async function loadPresetOnDeck(path, deckId = selectedDeckId) {
    if (!path) return;
    try {
      const result = await invoke("load_preset", { path, deckId });
      showToast("Loaded: " + path.split('/').pop(), "success");
      await refreshMultiDeckStatus();
    } catch (e) {
      showToast("Error: " + e, "error");
    }
  }

  /** @param {Preset} preset */
  function selectPreset(preset) {
    // Update local state for selected deck
    const deck = multiDeckStatus.decks.find(d => d.id === selectedDeckId);
    if (deck) {
      deck.preset = preset.path;
    }
    // If deck is running, load the preset immediately
    if (deck?.running) {
      loadPresetOnDeck(preset.path, selectedDeckId);
    }
  }

  /** @param {number} deckId */
  function selectDeck(deckId) {
    selectedDeckId = deckId;
  }

  // Playlist actions
  /** @param {Preset} preset */
  async function addToPlaylist(preset) {
    try {
      await invoke("playlist_add", {
        deckId: selectedDeckId,
        name: preset.name,
        path: preset.path
      });
      await refreshMultiDeckStatus();
    } catch (e) {
      showToast("Error adding to playlist: " + e, "error");
    }
  }
</script>

<div class="app">
  <Header
    version={projectmVersion}
    visualizerRunning={anyDeckRunning}
    audioRunning={multiDeckStatus.audio_running}
    onSettingsClick={() => settingsOpen = true}
  />

  <div class="main-layout" class:sidebar-collapsed={sidebarCollapsed}>
    <!-- Mobile sidebar toggle button -->
    <button
      class="sidebar-mobile-toggle"
      onclick={() => sidebarMobileOpen = true}
      aria-label="Open sidebar"
    >
      <PanelRightOpen size={20} />
    </button>

    <!-- Desktop sidebar collapse toggle -->
    <button
      class="sidebar-collapse-toggle"
      onclick={() => sidebarCollapsed = !sidebarCollapsed}
      aria-label={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
      title={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
    >
      {#if sidebarCollapsed}
        <PanelRightOpen size={18} />
      {:else}
        <PanelRightClose size={18} />
      {/if}
    </button>
    <main class="main-content">
      <!-- Multi-deck grid -->
      <div class="decks-section">
        <div class="section-header">
          <h2>Decks</h2>
          <span class="running-count">
            {runningDecksCount} / 4 running
          </span>
        </div>
        <div class="decks-grid">
          {#each multiDeckStatus.decks as deck (deck.id)}
            <DeckMiniCard
              deckId={deck.id}
              running={deck.running}
              preset={deck.preset}
              volume={deck.volume}
              selected={selectedDeckId === deck.id}
              onStart={() => startDeck(deck.id)}
              onStop={() => stopDeck(deck.id)}
              onFullscreen={() => toggleFullscreen(deck.id)}
              onVolumeChange={(/** @type {number} */ v) => setDeckVolume(deck.id, v)}
              onSelect={selectDeck}
            />
          {/each}
        </div>
      </div>

      <!-- Preset browser for selected deck -->
      <div class="preset-section">
        <div class="section-header">
          <h2>Presets for Deck {selectedDeckId + 1}</h2>
        </div>
        <PresetBrowser
          {presets}
          currentPreset={selectedDeck?.preset || ''}
          loading={loadingPresets}
          onSelect={selectPreset}
          onLoad={(/** @type {string} */ path) => loadPresetOnDeck(path)}
          onAddToPlaylist={addToPlaylist}
        />
      </div>
    </main>

    <!-- Mobile sidebar overlay backdrop -->
    {#if sidebarMobileOpen}
      <div
        class="sidebar-backdrop"
        onclick={() => sidebarMobileOpen = false}
        onkeydown={(e) => e.key === 'Escape' && (sidebarMobileOpen = false)}
        role="button"
        tabindex="-1"
        aria-label="Close sidebar"
        transition:fade={{ duration: 200 }}
      ></div>
    {/if}

    <aside class="sidebar" class:collapsed={sidebarCollapsed} class:mobile-open={sidebarMobileOpen}>
      <!-- Mobile close button -->
      <button
        class="sidebar-close-mobile"
        onclick={() => sidebarMobileOpen = false}
        aria-label="Close sidebar"
      >
        <X size={20} />
      </button>

      <AudioPanel
        devices={audioDevices}
        bind:selectedDevice
        running={multiDeckStatus.audio_running}
        onStart={startAudio}
        onStop={stopAudio}
        onRefresh={loadAudioDevices}
      />

      <!-- Playlist for selected deck -->
      <PlaylistPanel
        deckId={selectedDeckId}
        playlist={selectedDeck?.playlist || { name: '', items: [], current_index: 0, shuffle: false, auto_cycle: false, cycle_duration_secs: 30 }}
        running={selectedDeck?.running || false}
        onUpdate={refreshMultiDeckStatus}
      />

      <!-- Crossfader A/B -->
      <CrossfaderPanel
        crossfader={multiDeckStatus.crossfader}
        onUpdate={refreshMultiDeckStatus}
      />

      <!-- Video Output -->
      <VideoOutputPanel
        deckId={selectedDeckId}
        onStatusChange={refreshMultiDeckStatus}
      />

      <!-- MIDI Control -->
      <MidiPanel />

      <!-- Selected deck info -->
      <div class="sidebar-section">
        <h3>Selected: Deck {selectedDeckId + 1}</h3>
        <div class="info-row">
          <span class="label">Status</span>
          <span class="value" class:active={selectedDeck?.running}>
            {selectedDeck?.running ? 'Running' : 'Stopped'}
          </span>
        </div>
        <div class="info-row">
          <span class="label">Preset</span>
          <span class="value preset" title={selectedDeck?.preset}>
            {selectedDeck?.preset?.split('/').pop()?.replace('.milk', '') || 'None'}
          </span>
        </div>
        <div class="info-row">
          <span class="label">Volume</span>
          <span class="value">
            {Math.round((selectedDeck?.volume || 1) * 100)}%
          </span>
        </div>
      </div>

      <!-- Decks overview -->
      <div class="sidebar-section">
        <h3>All Decks</h3>
        {#each multiDeckStatus.decks as deck (deck.id)}
          <div class="deck-status-row" class:active={deck.running} class:selected={deck.id === selectedDeckId}>
            <span class="deck-label">Deck {deck.id + 1}</span>
            <span class="deck-state">{deck.running ? 'Live' : 'Off'}</span>
          </div>
        {/each}
      </div>

      <div class="sidebar-section tips">
        <h3>Shortcuts</h3>
        <div class="tip"><kbd>F</kbd> / <kbd>F11</kbd> Fullscreen</div>
        <div class="tip"><kbd>ESC</kbd> Close window</div>
      </div>
    </aside>
  </div>

  <!-- Settings panel -->
  {#if settingsOpen}
    <SettingsPanel
      onClose={() => settingsOpen = false}
      onPresetsRefresh={loadPresets}
    />
  {/if}

  <!-- Toast notification -->
  {#if toast.visible}
    <div class="toast {toast.type}" transition:fly={{ y: 20, duration: 200 }}>
      {toast.message}
    </div>
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg-darkest);
  }

  .main-layout {
    display: flex;
    flex: 1;
    overflow: hidden;
    position: relative;
  }

  .main-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--spacing-lg);
    padding: var(--spacing-lg);
    overflow: hidden;
  }

  .decks-section {
    flex-shrink: 0;
    min-height: 0;
    overflow: hidden;
  }

  .preset-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto; /* Fix: enable scroll when content overflows */
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-md);
  }

  .section-header h2 {
    font-size: 14px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent-primary);
    margin: 0;
  }

  .running-count {
    font-size: 12px;
    color: var(--text-muted);
    background: var(--bg-panel);
    padding: 4px 10px;
    border-radius: var(--radius-md);
  }

  .decks-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--spacing-md);
  }

  @media (max-width: 1200px) {
    .decks-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .sidebar {
    width: var(--sidebar-width);
    background: var(--bg-dark);
    border-left: 1px solid var(--border-subtle);
    padding: var(--spacing-lg);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-lg);
    overflow-y: auto;
    flex-shrink: 0;
    max-height: calc(100vh - 52px); /* Fix: limit height to viewport minus header */
  }

  /* Responsive breakpoints for desktop */
  @media (max-width: 1600px) {
    .sidebar {
      width: 260px;
    }
    .sidebar-collapse-toggle {
      right: 260px;
    }
    .main-layout.sidebar-collapsed .sidebar-collapse-toggle {
      right: 0;
    }
  }

  @media (max-width: 1400px) {
    .sidebar {
      width: 240px;
      padding: var(--spacing-md);
      gap: var(--spacing-md);
    }
    .sidebar-collapse-toggle {
      right: 240px;
    }
    .main-layout.sidebar-collapsed .sidebar-collapse-toggle {
      right: 0;
    }
  }

  @media (max-width: 1200px) {
    .sidebar {
      width: 220px;
    }
    .sidebar-collapse-toggle {
      right: 220px;
    }
    .main-layout.sidebar-collapsed .sidebar-collapse-toggle {
      right: 0;
    }
  }

  @media (max-width: 1024px) {
    .sidebar {
      width: 200px;
      padding: var(--spacing-sm);
      gap: var(--spacing-sm);
    }
    .main-content {
      padding: var(--spacing-md);
      gap: var(--spacing-md);
    }
  }

  /* Sidebar collapse toggle (desktop) */
  .sidebar-collapse-toggle {
    position: absolute;
    right: var(--sidebar-width);
    top: 50%;
    transform: translateY(-50%);
    z-index: 50;
    width: 24px;
    height: 48px;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-right: none;
    border-radius: var(--radius-md) 0 0 var(--radius-md);
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .sidebar-collapse-toggle:hover {
    background: var(--bg-elevated);
    color: var(--accent-primary);
  }

  .main-layout.sidebar-collapsed .sidebar-collapse-toggle {
    right: 0;
    border-right: 1px solid var(--border-subtle);
    border-radius: var(--radius-md) 0 0 var(--radius-md);
  }

  /* Collapsed sidebar state (desktop) */
  .sidebar.collapsed {
    width: 0;
    padding: 0;
    overflow: hidden;
    border-left: none;
  }

  /* Mobile sidebar toggle button */
  .sidebar-mobile-toggle {
    display: none;
    position: fixed;
    bottom: var(--spacing-lg);
    right: var(--spacing-lg);
    z-index: 100;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: var(--accent-primary);
    color: var(--bg-darkest);
    border: none;
    box-shadow: 0 4px 12px rgba(0, 240, 255, 0.3);
    cursor: pointer;
    transition: transform 0.2s ease;
  }

  .sidebar-mobile-toggle:hover {
    transform: scale(1.1);
  }

  /* Mobile sidebar close button */
  .sidebar-close-mobile {
    display: none;
    position: absolute;
    top: var(--spacing-md);
    right: var(--spacing-md);
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    color: var(--text-secondary);
    border: none;
    cursor: pointer;
    z-index: 10;
  }

  .sidebar-close-mobile:hover {
    background: var(--status-error);
    color: white;
  }

  /* Mobile sidebar backdrop */
  .sidebar-backdrop {
    display: none;
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 150;
  }

  /* Mobile responsive (< 900px) */
  @media (max-width: 900px) {
    .sidebar-collapse-toggle {
      display: none;
    }

    .sidebar-mobile-toggle {
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .sidebar-backdrop {
      display: block;
    }

    .sidebar {
      position: fixed;
      top: 52px;
      right: 0;
      bottom: 0;
      width: 280px;
      max-height: none;
      z-index: 200;
      transform: translateX(100%);
      transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
    }

    .sidebar.mobile-open {
      transform: translateX(0);
    }

    .sidebar-close-mobile {
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .decks-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  /* Tablet/small desktop (< 768px) */
  @media (max-width: 768px) {
    .decks-grid {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
      gap: var(--spacing-sm);
    }

    .main-content {
      padding: var(--spacing-sm);
      gap: var(--spacing-sm);
    }

    .section-header h2 {
      font-size: 12px;
    }
  }

  /* Mobile (< 600px) */
  @media (max-width: 600px) {
    .decks-grid {
      grid-template-columns: minmax(0, 1fr);
    }

    .sidebar {
      width: 100%;
    }

    .sidebar-mobile-toggle {
      bottom: var(--spacing-md);
      right: var(--spacing-md);
      width: 44px;
      height: 44px;
    }
  }

  .sidebar-section {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    padding: var(--spacing-lg);
  }

  .sidebar-section h3 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent-primary);
    margin-bottom: var(--spacing-md);
  }

  .info-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-sm) 0;
    border-bottom: 1px solid var(--border-subtle);
  }

  .info-row:last-child {
    border-bottom: none;
  }

  .info-row .label {
    font-size: 12px;
    color: var(--text-muted);
  }

  .info-row .value {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .info-row .value.active {
    color: var(--status-active);
  }

  .info-row .value.preset {
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .deck-status-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-xs) var(--spacing-sm);
    margin: 2px 0;
    border-radius: var(--radius-sm);
    font-size: 11px;
    transition: all 0.15s;
  }

  .deck-status-row:hover {
    background: var(--bg-elevated);
  }

  .deck-status-row.selected {
    background: var(--bg-elevated);
    border-left: 2px solid var(--accent-primary);
  }

  .deck-label {
    color: var(--text-secondary);
  }

  .deck-state {
    color: var(--text-muted);
  }

  .deck-status-row.active .deck-state {
    color: var(--status-active);
    font-weight: 600;
  }

  .tips {
    margin-top: auto;
  }

  .tip {
    font-size: 11px;
    color: var(--text-muted);
    margin-bottom: var(--spacing-sm);
  }

  .tip kbd {
    display: inline-block;
    padding: 2px 6px;
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 10px;
    margin-right: 4px;
  }

  /* Toast notification */
  .toast {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    padding: var(--spacing-md) var(--spacing-xl);
    border-radius: var(--radius-md);
    font-size: 13px;
    font-weight: 500;
    z-index: 1000;
    backdrop-filter: blur(10px);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
  }

  .toast.info {
    background: rgba(0, 150, 255, 0.85);
    color: white;
    border: 1px solid rgba(0, 150, 255, 0.5);
  }

  .toast.success {
    background: rgba(0, 200, 100, 0.85);
    color: white;
    border: 1px solid rgba(0, 200, 100, 0.5);
  }

  .toast.error {
    background: rgba(255, 71, 87, 0.85);
    color: white;
    border: 1px solid rgba(255, 71, 87, 0.5);
  }
</style>
