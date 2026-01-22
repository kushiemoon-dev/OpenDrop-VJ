<script>
  import { isFavorite, toggleFavorite } from '$lib/stores/favorites';
  import { getPresetTags } from '$lib/stores/tags';
  import { Clock, Star, Plus } from 'lucide-svelte';

  /**
   * @type {{
   *   name?: string,
   *   path?: string,
   *   selected?: boolean,
   *   onclick?: () => void,
   *   ondblclick?: () => void,
   *   onAddToPlaylist?: (preset: {name: string, path: string}) => void
   * }}
   */
  let { name = '', path = '', selected = false, onclick, ondblclick, onAddToPlaylist } = $props();

  // Truncate long names
  let displayName = $derived(
    name.length > 35 ? name.slice(0, 32) + '...' : name
  );

  // Reactive favorite state
  let favorite = $derived(isFavorite(path));

  // Reactive tags
  let tags = $derived(getPresetTags(path));

  /** @param {MouseEvent} e */
  function handleAddToPlaylist(e) {
    e.stopPropagation();
    e.preventDefault();
    onAddToPlaylist?.({ name, path });
  }

  /** @param {MouseEvent} e */
  function handleToggleFavorite(e) {
    e.stopPropagation();
    e.preventDefault();
    toggleFavorite(path);
  }

  /** @param {KeyboardEvent} e */
  function handleKeydown(e) {
    if (e.key === 'Enter') {
      onclick?.();
    }
  }
</script>

<div
  class="preset-card"
  class:selected
  onclick={onclick}
  ondblclick={ondblclick}
  onkeydown={handleKeydown}
  role="button"
  tabindex="0"
  title={path}
>
  <div class="preview">
    <div class="gradient-bg"></div>
    <Clock size={24} strokeWidth={1.5} class="preview-icon" />
    <button
      class="favorite-btn"
      class:active={favorite}
      onclick={handleToggleFavorite}
      title={favorite ? 'Remove from favorites' : 'Add to favorites'}
      aria-label={favorite ? 'Remove from favorites' : 'Add to favorites'}
    >
      <Star size={14} fill={favorite ? 'currentColor' : 'none'} />
    </button>
    {#if onAddToPlaylist}
      <button class="add-btn" onclick={handleAddToPlaylist} title="Add to playlist" aria-label="Add to playlist">
        <Plus size={14} />
      </button>
    {/if}
  </div>
  <span class="name">{displayName}</span>
  {#if tags.length > 0}
    <div class="tags">
      {#each tags.slice(0, 2) as tag}
        <span class="tag">{tag}</span>
      {/each}
      {#if tags.length > 2}
        <span class="tag more">+{tags.length - 2}</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .preset-card {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xs);
    padding: var(--spacing-sm);
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
    outline: none;
  }

  .preset-card:focus-visible {
    border-color: var(--accent-cyan);
    box-shadow: 0 0 0 2px rgba(0, 240, 255, 0.3);
  }

  .preset-card:hover {
    background: var(--bg-elevated);
    border-color: var(--border-medium);
    transform: translateY(-2px) scale(1.02);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .preset-card:active {
    transform: translateY(0) scale(0.98);
  }

  .preset-card.selected {
    border-color: var(--accent-cyan);
    box-shadow: 0 0 15px rgba(0, 240, 255, 0.25), inset 0 0 20px rgba(0, 240, 255, 0.05);
  }

  .preview {
    width: 100%;
    aspect-ratio: 16/10;
    background: var(--bg-dark);
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    overflow: hidden;
  }

  .gradient-bg {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      135deg,
      rgba(0, 240, 255, 0.1),
      rgba(139, 92, 246, 0.1),
      rgba(255, 0, 170, 0.1)
    );
    opacity: 0.5;
  }

  .preset-card:hover .gradient-bg {
    opacity: 1;
    animation: gradient-shift 3s ease infinite;
  }

  .preview :global(.preview-icon) {
    opacity: 0.3;
  }

  .name {
    font-size: 11px;
    color: var(--text-secondary);
    text-align: center;
    line-height: 1.3;
    word-break: break-word;
  }

  .preset-card:hover .name {
    color: var(--text-primary);
  }

  .preset-card.selected .name {
    color: var(--accent-cyan);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    justify-content: center;
    max-width: 100%;
    overflow: hidden;
  }

  .tag {
    font-size: 9px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-dark);
    color: var(--text-muted);
    white-space: nowrap;
    max-width: 50px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tag.more {
    background: var(--accent-purple, #8b5cf6);
    color: white;
    opacity: 0.8;
  }

  .preset-card:hover .tag {
    background: var(--bg-elevated);
  }

  @keyframes gradient-shift {
    0% { background-position: 0% 50%; }
    50% { background-position: 100% 50%; }
    100% { background-position: 0% 50%; }
  }

  .favorite-btn {
    position: absolute;
    top: 4px;
    left: 4px;
    width: 22px;
    height: 22px;
    border-radius: var(--radius-sm);
    background: var(--bg-dark);
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transform: scale(0.8);
    transition: all 0.15s ease;
    z-index: 2;
  }

  .preset-card:hover .favorite-btn {
    opacity: 1;
    transform: scale(1);
  }

  .favorite-btn:hover {
    background: var(--bg-elevated);
    color: var(--accent-yellow, #fbbf24);
  }

  .favorite-btn.active {
    opacity: 1;
    background: var(--accent-yellow, #fbbf24);
    color: var(--bg-darkest);
  }

  .favorite-btn.active:hover {
    background: var(--accent-yellow, #fbbf24);
    filter: brightness(1.1);
  }

  .add-btn {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 22px;
    height: 22px;
    border-radius: var(--radius-sm);
    background: var(--accent-cyan);
    color: var(--bg-darkest);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transform: scale(0.8);
    transition: all 0.15s ease;
    z-index: 2;
  }

  .preset-card:hover .add-btn {
    opacity: 1;
    transform: scale(1);
  }

  .add-btn:hover {
    background: var(--status-active);
    transform: scale(1.1);
  }
</style>
