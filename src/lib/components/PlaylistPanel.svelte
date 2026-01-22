<script>
  import { invoke } from "@tauri-apps/api/core";
  import { showToast } from "$lib/stores/toast";
  import { SkipBack, SkipForward, Shuffle, RefreshCw, Trash2, Music, X } from 'lucide-svelte';

  /**
   * @typedef {{ name: string, path: string }} PlaylistItem
   * @typedef {{
   *   name: string,
   *   items: PlaylistItem[],
   *   current_index: number,
   *   shuffle: boolean,
   *   auto_cycle: boolean,
   *   cycle_duration_secs: number
   * }} Playlist
   */

  /**
   * @type {{
   *   deckId?: number,
   *   playlist?: Playlist,
   *   running?: boolean,
   *   onUpdate?: () => void
   * }}
   */
  let {
    deckId = 0,
    playlist = { name: '', items: [], current_index: 0, shuffle: false, auto_cycle: false, cycle_duration_secs: 30 },
    running = false,
    onUpdate
  } = $props();

  let cycleDuration = $state(30);

  // Sync cycle duration from playlist
  $effect(() => {
    cycleDuration = playlist.cycle_duration_secs;
  });

  /**
   * @param {string} name
   * @param {string} path
   */
  async function addToPlaylist(name, path) {
    try {
      await invoke("playlist_add", { deckId, name, path });
      onUpdate?.();
    } catch (e) {
      showToast("Failed to add to playlist", "error");
    }
  }

  /** @param {number} index */
  async function removeFromPlaylist(index) {
    try {
      await invoke("playlist_remove", { deckId, index });
      onUpdate?.();
    } catch (e) {
      showToast("Failed to remove from playlist", "error");
    }
  }

  async function clearPlaylist() {
    try {
      await invoke("playlist_clear", { deckId });
      onUpdate?.();
    } catch (e) {
      showToast("Failed to clear playlist", "error");
    }
  }

  async function playNext() {
    try {
      await invoke("playlist_next", { deckId });
      onUpdate?.();
    } catch (e) {
      showToast("Failed to play next", "error");
    }
  }

  async function playPrevious() {
    try {
      await invoke("playlist_previous", { deckId });
      onUpdate?.();
    } catch (e) {
      showToast("Failed to play previous", "error");
    }
  }

  /** @param {number} index */
  async function jumpTo(index) {
    try {
      await invoke("playlist_jump_to", { deckId, index });
      onUpdate?.();
    } catch (e) {
      showToast("Failed to jump to preset", "error");
    }
  }

  async function toggleShuffle() {
    try {
      await invoke("playlist_set_settings", { deckId, shuffle: !playlist.shuffle });
      onUpdate?.();
    } catch (e) {
      showToast("Failed to toggle shuffle", "error");
    }
  }

  async function toggleAutoCycle() {
    try {
      await invoke("playlist_set_settings", { deckId, autoCycle: !playlist.auto_cycle });
      onUpdate?.();
    } catch (e) {
      showToast("Failed to toggle auto-cycle", "error");
    }
  }

  async function updateCycleDuration() {
    try {
      await invoke("playlist_set_settings", { deckId, cycleDurationSecs: cycleDuration });
      onUpdate?.();
    } catch (e) {
      showToast("Failed to update cycle duration", "error");
    }
  }

  /**
   * @param {string} name
   * @param {number} [maxLen=25]
   */
  function truncateName(name, maxLen = 25) {
    if (name.length > maxLen) {
      return name.slice(0, maxLen - 3) + '...';
    }
    return name;
  }
</script>

<div class="playlist-panel">
  <div class="playlist-header">
    <h3>Playlist - Deck {deckId + 1}</h3>
    <span class="count">{playlist.items.length} presets</span>
  </div>

  <div class="playlist-controls">
    <button class="ctrl-btn" onclick={playPrevious} disabled={playlist.items.length === 0} title="Previous">
      <SkipBack size={14} />
    </button>
    <button class="ctrl-btn" onclick={playNext} disabled={playlist.items.length === 0} title="Next">
      <SkipForward size={14} />
    </button>
    <button
      class="ctrl-btn"
      class:active={playlist.shuffle}
      onclick={toggleShuffle}
      title="Shuffle"
    >
      <Shuffle size={14} />
    </button>
    <button
      class="ctrl-btn"
      class:active={playlist.auto_cycle}
      onclick={toggleAutoCycle}
      title="Auto-cycle"
    >
      <RefreshCw size={14} />
    </button>
    <button class="ctrl-btn danger" onclick={clearPlaylist} disabled={playlist.items.length === 0} title="Clear all">
      <Trash2 size={14} />
    </button>
  </div>

  {#if playlist.auto_cycle}
    <div class="cycle-settings">
      <label>
        <span>Cycle every</span>
        <input
          type="number"
          min="5"
          max="300"
          bind:value={cycleDuration}
          onchange={updateCycleDuration}
        />
        <span>sec</span>
      </label>
    </div>
  {/if}

  <div class="playlist-items">
    {#if playlist.items.length === 0}
      <div class="empty-state">
        <Music size={24} strokeWidth={1.5} class="empty-icon" />
        <span>No presets in playlist</span>
        <span class="hint">Click + on presets to add</span>
      </div>
    {:else}
      {#each playlist.items as item, index (index)}
        <div
          class="playlist-item"
          class:current={index === playlist.current_index}
          class:playing={index === playlist.current_index && running}
        >
          <span class="item-index">{index + 1}</span>
          <button class="item-name" onclick={() => jumpTo(index)} title={item.path}>
            {truncateName(item.name)}
          </button>
          <button class="item-remove" onclick={() => removeFromPlaylist(index)} title="Remove">
            <X size={12} />
          </button>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .playlist-panel {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .playlist-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-md) var(--spacing-lg);
    border-bottom: 1px solid var(--border-subtle);
  }

  .playlist-header h3 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent-cyan);
    margin: 0;
  }

  .count {
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-dark);
    padding: 2px 8px;
    border-radius: 10px;
  }

  .playlist-controls {
    display: flex;
    gap: var(--spacing-xs);
    padding: var(--spacing-sm) var(--spacing-lg);
    border-bottom: 1px solid var(--border-subtle);
  }

  .ctrl-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
    background: var(--bg-dark);
    color: var(--text-secondary);
    transition: all 0.15s;
  }

  .ctrl-btn:hover:not(:disabled) {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .ctrl-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .ctrl-btn.active {
    background: var(--accent-cyan);
    color: var(--bg-darkest);
  }

  .ctrl-btn.danger:hover:not(:disabled) {
    background: var(--status-error);
    color: white;
  }

  .cycle-settings {
    padding: var(--spacing-sm) var(--spacing-lg);
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-dark);
  }

  .cycle-settings label {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    font-size: 11px;
    color: var(--text-muted);
  }

  .cycle-settings input {
    width: 50px;
    padding: 4px 6px;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 11px;
    text-align: center;
  }

  .playlist-items {
    flex: 1;
    overflow-y: auto;
    max-height: clamp(120px, 20vh, 200px); /* Adaptive height based on viewport */
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-xl);
    color: var(--text-muted);
    font-size: 12px;
  }

  .empty-state .hint {
    font-size: 10px;
    opacity: 0.6;
  }

  .empty-state :global(.empty-icon) {
    opacity: 0.3;
  }

  .playlist-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-xs) var(--spacing-lg);
    border-bottom: 1px solid var(--border-subtle);
    transition: background 0.15s;
  }

  .playlist-item:hover {
    background: var(--bg-elevated);
  }

  .playlist-item.current {
    background: rgba(0, 240, 255, 0.1);
    border-left: 2px solid var(--accent-cyan);
  }

  .playlist-item.playing .item-index {
    animation: pulse-glow 1s ease-in-out infinite;
  }

  @keyframes pulse-glow {
    0%, 100% { color: var(--accent-cyan); }
    50% { color: var(--status-active); }
  }

  .item-index {
    font-size: 10px;
    color: var(--text-muted);
    min-width: 16px;
    text-align: center;
  }

  .item-name {
    flex: 1;
    font-size: 11px;
    color: var(--text-secondary);
    text-align: left;
    background: none;
    border: none;
    padding: 4px 0;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-name:hover {
    color: var(--text-primary);
  }

  .playlist-item.current .item-name {
    color: var(--accent-cyan);
    font-weight: 500;
  }

  .item-remove {
    opacity: 0;
    padding: 4px;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    background: none;
    transition: all 0.15s;
  }

  .playlist-item:hover .item-remove {
    opacity: 1;
  }

  .item-remove:hover {
    background: var(--status-error);
    color: white;
  }

  .playlist-items::-webkit-scrollbar {
    width: 4px;
  }

  .playlist-items::-webkit-scrollbar-track {
    background: var(--bg-dark);
  }

  .playlist-items::-webkit-scrollbar-thumb {
    background: var(--border-medium);
    border-radius: 2px;
  }
</style>
