<script>
  import StatusIndicator from './StatusIndicator.svelte';
  import { Sun, Moon, Palette } from 'lucide-svelte';
  import { theme, toggleTheme } from '$lib/stores/theme';
  import { accent, setAccent, ACCENT_PRESETS } from '$lib/stores/accent';

  /**
   * @type {{
   *   version?: string,
   *   visualizerRunning?: boolean,
   *   audioRunning?: boolean
   * }}
   */
  let { version = '', visualizerRunning = false, audioRunning = false } = $props();

  // Reactive theme state
  let isDark = $derived(theme.current === 'dark');

  // Accent picker state
  let accentPickerOpen = $state(false);

  /**
   * @param {import('$lib/stores/accent').AccentColor} color
   */
  function selectAccent(color) {
    setAccent(color);
    accentPickerOpen = false;
  }

  // Close picker when clicking outside
  /** @param {MouseEvent} event */
  function handleClickOutside(event) {
    const target = /** @type {HTMLElement} */ (event.target);
    if (!target.closest('.accent-picker-wrapper')) {
      accentPickerOpen = false;
    }
  }
</script>

<svelte:window on:click={handleClickOutside} />

<header class="header glass">
  <div class="logo">
    <h1>OpenDrop</h1>
    <span class="version">v0.2.1</span>
  </div>

  <div class="center">
    {#if version}
      <span class="projectm-version">ProjectM {version}</span>
    {/if}
  </div>

  <div class="right-group">
    <div class="actions">
      <button
        class="theme-toggle btn-press"
        onclick={toggleTheme}
        title={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
        aria-label={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
      >
        {#if isDark}
          <Sun size={18} />
        {:else}
          <Moon size={18} />
        {/if}
      </button>

      <div class="accent-picker-wrapper">
        <button
          class="accent-toggle btn-press"
          onclick={(e) => { e.stopPropagation(); accentPickerOpen = !accentPickerOpen; }}
          title="Change accent color"
          aria-label="Change accent color"
          aria-expanded={accentPickerOpen}
        >
          <Palette size={18} />
          <span class="accent-dot" style="background: {ACCENT_PRESETS.find(p => p.value === accent.current)?.color}"></span>
        </button>

        {#if accentPickerOpen}
          <div class="accent-picker animate-scale-in" role="menu">
            <div class="accent-picker-title">Accent Color</div>
            <div class="accent-swatches">
              {#each ACCENT_PRESETS as preset (preset.value)}
                <button
                  class="accent-swatch"
                  class:selected={accent.current === preset.value}
                  style="--swatch-color: {preset.color}"
                  onclick={() => selectAccent(preset.value)}
                  title={preset.description}
                  role="menuitem"
                >
                  <span class="swatch-color"></span>
                  <span class="swatch-name">{preset.name}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>

    <div class="status-group">
      <StatusIndicator active={audioRunning} size="sm" label="Audio" />
      <StatusIndicator active={visualizerRunning} size="sm" label="Visu" />
    </div>
  </div>
</header>

<style>
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-md) var(--spacing-xl);
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border-subtle);
    height: 52px;
    flex-shrink: 0;
  }

  .logo {
    display: flex;
    align-items: baseline;
    gap: var(--spacing-sm);
  }

  .logo h1 {
    font-size: 1.4em;
    font-weight: 700;
    background: linear-gradient(135deg, var(--accent-cyan), var(--accent-magenta));
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
    letter-spacing: -0.5px;
  }

  .version {
    font-size: 11px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .center {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
  }

  .projectm-version {
    font-size: 11px;
    color: var(--text-muted);
    font-family: var(--font-mono);
    padding: 4px 10px;
    background: var(--bg-dark);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
  }

  .theme-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .theme-toggle:hover {
    background: var(--bg-elevated);
    border-color: var(--border-medium);
    color: var(--accent-yellow);
  }

  .right-group {
    display: flex;
    align-items: center;
    gap: var(--spacing-lg);
  }

  .status-group {
    display: flex;
    gap: var(--spacing-lg);
  }

  /* Accent picker */
  .accent-picker-wrapper {
    position: relative;
  }

  .accent-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    padding: 0 var(--spacing-sm);
    border-radius: var(--radius-md);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .accent-toggle:hover {
    background: var(--bg-elevated);
    border-color: var(--border-medium);
    color: var(--accent-primary, var(--accent-cyan));
  }

  .accent-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    box-shadow: 0 0 6px currentColor;
  }

  .accent-picker {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    width: 200px;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    padding: var(--spacing-md);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    z-index: 100;
  }

  .accent-picker-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    margin-bottom: var(--spacing-sm);
  }

  .accent-swatches {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: var(--spacing-xs);
  }

  .accent-swatch {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-sm) var(--spacing-md);
    border-radius: var(--radius-md);
    background: transparent;
    border: 1px solid transparent;
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .accent-swatch:hover {
    background: var(--bg-elevated);
  }

  .accent-swatch.selected {
    background: var(--bg-elevated);
    border-color: var(--swatch-color);
  }

  .swatch-color {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--swatch-color);
    box-shadow: 0 0 8px var(--swatch-color);
  }

  .swatch-name {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .accent-swatch.selected .swatch-name {
    color: var(--text-primary);
  }
</style>
