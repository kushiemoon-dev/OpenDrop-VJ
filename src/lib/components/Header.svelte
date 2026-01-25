<script>
  import StatusIndicator from './StatusIndicator.svelte';
  import { Settings } from 'lucide-svelte';

  /**
   * @type {{
   *   version?: string,
   *   visualizerRunning?: boolean,
   *   audioRunning?: boolean,
   *   onSettingsClick?: () => void
   * }}
   */
  let { version = '', visualizerRunning = false, audioRunning = false, onSettingsClick } = $props();
</script>

<header class="header glass">
  <div class="logo">
    <h1>OpenDrop</h1>
    <span class="version">v0.3.3</span>
  </div>

  <div class="center">
    {#if version}
      <span class="projectm-version">ProjectM {version}</span>
    {/if}
  </div>

  <div class="right-group">
    <div class="actions">
      <button
        class="settings-toggle btn-press"
        onclick={onSettingsClick}
        title="Settings"
        aria-label="Open settings"
      >
        <Settings size={18} />
      </button>
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
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-magenta));
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

  .settings-toggle {
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

  .settings-toggle:hover {
    background: var(--bg-elevated);
    border-color: var(--border-medium);
    color: var(--accent-primary);
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
</style>
