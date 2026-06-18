<!-- src/lib/components/MixerLayout.svelte -->
<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { PresetMeta } from '$lib/presets/index.js'
  import MixerDeckCard from './MixerDeckCard.svelte'
  import MixerCrossfader from './MixerCrossfader.svelte'
  import PresetBrowser from './PresetBrowser.svelte'
  import LayoutToggle from './LayoutToggle.svelte'

  interface Props {
    canvases: (HTMLCanvasElement | undefined)[]
    presets4: string[]
    deckBus: Array<'A' | 'B' | 'off'>
    runningCount: number
    isRunning: (slot: number) => boolean
    selectedSlot: number
    crossfader: number
    presetList: PresetMeta[]
    playlistAItems: string[]
    playlistBItems: string[]
    status: 'idle' | 'running' | 'error'
    layout: 'stage' | 'mixer'
    onStartSlot: (slot: number) => void
    onPauseSlot: (slot: number) => void
    onSelectSlot: (slot: number) => void
    onCycleBus: (slot: number) => void
    onCrossfaderChange: (v: number) => void
    onLoadPreset: (name: string) => void
    onAddToPlaylist: (deck: 'A' | 'B', name: string) => void
    onOpenOutput: () => void
    onLayoutToggle: (l: 'stage' | 'mixer') => void
    audioSection: Snippet
    videoSection: Snippet
  }

  let {
    canvases, presets4, deckBus, runningCount, isRunning, selectedSlot,
    crossfader, presetList, playlistAItems, playlistBItems, status, layout,
    onStartSlot, onPauseSlot, onSelectSlot, onCycleBus, onCrossfaderChange,
    onLoadPreset, onAddToPlaylist, onOpenOutput, onLayoutToggle,
    audioSection, videoSection,
  }: Props = $props()

  // Le deck actif détermine le "deck cible" du PresetBrowser en mode mixer.
  // On mappe le bus du slot sélectionné → 'A' | 'B' (off → 'A' par défaut).
  const activeDeckLetter = $derived<'A' | 'B'>(deckBus[selectedSlot] === 'B' ? 'B' : 'A')
</script>

<div class="mixer">
  <!-- ── Colonne principale ── -->
  <div class="mixer__main">
    <!-- Header -->
    <div class="mixer__header">
      <LayoutToggle {layout} onToggle={onLayoutToggle} />
      <div class="mixer__title">
        <span class="mixer__brand">OpenDrop</span>
        <span class="mixer__vj">VJ</span>
      </div>
      <span class="mixer__running">{runningCount}/4 running</span>
    </div>

    <!-- Grille des 4 decks -->
    <div class="mixer__decks">
      {#each [0, 1, 2, 3] as i}
        <MixerDeckCard
          slot={i}
          canvas={canvases[i]}
          presetName={presets4[i]}
          running={isRunning(i)}
          bus={deckBus[i]}
          isSelected={selectedSlot === i}
          onStart={() => onStartSlot(i)}
          onStop={() => onPauseSlot(i)}
          onSelect={() => onSelectSlot(i)}
          onCycleBus={() => onCycleBus(i)}
        />
      {/each}
    </div>

    <!-- Preset browser inline (toujours ouvert en mixer) -->
    <PresetBrowser
      presets={presetList}
      isOpen={true}
      activeDeck={activeDeckLetter}
      targetSlot={selectedSlot}
      {playlistAItems}
      {playlistBItems}
      onClose={() => {}}
      onLoadPreset={onLoadPreset}
      onAddToPlaylist={onAddToPlaylist}
      variant="grid"
    />
  </div>

  <!-- ── Colonne droite (contrôles) ── -->
  <aside class="mixer__sidebar">
    {@render audioSection()}

    <div class="controls-section">
      <span class="label">Crossfader</span>
      <MixerCrossfader value={crossfader} onchange={onCrossfaderChange} />
    </div>

    {@render videoSection()}

    <div class="controls-section">
      <button
        class="btn-output"
        onclick={onOpenOutput}
        disabled={status !== 'running'}
      >
        ⎋ Open output window
      </button>
    </div>
  </aside>
</div>

<style>
  .mixer {
    display: flex;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .mixer__main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }

  .mixer__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-2) var(--sp-3);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .mixer__title { display: flex; align-items: baseline; gap: 4px; }

  .mixer__brand {
    font-size: 14px;
    font-weight: 800;
    letter-spacing: 0.1em;
    background: linear-gradient(135deg, #ff2d78 0%, #00e5ff 100%);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .mixer__vj {
    font-size: 9px;
    color: var(--text-muted);
    font-weight: 700;
    letter-spacing: 0.2em;
  }

  .mixer__running {
    font-size: 9px;
    color: var(--live);
    font-weight: 700;
    letter-spacing: 0.06em;
  }

  .mixer__decks {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    flex-shrink: 0;
  }

  /* PresetBrowser en mode inline : override position fixed */
  .mixer__main :global(.preset-drawer) {
    position: static !important;
    flex: 1;
    transform: none !important;
    border-top: 1px solid var(--accent);
    overflow: hidden;
  }

  .mixer__main :global(.preset-drawer--open) {
    transform: none !important;
  }

  .mixer__sidebar {
    width: 240px;
    flex-shrink: 0;
    background: #0b0b20;
    border-left: 1px solid #1a1a42;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  /* Réutiliser les classes globales de +page.svelte */
  .controls-section {
    padding: var(--sp-3);
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--accent);
    font-weight: 600;
  }

  .btn-output {
    width: 100%;
    background: linear-gradient(135deg, rgba(0,229,255,0.08), rgba(180,79,255,0.08));
    color: #00e5ff;
    border: 1px solid #004455;
    border-radius: 6px;
    padding: 0.45rem;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    letter-spacing: 0.03em;
    transition: all 0.15s;
  }

  .btn-output:hover:not(:disabled) {
    background: linear-gradient(135deg, rgba(0,229,255,0.14), rgba(180,79,255,0.14));
    border-color: #00e5ff;
  }

  .btn-output:disabled { opacity: 0.3; cursor: not-allowed; }
</style>
