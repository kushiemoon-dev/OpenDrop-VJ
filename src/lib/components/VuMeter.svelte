<script>
  import { onDestroy } from 'svelte';

  /** @type {{ level?: number, label?: string, vertical?: boolean }} */
  let { level = 0, label = '', vertical = false } = $props();

  // Smoothed level for animation
  let displayLevel = $state(0);
  let peakLevel = $state(0);
  /** @type {ReturnType<typeof setTimeout> | null} */
  let peakDecay = null;

  // Smooth the level changes
  $effect(() => {
    // Smooth rise, faster fall
    if (level > displayLevel) {
      displayLevel = level;
    } else {
      displayLevel = displayLevel * 0.92 + level * 0.08;
    }

    // Peak hold
    if (level > peakLevel) {
      peakLevel = level;
      // Reset peak decay timer
      if (peakDecay) clearTimeout(peakDecay);
      peakDecay = setTimeout(() => {
        peakLevel = 0;
      }, 1500);
    }
  });

  onDestroy(() => {
    if (peakDecay) clearTimeout(peakDecay);
  });

  // Calculate color based on level
  /** @param {number} lvl */
  function getColor(lvl) {
    if (lvl > 0.9) return 'var(--status-error)';
    if (lvl > 0.7) return 'var(--status-warning)';
    return 'var(--accent-green)';
  }
</script>

<div class="vu-meter" class:vertical>
  {#if label}
    <span class="label">{label}</span>
  {/if}
  <div class="track">
    <div
      class="fill"
      style="--level: {displayLevel * 100}%; --color: {getColor(displayLevel)}"
    ></div>
    <div
      class="peak"
      style="--peak: {peakLevel * 100}%"
      class:hot={peakLevel > 0.9}
    ></div>
    <div class="segments">
      {#each Array(10) as _, i}
        <div class="segment"></div>
      {/each}
    </div>
  </div>
</div>

<style>
  .vu-meter {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    width: 100%;
  }

  .vu-meter.vertical {
    flex-direction: column;
    width: auto;
    height: 100%;
  }

  .label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    min-width: 14px;
    text-align: center;
  }

  .track {
    position: relative;
    flex: 1;
    height: 8px;
    background: var(--bg-dark);
    border-radius: 4px;
    overflow: hidden;
    border: 1px solid var(--border-subtle);
  }

  .vertical .track {
    width: 8px;
    height: 100%;
    flex: 1;
  }

  .fill {
    position: absolute;
    left: 0;
    top: 0;
    height: 100%;
    width: var(--level);
    background: linear-gradient(90deg, var(--accent-green), var(--color));
    border-radius: 3px;
    transition: width 0.05s linear;
    box-shadow: 0 0 8px var(--color);
  }

  .vertical .fill {
    width: 100%;
    height: var(--level);
    top: auto;
    bottom: 0;
    background: linear-gradient(0deg, var(--accent-green), var(--color));
  }

  .peak {
    position: absolute;
    left: var(--peak);
    top: 0;
    width: 2px;
    height: 100%;
    background: var(--accent-primary);
    opacity: 0.8;
    transition: left 0.05s linear, opacity 0.3s;
  }

  .peak.hot {
    background: var(--status-error);
    box-shadow: 0 0 6px var(--status-error);
    animation: peak-pulse 0.3s ease-in-out infinite;
  }

  @keyframes peak-pulse {
    0%, 100% { opacity: 0.8; }
    50% { opacity: 1; }
  }

  .vertical .peak {
    width: 100%;
    height: 2px;
    left: 0;
    top: auto;
    bottom: var(--peak);
  }

  .segments {
    position: absolute;
    inset: 0;
    display: flex;
    gap: 1px;
    pointer-events: none;
  }

  .vertical .segments {
    flex-direction: column;
  }

  .segment {
    flex: 1;
    border-right: 1px solid rgba(0, 0, 0, 0.3);
  }

  .vertical .segment {
    border-right: none;
    border-bottom: 1px solid rgba(0, 0, 0, 0.3);
  }

  .segment:last-child {
    border: none;
  }
</style>
