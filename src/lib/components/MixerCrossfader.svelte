<!-- src/lib/components/MixerCrossfader.svelte -->
<script lang="ts">
  interface Props {
    value: number
    onchange: (v: number) => void
  }
  let { value, onchange }: Props = $props()

  const gainA = $derived(Math.round((1 - value) * 100))
  const gainB = $derived(Math.round(value * 100))
</script>

<div class="xcf">
  <div class="xcf__labels">
    <span class="xcf__side xcf__side--a" style:opacity={0.4 + (1 - value) * 0.6}>
      A <span class="xcf__pct">{gainA}%</span>
    </span>
    <span class="xcf__side xcf__side--b" style:opacity={0.4 + value * 0.6}>
      <span class="xcf__pct">{gainB}%</span> B
    </span>
  </div>
  <input
    class="xcf__slider"
    type="range"
    min="0"
    max="1"
    step="0.01"
    {value}
    oninput={(e) => onchange(Number((e.currentTarget as HTMLInputElement).value))}
  />
  <div class="xcf__curve">Linear</div>
</div>

<style>
  .xcf { display: flex; flex-direction: column; gap: 4px; }

  .xcf__labels {
    display: flex;
    justify-content: space-between;
    font-size: 10px;
    font-weight: 700;
  }

  .xcf__side--a { color: var(--accent); }
  .xcf__side--b { color: var(--live); }

  .xcf__pct { font-size: 9px; font-weight: 400; opacity: 0.7; }

  .xcf__slider {
    width: 100%;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .xcf__curve {
    font-size: 8px;
    color: var(--text-muted);
    text-align: center;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
</style>
