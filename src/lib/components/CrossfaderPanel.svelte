<script>
  import { invoke } from "@tauri-apps/api/core";
  import { showToast } from "$lib/stores/toast";

  /**
   * @typedef {{
   *   position: number,
   *   side_a: number[],
   *   side_b: number[],
   *   curve: string,
   *   enabled: boolean
   * }} CrossfaderConfig
   */

  /**
   * @type {{
   *   crossfader?: CrossfaderConfig,
   *   onUpdate?: () => void
   * }}
   */
  let {
    crossfader = { position: 0.5, side_a: [0, 1], side_b: [2, 3], curve: 'equal_power', enabled: false },
    onUpdate
  } = $props();

  let position = $state(0.5);
  let isDragging = $state(false);

  // Sync position from props
  $effect(() => {
    if (!isDragging) {
      position = crossfader.position;
    }
  });

  /** @param {Event} e */
  async function handlePositionChange(e) {
    const target = /** @type {HTMLInputElement} */ (e.target);
    const newPos = parseFloat(target.value);
    position = newPos;
    try {
      await invoke("crossfader_set_position", { position: newPos });
      onUpdate?.();
    } catch (err) {
      showToast("Failed to set crossfader position", "error");
    }
  }

  async function toggleEnabled() {
    try {
      await invoke("crossfader_set_enabled", { enabled: !crossfader.enabled });
      onUpdate?.();
    } catch (err) {
      showToast("Failed to toggle crossfader", "error");
    }
  }

  /** @param {string} curve */
  async function setCurve(curve) {
    try {
      await invoke("crossfader_set_curve", { curve });
      onUpdate?.();
    } catch (err) {
      showToast("Failed to set curve", "error");
    }
  }

  // Calculate volume display for each side
  let sideAVolume = $derived(
    crossfader.enabled
      ? (crossfader.curve === 'equal_power'
          ? Math.sqrt(1 - position)
          : 1 - position)
      : 1
  );

  let sideBVolume = $derived(
    crossfader.enabled
      ? (crossfader.curve === 'equal_power'
          ? Math.sqrt(position)
          : position)
      : 1
  );
</script>

<div class="crossfader-panel">
  <div class="panel-header">
    <h3>Crossfader</h3>
    <button
      class="toggle-btn"
      class:active={crossfader.enabled}
      onclick={toggleEnabled}
      title={crossfader.enabled ? 'Disable crossfader' : 'Enable crossfader'}
    >
      {crossfader.enabled ? 'ON' : 'OFF'}
    </button>
  </div>

  <div class="fader-section" class:disabled={!crossfader.enabled}>
    <div class="side-labels">
      <div class="side-label">
        <span class="side-name">A</span>
        <span class="side-decks">
          {crossfader.side_a.map(d => d + 1).join(', ') || '-'}
        </span>
        <span class="side-vol">{Math.round(sideAVolume * 100)}%</span>
      </div>
      <div class="side-label">
        <span class="side-name">B</span>
        <span class="side-decks">
          {crossfader.side_b.map(d => d + 1).join(', ') || '-'}
        </span>
        <span class="side-vol">{Math.round(sideBVolume * 100)}%</span>
      </div>
    </div>

    <div class="fader-track">
      <input
        type="range"
        min="0"
        max="1"
        step="0.01"
        value={position}
        oninput={handlePositionChange}
        onmousedown={() => isDragging = true}
        onmouseup={() => isDragging = false}
        class="fader-slider"
        disabled={!crossfader.enabled}
      />
      <div class="fader-markers">
        <span class="marker">A</span>
        <span class="marker center">|</span>
        <span class="marker">B</span>
      </div>
    </div>

    <div class="curve-selector">
      <span class="label">Curve:</span>
      <button
        class="curve-btn"
        class:active={crossfader.curve === 'linear'}
        onclick={() => setCurve('linear')}
        disabled={!crossfader.enabled}
      >
        Linear
      </button>
      <button
        class="curve-btn"
        class:active={crossfader.curve === 'equal_power'}
        onclick={() => setCurve('equal_power')}
        disabled={!crossfader.enabled}
      >
        Equal Power
      </button>
    </div>
  </div>
</div>

<style>
  .crossfader-panel {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    padding: var(--spacing-lg);
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-md);
  }

  .panel-header h3 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent-cyan);
    margin: 0;
  }

  .toggle-btn {
    padding: 4px 12px;
    font-size: 10px;
    font-weight: 600;
    border-radius: var(--radius-sm);
    background: var(--bg-dark);
    color: var(--text-muted);
    border: 1px solid var(--border-subtle);
    transition: all 0.15s;
  }

  .toggle-btn.active {
    background: var(--accent-cyan);
    color: var(--bg-darkest);
    border-color: var(--accent-cyan);
  }

  .fader-section {
    transition: opacity 0.2s;
  }

  .fader-section.disabled {
    opacity: 0.5;
  }

  .side-labels {
    display: flex;
    justify-content: space-between;
    margin-bottom: var(--spacing-sm);
  }

  .side-label {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }

  .side-name {
    font-size: 14px;
    font-weight: 700;
    color: var(--accent-magenta);
  }

  .side-label:last-child .side-name {
    color: var(--accent-cyan);
  }

  .side-decks {
    font-size: 10px;
    color: var(--text-muted);
  }

  .side-vol {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    font-family: var(--font-mono);
  }

  .fader-track {
    margin-bottom: var(--spacing-md);
  }

  .fader-slider {
    width: 100%;
    height: 8px;
    -webkit-appearance: none;
    appearance: none;
    background: linear-gradient(90deg,
      var(--accent-magenta) 0%,
      var(--bg-dark) 50%,
      var(--accent-cyan) 100%
    );
    border-radius: 4px;
    outline: none;
  }

  .fader-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 20px;
    height: 24px;
    background: linear-gradient(180deg, #444, #222);
    border: 2px solid var(--text-secondary);
    border-radius: 4px;
    cursor: grab;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5);
    transition: transform 0.1s;
  }

  .fader-slider::-webkit-slider-thumb:hover {
    transform: scaleY(1.1);
    border-color: var(--accent-cyan);
  }

  .fader-slider::-webkit-slider-thumb:active {
    cursor: grabbing;
    border-color: var(--status-active);
  }

  .fader-slider:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .fader-slider:disabled::-webkit-slider-thumb {
    cursor: not-allowed;
  }

  .fader-markers {
    display: flex;
    justify-content: space-between;
    padding: 4px 8px 0;
  }

  .marker {
    font-size: 9px;
    color: var(--text-muted);
    font-weight: 600;
  }

  .marker.center {
    color: var(--text-secondary);
  }

  .curve-selector {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
  }

  .curve-selector .label {
    font-size: 11px;
    color: var(--text-muted);
  }

  .curve-btn {
    padding: 4px 8px;
    font-size: 10px;
    border-radius: var(--radius-sm);
    background: var(--bg-dark);
    color: var(--text-muted);
    border: 1px solid var(--border-subtle);
    transition: all 0.15s;
  }

  .curve-btn.active {
    background: var(--bg-elevated);
    color: var(--text-primary);
    border-color: var(--accent-purple);
  }

  .curve-btn:hover:not(:disabled) {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .curve-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
