<!-- src/lib/components/MixerLayout.svelte -->
<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { PresetMeta } from '$lib/presets/index.js'
  import DeckCard from './DeckCard.svelte'
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
    transitionTime: number
    presetList: PresetMeta[]
    playlistAItems: string[]
    playlistBItems: string[]
    layout: 'stage' | 'mixer'
    onStartSlot: (slot: number) => void
    onPauseSlot: (slot: number) => void
    onSelectSlot: (slot: number) => void
    onCycleBus: (slot: number) => void
    onCrossfaderChange: (v: number) => void
    onTransitionChange: (v: number) => void
    onLoadPreset: (name: string) => void
    onAddToPlaylist: (deck: 'A' | 'B', name: string) => void
    onLayoutToggle: (l: 'stage' | 'mixer') => void
    audioSection: Snippet
    videoSection: Snippet
    qualiteSection: Snippet
    outputSection: Snippet
    midiSection: Snippet
    clavierSection: Snippet
    strobeSection: Snippet
    lfoSection: Snippet
    colorSection: Snippet
    compositeSection: Snippet
    snapshotSection: Snippet
    electronSection: Snippet
  }

  let {
    canvases, presets4, deckBus, runningCount, isRunning, selectedSlot,
    crossfader, transitionTime, presetList, playlistAItems, playlistBItems, layout,
    onStartSlot, onPauseSlot, onSelectSlot, onCycleBus, onCrossfaderChange, onTransitionChange,
    onLoadPreset, onAddToPlaylist, onLayoutToggle,
    audioSection, videoSection,
    qualiteSection, outputSection, midiSection, clavierSection,
    strobeSection, lfoSection, colorSection, compositeSection, snapshotSection, electronSection,
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
        <DeckCard
          letter="DECK {i + 1}"
          canvas={canvases[i]}
          presetName={presets4[i]}
          isActive={selectedSlot === i}
          isLive={isRunning(i)}
          bus={deckBus[i]}
          running={isRunning(i)}
          onSelect={() => onSelectSlot(i)}
          onCycleBus={() => onCycleBus(i)}
          onToggleRun={() => { isRunning(i) ? onPauseSlot(i) : onStartSlot(i) }}
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
      <div class="xcf">
        <div class="xcf__labels">
          <span class="xcf__side xcf__side--a" style:opacity={0.4 + (1 - crossfader) * 0.6}>A <span class="xcf__pct">{Math.round((1 - crossfader) * 100)}%</span></span>
          <span class="xcf__side xcf__side--b" style:opacity={0.4 + crossfader * 0.6}><span class="xcf__pct">{Math.round(crossfader * 100)}%</span> B</span>
        </div>
        <input class="xcf__slider" type="range" min="0" max="1" step="0.01" value={crossfader}
          oninput={(e) => onCrossfaderChange(Number(e.currentTarget.value))} />
        <div class="xcf__curve">Linear</div>
      </div>
      <div class="transition-row">
        <span class="transition-label">Fondu</span>
        <input class="transition-slider" type="range" min="0" max="5" step="0.1" value={transitionTime}
          oninput={(e) => onTransitionChange(Number(e.currentTarget.value))} title="Durée de transition preset (s)" />
        <span class="transition-value">{transitionTime.toFixed(1)}s</span>
        <button class="btn-sm" onclick={() => onTransitionChange(0)} title="Coupe nette">Hard Cut</button>
      </div>
    </div>

    {@render videoSection()}
    {@render qualiteSection()}
    {@render colorSection()}
    {@render compositeSection()}
    {@render snapshotSection()}
    {@render strobeSection()}
    {@render lfoSection()}
    {@render outputSection()}
    {@render midiSection()}
    {@render clavierSection()}
    {@render electronSection()}
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
    background: linear-gradient(135deg, var(--accent) 0%, var(--cyan) 100%);
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

  .xcf { display: flex; flex-direction: column; gap: 4px; }
  .xcf__labels { display: flex; justify-content: space-between; font-size: 10px; font-weight: 700; }
  .xcf__side--a { color: var(--accent); }
  .xcf__side--b { color: var(--live); }
  .xcf__pct { font-size: 9px; font-weight: 400; opacity: 0.7; }
  .xcf__slider { width: 100%; accent-color: var(--accent); cursor: pointer; }
  .xcf__curve { font-size: 8px; color: var(--text-muted); text-align: center; letter-spacing: 0.06em; text-transform: uppercase; }

  .transition-row { display: flex; align-items: center; gap: 0.4rem; margin-top: 0.4rem; }
  .transition-label { font-size: 10px; color: var(--text-muted); }
  .transition-slider { flex: 1; accent-color: var(--accent); cursor: pointer; }
  .transition-value { font-size: 10px; color: var(--text-muted); width: 28px; text-align: right; }

  .btn-sm {
    background: var(--bg-elevated); color: var(--text-secondary);
    border: 1px solid var(--border); border-radius: var(--r-sm);
    padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
    transition: border-color var(--t-fast), color var(--t-fast);
  }
  .btn-sm:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }
</style>
