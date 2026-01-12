<script>
  /** @type {{ active?: boolean, size?: 'sm' | 'md' | 'lg', label?: string }} */
  let { active = false, size = 'md', label = '' } = $props();
</script>

<div class="status" class:active class:sm={size === 'sm'} class:lg={size === 'lg'}>
  <span class="dot"></span>
  {#if label}
    <span class="label">{label}</span>
  {/if}
</div>

<style>
  .status {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-sm);
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--status-inactive);
    transition: var(--transition-normal);
  }

  .sm .dot {
    width: 8px;
    height: 8px;
  }

  .lg .dot {
    width: 12px;
    height: 12px;
  }

  .active .dot {
    background: var(--status-active);
    box-shadow: 0 0 8px var(--status-active), 0 0 16px var(--status-active);
    animation: pulse-glow 2s ease-in-out infinite;
  }

  .label {
    font-size: 12px;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .active .label {
    color: var(--status-active);
  }

  @keyframes pulse-glow {
    0%, 100% {
      box-shadow: 0 0 8px var(--status-active), 0 0 16px var(--status-active);
    }
    50% {
      box-shadow: 0 0 12px var(--status-active), 0 0 24px var(--status-active);
    }
  }
</style>
