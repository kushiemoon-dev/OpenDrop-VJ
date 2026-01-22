/**
 * Accent color store for customizable UI accent
 *
 * Usage:
 *   import { accent, setAccent, getAccentColors } from '$lib/stores/accent';
 *   setAccent('magenta');
 */

const STORAGE_KEY = 'opendrop-accent';

export type AccentColor = 'cyan' | 'magenta' | 'purple' | 'green' | 'orange' | 'yellow';

export interface AccentPreset {
  name: string;
  value: AccentColor;
  color: string;
  description: string;
}

export const ACCENT_PRESETS: AccentPreset[] = [
  { name: 'Cyan', value: 'cyan', color: '#00f0ff', description: 'Default neon blue' },
  { name: 'Magenta', value: 'magenta', color: '#ff00aa', description: 'Hot pink' },
  { name: 'Purple', value: 'purple', color: '#8b5cf6', description: 'Electric violet' },
  { name: 'Green', value: 'green', color: '#00ff88', description: 'Matrix green' },
  { name: 'Orange', value: 'orange', color: '#ff6b35', description: 'Sunset orange' },
  { name: 'Yellow', value: 'yellow', color: '#fbbf24', description: 'Golden yellow' },
];

interface AccentState {
  current: AccentColor;
}

function getInitialAccent(): AccentColor {
  if (typeof localStorage === 'undefined') {
    return 'cyan';
  }

  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored && ACCENT_PRESETS.some(p => p.value === stored)) {
    return stored as AccentColor;
  }

  return 'cyan';
}

function applyAccent(accent: AccentColor): void {
  if (typeof document === 'undefined') {
    return;
  }

  document.documentElement.setAttribute('data-accent', accent);
}

function saveAccent(accent: AccentColor): void {
  if (typeof localStorage === 'undefined') {
    return;
  }
  localStorage.setItem(STORAGE_KEY, accent);
}

let accentState = $state<AccentState>({
  current: getInitialAccent()
});

// Apply initial accent
if (typeof document !== 'undefined') {
  applyAccent(accentState.current);
}

/**
 * Set the accent color
 * @param accent - Accent color preset
 */
export function setAccent(accent: AccentColor): void {
  accentState.current = accent;
  applyAccent(accent);
  saveAccent(accent);
}

/**
 * Get the current accent color
 * @returns Current accent color
 */
export function getAccent(): AccentColor {
  return accentState.current;
}

/**
 * Get the accent color preset details
 * @returns Current accent preset object
 */
export function getAccentPreset(): AccentPreset {
  return ACCENT_PRESETS.find(p => p.value === accentState.current) || ACCENT_PRESETS[0];
}

/**
 * Get all available accent colors
 * @returns Array of accent presets
 */
export function getAccentColors(): AccentPreset[] {
  return ACCENT_PRESETS;
}

/**
 * Cycle to the next accent color
 */
export function cycleAccent(): void {
  const currentIndex = ACCENT_PRESETS.findIndex(p => p.value === accentState.current);
  const nextIndex = (currentIndex + 1) % ACCENT_PRESETS.length;
  setAccent(ACCENT_PRESETS[nextIndex].value);
}

/**
 * Get the current accent state (reactive)
 */
export function getAccentState(): AccentState {
  return accentState;
}

export { accentState as accent };
