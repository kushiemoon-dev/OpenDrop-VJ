// Re-export from .svelte.ts for compatibility
export {
  accent,
  setAccent,
  getAccent,
  getAccentPreset,
  getAccentColors,
  cycleAccent,
  getAccentState,
  ACCENT_PRESETS
} from './accent.svelte';

export type { AccentColor, AccentPreset } from './accent.svelte';
