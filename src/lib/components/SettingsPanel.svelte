<script>
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { X, FolderPlus, Trash2, RefreshCw, FolderOpen, Sun, Moon } from 'lucide-svelte';
  import { settings, addPresetPath, removePresetPath } from '$lib/stores/settings.svelte';
  import { theme, toggleTheme } from '$lib/stores/theme';
  import { accent, setAccent, ACCENT_PRESETS } from '$lib/stores/accent';

  /**
   * @type {{
   *   onClose: () => void,
   *   onPresetsRefresh?: () => void
   * }}
   */
  let { onClose, onPresetsRefresh } = $props();

  /** @type {string[]} */
  let detectedPaths = $state([]);
  let loadingPaths = $state(false);
  let newPathInput = $state('');

  async function loadDetectedPaths() {
    loadingPaths = true;
    try {
      detectedPaths = await invoke('get_preset_directories');
    } catch (e) {
      console.error('Failed to get preset directories:', e);
      detectedPaths = [];
    }
    loadingPaths = false;
  }

  async function browseForFolder() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Preset Folder'
      });
      if (selected && typeof selected === 'string') {
        addPresetPath(selected);
        onPresetsRefresh?.();
      }
    } catch (e) {
      console.error('Failed to open folder dialog:', e);
    }
  }

  function handleAddPath() {
    if (newPathInput.trim()) {
      addPresetPath(newPathInput.trim());
      newPathInput = '';
      onPresetsRefresh?.();
    }
  }

  /** @param {string} path */
  function handleRemovePath(path) {
    removePresetPath(path);
    onPresetsRefresh?.();
  }

  // Load detected paths on mount
  $effect(() => {
    loadDetectedPaths();
  });

  // Reactive theme state
  let isDark = $derived(theme.current === 'dark');
</script>

<div class="settings-overlay" onclick={onClose} role="presentation">
  <div class="settings-panel glass" onclick={(e) => e.stopPropagation()} role="dialog" aria-label="Settings">
    <header class="panel-header">
      <h2>Settings</h2>
      <button class="close-btn" onclick={onClose} aria-label="Close settings">
        <X size={20} />
      </button>
    </header>

    <div class="settings-content">
      <!-- Appearance Section -->
      <section class="settings-section">
        <h3>Appearance</h3>
        <p class="section-desc">Customize the look and feel of OpenDrop</p>

        <!-- Theme Toggle -->
        <div class="subsection">
          <div class="subsection-header">
            <span>Theme</span>
          </div>
          <div class="theme-toggle-row">
            <button
              class="theme-option"
              class:active={!isDark}
              onclick={() => !isDark || toggleTheme()}
            >
              <Sun size={16} />
              <span>Light</span>
            </button>
            <button
              class="theme-option"
              class:active={isDark}
              onclick={() => isDark || toggleTheme()}
            >
              <Moon size={16} />
              <span>Dark</span>
            </button>
          </div>
        </div>

        <!-- Accent Color -->
        <div class="subsection">
          <div class="subsection-header">
            <span>Accent Color</span>
          </div>
          <div class="accent-grid">
            {#each ACCENT_PRESETS as preset (preset.value)}
              <button
                class="accent-option"
                class:selected={accent.current === preset.value}
                style="--swatch-color: {preset.color}"
                onclick={() => setAccent(preset.value)}
                title={preset.description}
              >
                <span class="accent-swatch"></span>
                <span class="accent-name">{preset.name}</span>
              </button>
            {/each}
          </div>
        </div>
      </section>

      <!-- Preset Paths Section -->
      <section class="settings-section">
        <h3>Preset Directories</h3>
        <p class="section-desc">Configure where OpenDrop looks for .milk preset files</p>

        <!-- Detected Paths -->
        <div class="subsection">
          <div class="subsection-header">
            <span>Auto-detected Paths</span>
            <button class="icon-btn" onclick={loadDetectedPaths} disabled={loadingPaths} title="Refresh">
              <RefreshCw size={14} class={loadingPaths ? 'spinning' : ''} />
            </button>
          </div>
          <div class="path-list">
            {#if detectedPaths.length === 0}
              <div class="empty-state">No preset directories found</div>
            {:else}
              {#each detectedPaths as path}
                <div class="path-item detected">
                  <FolderOpen size={14} />
                  <span class="path-text" title={path}>{path}</span>
                </div>
              {/each}
            {/if}
          </div>
        </div>

        <!-- Custom Paths -->
        <div class="subsection">
          <div class="subsection-header">
            <span>Custom Paths</span>
            <button class="icon-btn primary" onclick={browseForFolder} title="Browse for folder">
              <FolderPlus size={14} />
            </button>
          </div>
          <div class="path-list">
            {#if settings.customPresetPaths.length === 0}
              <div class="empty-state">No custom paths added</div>
            {:else}
              {#each settings.customPresetPaths as path}
                <div class="path-item custom">
                  <FolderOpen size={14} />
                  <span class="path-text" title={path}>{path}</span>
                  <button class="remove-btn" onclick={() => handleRemovePath(path)} title="Remove">
                    <Trash2 size={12} />
                  </button>
                </div>
              {/each}
            {/if}
          </div>

          <!-- Manual path input -->
          <div class="add-path-row">
            <input
              type="text"
              placeholder="Enter path manually..."
              bind:value={newPathInput}
              onkeydown={(e) => e.key === 'Enter' && handleAddPath()}
            />
            <button class="add-btn" onclick={handleAddPath} disabled={!newPathInput.trim()}>
              Add
            </button>
          </div>
        </div>
      </section>

      <!-- Info Section -->
      <section class="settings-section info">
        <h3>Preset Format</h3>
        <p class="section-desc">
          OpenDrop supports MilkDrop presets (.milk) and projectM presets (.prjm).
          Place your presets in any of the directories above, and they will appear in the preset browser.
        </p>
      </section>
    </div>
  </div>
</div>

<style>
  .settings-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 500;
    backdrop-filter: blur(4px);
  }

  .settings-panel {
    width: 90%;
    max-width: 600px;
    max-height: 80vh;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-md) var(--spacing-lg);
    border-bottom: 1px solid var(--border-subtle);
  }

  .panel-header h2 {
    font-size: 1.1em;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: var(--radius-md);
    transition: var(--transition-fast);
  }

  .close-btn:hover {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .settings-content {
    padding: var(--spacing-lg);
    overflow-y: auto;
    flex: 1;
  }

  .settings-section {
    margin-bottom: var(--spacing-xl);
  }

  .settings-section:last-child {
    margin-bottom: 0;
  }

  .settings-section h3 {
    font-size: 0.95em;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 var(--spacing-xs) 0;
  }

  .section-desc {
    font-size: 0.85em;
    color: var(--text-muted);
    margin: 0 0 var(--spacing-md) 0;
  }

  .subsection {
    margin-bottom: var(--spacing-lg);
  }

  .subsection-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 0.8em;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    margin-bottom: var(--spacing-sm);
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    background: var(--bg-dark);
    color: var(--text-secondary);
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: var(--transition-fast);
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .icon-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .icon-btn.primary:hover:not(:disabled) {
    color: var(--accent-primary);
  }

  .path-list {
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .path-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-sm) var(--spacing-md);
    font-size: 0.85em;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
  }

  .path-item:last-child {
    border-bottom: none;
  }

  .path-item.detected {
    color: var(--text-muted);
  }

  .path-text {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 0.9em;
  }

  .remove-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: var(--transition-fast);
    opacity: 0;
  }

  .path-item:hover .remove-btn {
    opacity: 1;
  }

  .remove-btn:hover {
    color: var(--accent-red);
    background: rgba(255, 107, 107, 0.1);
  }

  .empty-state {
    padding: var(--spacing-md);
    text-align: center;
    font-size: 0.85em;
    color: var(--text-muted);
    font-style: italic;
  }

  .add-path-row {
    display: flex;
    gap: var(--spacing-sm);
    margin-top: var(--spacing-sm);
  }

  .add-path-row input {
    flex: 1;
    padding: var(--spacing-sm) var(--spacing-md);
    font-size: 0.85em;
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-mono);
  }

  .add-path-row input::placeholder {
    color: var(--text-muted);
  }

  .add-path-row input:focus {
    outline: none;
    border-color: var(--accent-primary);
  }

  .add-btn {
    padding: var(--spacing-sm) var(--spacing-md);
    font-size: 0.85em;
    font-weight: 500;
    background: var(--accent-primary);
    border: none;
    border-radius: var(--radius-md);
    color: var(--bg-dark);
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .add-btn:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .add-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .info {
    background: var(--bg-dark);
    padding: var(--spacing-md);
    border-radius: var(--radius-md);
    border: 1px solid var(--border-subtle);
  }

  .info h3 {
    font-size: 0.85em;
  }

  .info .section-desc {
    margin-bottom: 0;
    line-height: 1.5;
  }

  :global(.spinning) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  /* Theme toggle styles */
  .theme-toggle-row {
    display: flex;
    gap: var(--spacing-sm);
  }

  .theme-option {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-md);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    cursor: pointer;
    transition: var(--transition-fast);
    font-size: 0.85em;
  }

  .theme-option:hover {
    background: var(--bg-elevated);
    border-color: var(--border-medium);
  }

  .theme-option.active {
    background: var(--bg-elevated);
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  /* Accent color styles */
  .accent-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--spacing-sm);
  }

  .accent-option {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--spacing-xs);
    padding: var(--spacing-sm);
    background: var(--bg-dark);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .accent-option:hover {
    background: var(--bg-elevated);
    border-color: var(--border-medium);
  }

  .accent-option.selected {
    background: var(--bg-elevated);
    border-color: var(--swatch-color);
  }

  .accent-swatch {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--swatch-color);
    box-shadow: 0 0 8px var(--swatch-color);
  }

  .accent-name {
    font-size: 0.7em;
    color: var(--text-muted);
    text-transform: capitalize;
  }

  .accent-option.selected .accent-name {
    color: var(--text-primary);
  }
</style>
