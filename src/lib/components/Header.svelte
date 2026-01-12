<script>
  import StatusIndicator from './StatusIndicator.svelte';

  /**
   * @type {{
   *   version?: string,
   *   visualizerRunning?: boolean,
   *   audioRunning?: boolean
   * }}
   */
  let { version = '', visualizerRunning = false, audioRunning = false } = $props();
</script>

<header class="header glass">
  <div class="logo">
    <h1>OpenDrop</h1>
    <span class="version">v0.1.0</span>
  </div>

  <div class="center">
    {#if version}
      <span class="projectm-version">ProjectM {version}</span>
    {/if}
  </div>

  <div class="status-group">
    <StatusIndicator active={audioRunning} size="sm" label="Audio" />
    <StatusIndicator active={visualizerRunning} size="sm" label="Visu" />
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

  .status-group {
    display: flex;
    gap: var(--spacing-lg);
  }
</style>
