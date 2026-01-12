<script>
  import PresetCard from './PresetCard.svelte';

  /**
   * @typedef {{ name: string, path: string }} Preset
   */

  /**
   * @type {{
   *   presets?: Preset[],
   *   currentPreset?: string,
   *   loading?: boolean,
   *   onSelect?: (preset: Preset) => void,
   *   onLoad?: (path: string) => void,
   *   onAddToPlaylist?: (preset: Preset) => void
   * }}
   */
  let {
    presets = [],
    currentPreset = '',
    loading = false,
    onSelect,
    onLoad,
    onAddToPlaylist
  } = $props();

  let search = $state('');
  let expanded = $state(true);

  // Filter presets by search
  let filteredPresets = $derived(
    presets
      .filter((/** @type {Preset} */ p) => p.name.toLowerCase().includes(search.toLowerCase()))
      .slice(0, 50)
  );

  /** @param {Preset} preset */
  function handleSelect(preset) {
    onSelect?.(preset);
  }

  /** @param {Preset} preset */
  function handleDoubleClick(preset) {
    onSelect?.(preset);
    onLoad?.(preset.path);
  }
</script>

<div class="preset-browser" class:collapsed={!expanded}>
  <div class="browser-header">
    <button class="toggle-btn" onclick={() => expanded = !expanded}>
      <svg
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        class:rotated={!expanded}
      >
        <polyline points="6,9 12,15 18,9" />
      </svg>
      <h3>Presets</h3>
      <span class="count">{presets.length}</span>
    </button>

    <div class="search-bar">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="11" cy="11" r="8" />
        <path d="m21 21-4.35-4.35" />
      </svg>
      <input
        type="text"
        bind:value={search}
        placeholder="Search presets..."
        disabled={!expanded}
      />
      {#if search}
        <button class="clear-btn" onclick={() => search = ''} title="Clear search" aria-label="Clear search">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      {/if}
    </div>
  </div>

  {#if expanded}
    <div class="browser-content">
      {#if loading}
        <div class="loading">
          <div class="spinner"></div>
          <span>Loading presets...</span>
        </div>
      {:else if filteredPresets.length === 0}
        <div class="empty">
          {#if search}
            <span>No presets match "{search}"</span>
          {:else}
            <span>No presets loaded</span>
          {/if}
        </div>
      {:else}
        <div class="preset-grid">
          {#each filteredPresets as preset (preset.path)}
            <PresetCard
              name={preset.name}
              path={preset.path}
              selected={currentPreset === preset.path}
              onclick={() => handleSelect(preset)}
              ondblclick={() => handleDoubleClick(preset)}
              onAddToPlaylist={onAddToPlaylist}
            />
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .preset-browser {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    transition: var(--transition-normal);
  }

  .preset-browser.collapsed {
    max-height: 48px;
  }

  .browser-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    padding: var(--spacing-md) var(--spacing-lg);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .toggle-btn {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    color: var(--text-primary);
    flex-shrink: 0;
  }

  .toggle-btn svg {
    transition: var(--transition-fast);
  }

  .toggle-btn svg.rotated {
    transform: rotate(-90deg);
  }

  .toggle-btn h3 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent-cyan);
  }

  .count {
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-dark);
    padding: 2px 8px;
    border-radius: 10px;
  }

  .search-bar {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 0 var(--spacing-md);
    max-width: 300px;
  }

  .search-bar svg {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .search-bar input {
    flex: 1;
    background: none;
    border: none;
    padding: var(--spacing-sm) 0;
    font-size: 12px;
  }

  .search-bar input:focus {
    outline: none;
    box-shadow: none;
  }

  .clear-btn {
    color: var(--text-muted);
    padding: 2px;
    border-radius: 4px;
  }

  .clear-btn:hover {
    color: var(--text-primary);
    background: var(--bg-elevated);
  }

  .browser-content {
    flex: 1;
    overflow: hidden;
    padding: var(--spacing-md);
    animation: fade-in 0.2s ease;
  }

  .preset-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: var(--spacing-sm);
    max-height: 200px;
    overflow-y: auto;
    padding-right: var(--spacing-xs);
  }

  .preset-grid::-webkit-scrollbar {
    width: 4px;
  }

  .preset-grid::-webkit-scrollbar-track {
    background: var(--bg-dark);
    border-radius: 2px;
  }

  .preset-grid::-webkit-scrollbar-thumb {
    background: var(--border-medium);
    border-radius: 2px;
  }

  .preset-grid::-webkit-scrollbar-thumb:hover {
    background: var(--accent-cyan);
  }

  .loading, .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-md);
    padding: var(--spacing-xl);
    color: var(--text-muted);
    font-size: 13px;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--border-subtle);
    border-top-color: var(--accent-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }
</style>
