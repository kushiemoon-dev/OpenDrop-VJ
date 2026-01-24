<script>
  import PresetCard from './PresetCard.svelte';
  import { isFavorite, getFavoriteCount } from '$lib/stores/favorites';
  import { extractCategory, buildCategoriesIndex, getCategoriesWithCounts } from '$lib/stores/categories';
  import { getAllTags, hasTag, getTagCount } from '$lib/stores/tags';
  import { ChevronDown, Star, Search, X } from 'lucide-svelte';

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
  let showFavoritesOnly = $state(false);
  let selectedCategory = $state('all');
  /** @type {string[]} */
  let selectedTags = $state([]);

  // Get favorite count for display
  let favoriteCount = $derived(getFavoriteCount());

  // Build categories index when presets change
  $effect(() => {
    if (presets.length > 0) {
      buildCategoriesIndex(presets);
    }
  });

  // Get categories with counts
  let categoriesWithCounts = $derived(getCategoriesWithCounts());

  // Get all available tags
  let availableTags = $derived(getAllTags());
  let tagCount = $derived(getTagCount());

  /**
   * Toggle a tag in the selected tags filter
   * @param {string} tag
   */
  function toggleTagFilter(tag) {
    if (selectedTags.includes(tag)) {
      selectedTags = selectedTags.filter((t) => t !== tag);
    } else {
      selectedTags = [...selectedTags, tag];
    }
  }

  // Filter presets by search, favorites, category, and tags
  let filteredPresets = $derived(
    presets
      .filter((/** @type {Preset} */ p) => {
        const matchesSearch = p.name.toLowerCase().includes(search.toLowerCase());
        const matchesFavorites = !showFavoritesOnly || isFavorite(p.path);
        const matchesCategory = selectedCategory === 'all' || extractCategory(p.path) === selectedCategory;
        const matchesTags = selectedTags.length === 0 || selectedTags.every((tag) => hasTag(p.path, tag));
        return matchesSearch && matchesFavorites && matchesCategory && matchesTags;
      })
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
      <span class="chevron" class:rotated={!expanded}>
        <ChevronDown size={16} />
      </span>
      <h3>Presets</h3>
      <span class="count">{presets.length}</span>
    </button>

    <button
      class="favorites-filter-btn"
      class:active={showFavoritesOnly}
      onclick={() => showFavoritesOnly = !showFavoritesOnly}
      title={showFavoritesOnly ? 'Show all presets' : 'Show favorites only'}
      disabled={!expanded}
    >
      <Star size={14} fill={showFavoritesOnly ? 'currentColor' : 'none'} />
      {#if favoriteCount > 0}
        <span class="fav-count">{favoriteCount}</span>
      {/if}
    </button>

    {#if categoriesWithCounts.length > 1}
      <select
        class="category-select"
        bind:value={selectedCategory}
        disabled={!expanded}
        title="Filter by category"
      >
        <option value="all">All Categories</option>
        {#each categoriesWithCounts as [category, count]}
          <option value={category}>{category} ({count})</option>
        {/each}
      </select>
    {/if}

    {#if tagCount > 0}
      <div class="tag-filters" class:disabled={!expanded}>
        {#each availableTags.slice(0, 5) as tag}
          <button
            class="tag-chip"
            class:active={selectedTags.includes(tag)}
            onclick={() => toggleTagFilter(tag)}
            disabled={!expanded}
          >
            {tag}
          </button>
        {/each}
        {#if availableTags.length > 5}
          <span class="tag-more">+{availableTags.length - 5}</span>
        {/if}
      </div>
    {/if}

    <div class="search-bar">
      <Search size={14} />
      <input
        type="text"
        bind:value={search}
        placeholder="Search presets..."
        disabled={!expanded}
      />
      {#if search}
        <button class="clear-btn" onclick={() => search = ''} title="Clear search" aria-label="Clear search">
          <X size={12} />
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
          {#if showFavoritesOnly && favoriteCount === 0}
            <Star size={24} strokeWidth={1.5} class="empty-star" />
            <span>No favorites yet</span>
            <span class="hint">Click the star on any preset to add it</span>
          {:else if showFavoritesOnly && search}
            <span>No favorites match "{search}"</span>
          {:else if selectedCategory !== 'all' && search}
            <span>No presets in "{selectedCategory}" match "{search}"</span>
          {:else if selectedCategory !== 'all'}
            <span>No presets in "{selectedCategory}"</span>
          {:else if search}
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

  .toggle-btn .chevron {
    display: flex;
    transition: var(--transition-fast);
  }

  .toggle-btn .chevron.rotated {
    transform: rotate(-90deg);
  }

  .toggle-btn h3 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent-primary);
  }

  .count {
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-dark);
    padding: 2px 8px;
    border-radius: 10px;
  }

  .favorites-filter-btn {
    display: flex;
    align-items: center;
    gap: var(--spacing-xs);
    padding: var(--spacing-xs) var(--spacing-sm);
    border-radius: var(--radius-md);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    color: var(--text-muted);
    font-size: 11px;
    transition: all 0.15s ease;
  }

  .favorites-filter-btn:hover:not(:disabled) {
    background: var(--bg-elevated);
    border-color: var(--border-medium);
    color: var(--accent-yellow, #fbbf24);
  }

  .favorites-filter-btn.active {
    background: var(--accent-yellow, #fbbf24);
    border-color: var(--accent-yellow, #fbbf24);
    color: var(--bg-darkest);
  }

  .favorites-filter-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .fav-count {
    background: var(--bg-panel);
    padding: 1px 5px;
    border-radius: 8px;
    font-weight: 600;
  }

  .favorites-filter-btn.active .fav-count {
    background: rgba(0, 0, 0, 0.2);
  }

  .category-select {
    padding: var(--spacing-xs) var(--spacing-sm);
    border-radius: var(--radius-md);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    font-size: 11px;
    cursor: pointer;
    max-width: 150px;
    transition: all 0.15s ease;
  }

  .category-select:hover:not(:disabled) {
    background: var(--bg-elevated);
    border-color: var(--border-medium);
    color: var(--text-primary);
  }

  .category-select:focus {
    outline: none;
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 2px rgba(0, 240, 255, 0.2);
  }

  .category-select:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .category-select option {
    background: var(--bg-panel);
    color: var(--text-primary);
    padding: var(--spacing-sm);
  }

  .tag-filters {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: nowrap;
    overflow-x: auto;
    max-width: 200px;
  }

  .tag-filters.disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  .tag-chip {
    font-size: 10px;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    color: var(--text-muted);
    white-space: nowrap;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .tag-chip:hover:not(:disabled) {
    background: var(--bg-elevated);
    border-color: var(--border-medium);
    color: var(--text-primary);
  }

  .tag-chip.active {
    background: var(--accent-purple, #8b5cf6);
    border-color: var(--accent-purple, #8b5cf6);
    color: white;
  }

  .tag-chip:disabled {
    cursor: not-allowed;
  }

  .tag-more {
    font-size: 10px;
    color: var(--text-muted);
    white-space: nowrap;
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

  .search-bar :global(svg) {
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
    background: var(--accent-primary);
  }

  .loading, .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-xl);
    color: var(--text-muted);
    font-size: 13px;
  }

  .empty .hint {
    font-size: 11px;
    opacity: 0.7;
  }

  .empty :global(.empty-star) {
    opacity: 0.5;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--border-subtle);
    border-top-color: var(--accent-primary);
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
