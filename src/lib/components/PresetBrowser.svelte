<script lang="ts">
  import { searchPresets, type PresetMeta } from '$lib/presets/index.js'

  interface Props {
    presets: PresetMeta[]
    isOpen: boolean
    activeDeck: 'A' | 'B'
    playlistAItems: string[]
    playlistBItems: string[]
    onClose: () => void
    onLoadPreset: (name: string) => void
    onAddToPlaylist: (deck: 'A' | 'B', name: string) => void
    targetSlot?: number
  }

  let {
    presets,
    isOpen,
    activeDeck,
    playlistAItems,
    playlistBItems,
    onClose,
    onLoadPreset,
    onAddToPlaylist,
    targetSlot = undefined,
  }: Props = $props()

  // ── Recherche ──────────────────────────────────────────────────────────────
  let searchQuery: string = $state('')
  let debouncedQuery: string = $state('')
  let activeTag: string = $state('')   // '' | '★'

  $effect(() => {
    const q = searchQuery
    const timer = setTimeout(() => { debouncedQuery = q }, 150)
    return () => clearTimeout(timer)
  })

  // ── Favoris (localStorage) ──────────────────────────────────────────────────
  const FAV_KEY = 'od-favorites'

  function loadFavorites(): string[] {
    try { return JSON.parse(localStorage.getItem(FAV_KEY) ?? '[]') }
    catch { return [] }
  }

  let favorites: string[] = $state(loadFavorites())

  function toggleFavorite(name: string) {
    favorites = favorites.includes(name)
      ? favorites.filter((f) => f !== name)
      : [...favorites, name]
    localStorage.setItem(FAV_KEY, JSON.stringify(favorites))
  }

  // ── Filtrage ───────────────────────────────────────────────────────────────
  let filteredPresets: PresetMeta[] = $derived(
    (() => {
      let list = debouncedQuery ? searchPresets(presets, debouncedQuery) : presets
      if (activeTag === '★') list = list.filter((p) => favorites.includes(p.name))
      return list
    })()
  )

  // ── Virtualisation ────────────────────────────────────────────────────────
  const PRESET_ROW_H = 28
  let listEl: HTMLElement | undefined = $state()
  let containerH: number = $state(180)
  let scrollTop: number = $state(0)

  let vStart: number = $derived(Math.max(0, Math.floor(scrollTop / PRESET_ROW_H) - 5))
  let vEnd: number = $derived(Math.min(filteredPresets.length, vStart + Math.ceil(containerH / PRESET_ROW_H) + 10))

  $effect(() => {
    filteredPresets // track
    scrollTop = 0
    if (listEl) listEl.scrollTop = 0
  })

  function onScroll(e: Event) {
    scrollTop = (e.currentTarget as HTMLElement).scrollTop
  }

  // ── Fermeture clavier ──────────────────────────────────────────────────────
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose()
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="preset-drawer" class:preset-drawer--open={isOpen} aria-hidden={!isOpen}>
  <div class="preset-drawer__bar">
    <span class="preset-drawer__title">
      Presets → {targetSlot !== undefined ? `Deck ${targetSlot + 1}` : `Deck ${activeDeck}`}
      <span class="preset-drawer__count">({filteredPresets.length}/{presets.length})</span>
    </span>

    <div class="preset-drawer__filters">
      <button
        class="tag-chip"
        class:tag-active={activeTag === ''}
        onclick={() => { activeTag = '' }}
        type="button"
      >Tous</button>
      <button
        class="tag-chip"
        class:tag-active={activeTag === '★'}
        onclick={() => { activeTag = activeTag === '★' ? '' : '★' }}
        type="button"
      >★ Favoris</button>
    </div>

    <input
      class="search-input"
      type="search"
      placeholder="Search presets…"
      bind:value={searchQuery}
    />

    <button class="preset-drawer__close" onclick={onClose} type="button" aria-label="Fermer">✕</button>
  </div>

  <ul
    class="preset-list"
    bind:this={listEl}
    bind:clientHeight={containerH}
    onscroll={onScroll}
  >
    <li style="height:{vStart * PRESET_ROW_H}px" aria-hidden="true"></li>

    {#each filteredPresets.slice(vStart, vEnd) as p (p.name)}
      {@const isFav = favorites.includes(p.name)}
      <li class="preset-row">
        <button
          class="fav-btn"
          class:fav-on={isFav}
          onclick={() => toggleFavorite(p.name)}
          type="button"
          aria-label={isFav ? 'Retirer des favoris' : 'Ajouter aux favoris'}
        >★</button>

        <button
          class="preset-item"
          onclick={() => onLoadPreset(p.name)}
          type="button"
          title={p.name}
        >{p.name}</button>

        <button
          class="pl-add"
          class:in-list={playlistAItems.includes(p.name)}
          onclick={() => onAddToPlaylist('A', p.name)}
          type="button"
        >A</button>
        <button
          class="pl-add"
          class:in-list={playlistBItems.includes(p.name)}
          onclick={() => onAddToPlaylist('B', p.name)}
          type="button"
        >B</button>
      </li>
    {/each}

    <li style="height:{Math.max(0, filteredPresets.length - vEnd) * PRESET_ROW_H}px" aria-hidden="true"></li>
  </ul>
</div>

<style>
  .preset-drawer {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    height: 260px;
    background: #0d0d1a;
    border-top: 2px solid #ff2d78;
    display: flex;
    flex-direction: column;
    transform: translateY(100%);
    transition: transform 200ms ease;
    z-index: 100;
  }

  .preset-drawer--open {
    transform: translateY(0);
  }

  .preset-drawer__bar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 6px 12px;
    border-bottom: 1px solid #131330;
    flex-shrink: 0;
  }

  .preset-drawer__title {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #ff2d78;
    flex-shrink: 0;
  }

  .preset-drawer__count {
    font-weight: 400;
    color: #44447a;
  }

  .preset-drawer__filters {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .tag-chip {
    background: #0e0e26;
    border: 1px solid #1e1e48;
    border-radius: 3px;
    color: #44447a;
    font-size: 9px;
    font-weight: 700;
    padding: 2px 6px;
    cursor: pointer;
    transition: all 150ms ease;
  }

  .tag-chip:hover { border-color: #ff2d78; color: #ff2d78; }
  .tag-active { border-color: #ff2d78; color: #ff2d78; background: #1a0a22; }

  .search-input {
    flex: 1;
    background: #0e0e26;
    border: 1px solid #1e1e48;
    border-radius: 3px;
    color: #ccccee;
    font-size: 11px;
    padding: 3px 6px;
    outline: none;
  }

  .search-input:focus { border-color: #ff2d78; }

  .preset-drawer__close {
    background: none;
    border: none;
    color: #44447a;
    cursor: pointer;
    font-size: 14px;
    padding: 0 4px;
    flex-shrink: 0;
    transition: color 150ms ease;
  }

  .preset-drawer__close:hover { color: #ff2d78; }

  /* Liste virtualisée */
  .preset-list {
    flex: 1;
    overflow-y: auto;
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .preset-row {
    display: flex;
    align-items: center;
    height: 28px;
    padding: 0 8px;
    gap: 4px;
  }

  .preset-row:hover { background: #111130; }

  .fav-btn {
    background: none;
    border: none;
    color: #33335a;
    cursor: pointer;
    font-size: 11px;
    padding: 0 2px;
    flex-shrink: 0;
    transition: color 150ms ease;
  }

  .fav-on { color: #ff2d78; }

  .preset-item {
    flex: 1;
    background: none;
    border: none;
    color: #8888bb;
    font-size: 11px;
    text-align: left;
    cursor: pointer;
    padding: 0 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: color 150ms ease;
  }

  .preset-item:hover { color: #eeeeff; }

  .pl-add {
    background: #0e0e26;
    border: 1px solid #1e1e48;
    border-radius: 3px;
    color: #44447a;
    font-size: 9px;
    font-weight: 700;
    padding: 1px 5px;
    cursor: pointer;
    flex-shrink: 0;
    transition: all 150ms ease;
  }

  .pl-add:hover { border-color: #ff2d78; color: #ff2d78; }
  .in-list { border-color: #ff2d78; color: #ff2d78; background: #1a0a22; }
</style>
