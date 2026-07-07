<script lang="ts">
  import { searchPresets, getSlug, type PresetMeta } from '$lib/presets/index.js'
  import PresetTile from './PresetTile.svelte'
  import { computeGrid, type GridWindow } from '$lib/presets/grid-virtual.js'
  import { thumbUrls, requestThumb, releaseThumb } from '$lib/presets/thumbnailer.svelte.js'
  import { loadFavColors, saveFavColors, FAV_COLORS } from '$lib/presets/favorites.js'

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
    variant?: 'list' | 'grid'
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
    variant = 'list',
  }: Props = $props()

  // ── Search ─────────────────────────────────────────────────────────────────
  let searchQuery: string = $state('')
  let debouncedQuery: string = $state('')
  // 0 = all, -1 = all favorites (any color), 1-5 = specific color
  let activeColorFilter: number = $state(0)

  $effect(() => {
    const q = searchQuery
    const timer = setTimeout(() => { debouncedQuery = q }, 150)
    return () => clearTimeout(timer)
  })

  // ── Color favorites (localStorage) ──────────────────────────────────────────
  let favColors: Record<string, number> = $state(loadFavColors())

  function setFavColor(name: string, color: number) {
    const next = { ...favColors }
    if (color === 0) delete next[name]
    else next[name] = color
    favColors = next
    saveFavColors(next)
  }

  // ── Preset notes (localStorage) ──────────────────────────────────────────────
  const NOTE_KEY = 'od-preset-notes'
  function loadNotes(): Record<string, string> {
    try { return JSON.parse(localStorage.getItem(NOTE_KEY) ?? '{}') }
    catch { return {} }
  }

  let notes: Record<string, string> = $state(loadNotes())
  let selectedName: string = $state('')

  function setNote(name: string, text: string) {
    const next = { ...notes }
    if (text) next[name] = text
    else delete next[name]
    notes = next
    localStorage.setItem(NOTE_KEY, JSON.stringify(next))
  }

  // ── Filtering ──────────────────────────────────────────────────────────────
  let filteredPresets: PresetMeta[] = $derived(
    (() => {
      let list = debouncedQuery ? searchPresets(presets, debouncedQuery) : presets
      if (activeColorFilter === -1) list = list.filter((p) => (favColors[p.name] ?? 0) > 0)
      else if (activeColorFilter >= 1) list = list.filter((p) => favColors[p.name] === activeColorFilter)
      return list
    })()
  )

  // ── Virtualization ─────────────────────────────────────────────────────────
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
    gridScrollTop = 0
    if (gridEl) gridEl.scrollTop = 0
  })

  function onScroll(e: Event) {
    scrollTop = (e.currentTarget as HTMLElement).scrollTop
  }

  // ── Grid virtualization ───────────────────────────────────────────────────
  const CARD_MIN_W = 120
  const CARD_H = 120   // approximate height (thumb 16/9 ~80px + name ~16px + footer ~24px)
  const GRID_GAP = 8

  let gridEl: HTMLElement | undefined = $state()
  let gridW: number = $state(600)
  let gridH: number = $state(300)
  let gridScrollTop: number = $state(0)

  let gridWindow: GridWindow = $derived(
    computeGrid({
      count: filteredPresets.length,
      containerW: gridW,
      containerH: gridH,
      scrollTop: gridScrollTop,
      cardMinW: CARD_MIN_W,
      cardH: CARD_H,
      gap: GRID_GAP,
      overscanRows: 2,
    })
  )

  // Precomputed name→slug map for O(1) lookups
  let slugMap: Map<string, string> = $derived(
    new Map(filteredPresets.map(p => [p.name, getSlug(p.name) ?? p.name]))
  )

  function onGridScroll(e: Event) {
    gridScrollTop = (e.currentTarget as HTMLElement).scrollTop
  }

  // ── Keyboard close ─────────────────────────────────────────────────────────
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
        class:tag-active={activeColorFilter === 0}
        onclick={() => { activeColorFilter = 0 }}
        type="button"
      >All</button>
      <button
        class="tag-chip"
        class:tag-active={activeColorFilter === -1}
        onclick={() => { activeColorFilter = activeColorFilter === -1 ? 0 : -1 }}
        type="button"
      >★</button>
      {#each [1, 2, 3, 4, 5] as c}
        <button
          class="tag-chip fav-chip"
          class:tag-active={activeColorFilter === c}
          style:background={activeColorFilter === c ? FAV_COLORS[c] + '33' : undefined}
          style:border-color={activeColorFilter === c ? FAV_COLORS[c] : undefined}
          onclick={() => { activeColorFilter = activeColorFilter === c ? 0 : c }}
          type="button"
          aria-label={`Filter color ${c}`}
        ><span class="fav-dot" style:background={FAV_COLORS[c]}></span></button>
      {/each}
    </div>

    {#if selectedName}
      <input
        class="note-input"
        type="text"
        placeholder="Note…"
        value={notes[selectedName] ?? ''}
        oninput={(e) => setNote(selectedName, e.currentTarget.value)}
        title={selectedName}
      />
    {/if}

    <input
      class="search-input"
      type="search"
      placeholder="Search presets…"
      bind:value={searchQuery}
    />

    <button class="preset-drawer__close" onclick={onClose} type="button" aria-label="Close">✕</button>
  </div>

  {#if variant === 'grid'}
    <div
      class="preset-grid"
      bind:this={gridEl}
      bind:clientWidth={gridW}
      bind:clientHeight={gridH}
      onscroll={onGridScroll}
    >
      <div class="preset-grid__sizer" style="height:{gridWindow.totalH}px">
        <div
          class="preset-grid__inner"
          style="transform:translateY({gridWindow.offsetY}px); grid-template-columns:repeat({gridWindow.cols},minmax(0,1fr));"
        >
          {#each filteredPresets.slice(gridWindow.vStart, gridWindow.vEnd) as p (slugMap.get(p.name) ?? p.name)}
            {@const slug = slugMap.get(p.name) ?? p.name}
            <PresetTile
              preset={p}
              {slug}
              thumbUrl={thumbUrls.get(slug)}
              favColor={favColors[p.name] ?? 0}
              inA={playlistAItems.includes(p.name)}
              inB={playlistBItems.includes(p.name)}
              onLoad={() => { selectedName = p.name; onLoadPreset(p.name); }}
              onSetFavColor={(c) => setFavColor(p.name, c)}
              onAddA={() => onAddToPlaylist('A', p.name)}
              onAddB={() => onAddToPlaylist('B', p.name)}
              onVisible={() => requestThumb(slug, p.name)}
              onHidden={() => releaseThumb(slug)}
            />
          {/each}
        </div>
      </div>
    </div>
  {:else}
    <ul
      class="preset-list"
      bind:this={listEl}
      bind:clientHeight={containerH}
      onscroll={onScroll}
    >
      <li style="height:{vStart * PRESET_ROW_H}px" aria-hidden="true"></li>

      {#each filteredPresets.slice(vStart, vEnd) as p (p.name)}
        {@const fc = favColors[p.name] ?? 0}
        <li class="preset-row">
          <button
            class="fav-btn"
            class:fav-on={fc > 0}
            style:color={FAV_COLORS[fc] || undefined}
            onclick={() => setFavColor(p.name, (fc + 1) % 6)}
            type="button"
            aria-label={fc > 0 ? 'Change favorite color' : 'Add to favorites'}
          >★</button>

          <button
            class="preset-item"
            onclick={() => { selectedName = p.name; onLoadPreset(p.name); }}
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
  {/if}
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

  .fav-chip { padding: 2px 5px; }
  .fav-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; }

  .note-input {
    flex: 1;
    min-width: 0;
    background: #0e0e26;
    border: 1px solid #1e1e48;
    border-radius: 3px;
    color: #ccccee;
    font-size: 10px;
    padding: 2px 6px;
    outline: none;
  }
  .note-input:focus { border-color: #ff2d78; }

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

  /* Virtualized list */
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

  /* Virtualized preset grid */
  .preset-grid {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .preset-grid__sizer {
    position: relative;
    width: 100%;
  }

  .preset-grid__inner {
    position: absolute;
    inset: 0 0 auto 0;
    display: grid;
    gap: 8px;
    padding: 8px;
  }
</style>
