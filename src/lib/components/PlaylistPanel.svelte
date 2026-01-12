<script>
  import { invoke } from "@tauri-apps/api/core";

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
      console.error("Failed to add to playlist:", e);
    }
  }

  /** @param {number} index */
  async function removeFromPlaylist(index) {
    try {
      await invoke("playlist_remove", { deckId, index });
      onUpdate?.();
    } catch (e) {
      console.error("Failed to remove from playlist:", e);
    }
  }

  async function clearPlaylist() {
    try {
      await invoke("playlist_clear", { deckId });
      onUpdate?.();
    } catch (e) {
      console.error("Failed to clear playlist:", e);
    }
  }

  async function playNext() {
    try {
      await invoke("playlist_next", { deckId });
      onUpdate?.();
    } catch (e) {
      console.error("Failed to play next:", e);
    }
  }

  async function playPrevious() {
    try {
      await invoke("playlist_previous", { deckId });
      onUpdate?.();
    } catch (e) {
      console.error("Failed to play previous:", e);
    }
  }

  /** @param {number} index */
  async function jumpTo(index) {
    try {
      await invoke("playlist_jump_to", { deckId, index });
      onUpdate?.();
    } catch (e) {
      console.error("Failed to jump to:", e);
    }
  }

  async function toggleShuffle() {
    try {
      await invoke("playlist_set_settings", { deckId, shuffle: !playlist.shuffle });
      onUpdate?.();
    } catch (e) {
      console.error("Failed to toggle shuffle:", e);
    }
  }

  async function toggleAutoCycle() {
    try {
      await invoke("playlist_set_settings", { deckId, autoCycle: !playlist.auto_cycle });
      onUpdate?.();
    } catch (e) {
      console.error("Failed to toggle auto-cycle:", e);
    }
  }

  async function updateCycleDuration() {
    try {
      await invoke("playlist_set_settings", { deckId, cycleDurationSecs: cycleDuration });
      onUpdate?.();
    } catch (e) {
      console.error("Failed to update cycle duration:", e);
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
      <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
        <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/>
      </svg>
    </button>
    <button class="ctrl-btn" onclick={playNext} disabled={playlist.items.length === 0} title="Next">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
        <path d="M6 18l8.5-6L6 6v12zm10-12v12h2V6h-2z"/>
      </svg>
    </button>
    <button
      class="ctrl-btn"
      class:active={playlist.shuffle}
      onclick={toggleShuffle}
      title="Shuffle"
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <polyline points="16 3 21 3 21 8"/>
        <line x1="4" y1="20" x2="21" y2="3"/>
        <polyline points="21 16 21 21 16 21"/>
        <line x1="15" y1="15" x2="21" y2="21"/>
        <line x1="4" y1="4" x2="9" y2="9"/>
      </svg>
    </button>
    <button
      class="ctrl-btn"
      class:active={playlist.auto_cycle}
      onclick={toggleAutoCycle}
      title="Auto-cycle"
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M21 12a9 9 0 11-6.219-8.56"/>
        <polyline points="22 2 22 8 16 8"/>
      </svg>
    </button>
    <button class="ctrl-btn danger" onclick={clearPlaylist} disabled={playlist.items.length === 0} title="Clear all">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <polyline points="3 6 5 6 21 6"/>
        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
      </svg>
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
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" opacity="0.3">
          <path d="M9 18V5l12-2v13"/>
          <circle cx="6" cy="18" r="3"/>
          <circle cx="18" cy="16" r="3"/>
        </svg>
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
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M18 6L6 18M6 6l12 12"/>
            </svg>
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
