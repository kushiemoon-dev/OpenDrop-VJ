/**
 * Theme store for dark/light mode toggle
 *
 * Usage:
 *   import { theme, toggleTheme, setTheme } from '$lib/stores/theme';
 *   toggleTheme();
 *   setTheme('light');
 */

const STORAGE_KEY = 'opendrop-theme';

type Theme = 'dark' | 'light';

interface ThemeState {
  current: Theme;
}

function getInitialTheme(): Theme {
  if (typeof localStorage === 'undefined') {
    return 'dark';
  }

  // Check localStorage first
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === 'light' || stored === 'dark') {
    return stored;
  }

  // Check system preference
  if (typeof window !== 'undefined' && window.matchMedia) {
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    return prefersDark ? 'dark' : 'light';
  }

  return 'dark';
}

function applyTheme(theme: Theme): void {
  if (typeof document === 'undefined') {
    return;
  }

  document.documentElement.setAttribute('data-theme', theme);

  // Also set class for easier CSS targeting
  document.documentElement.classList.remove('theme-dark', 'theme-light');
  document.documentElement.classList.add(`theme-${theme}`);
}

function saveTheme(theme: Theme): void {
  if (typeof localStorage === 'undefined') {
    return;
  }
  localStorage.setItem(STORAGE_KEY, theme);
}

let themeState = $state<ThemeState>({
  current: getInitialTheme()
});

// Apply initial theme
if (typeof document !== 'undefined') {
  applyTheme(themeState.current);
}

/**
 * Toggle between dark and light theme
 */
export function toggleTheme(): void {
  const newTheme: Theme = themeState.current === 'dark' ? 'light' : 'dark';
  themeState.current = newTheme;
  applyTheme(newTheme);
  saveTheme(newTheme);
}

/**
 * Set a specific theme
 * @param theme - 'dark' or 'light'
 */
export function setTheme(theme: Theme): void {
  themeState.current = theme;
  applyTheme(theme);
  saveTheme(theme);
}

/**
 * Get the current theme
 * @returns Current theme ('dark' or 'light')
 */
export function getTheme(): Theme {
  return themeState.current;
}

/**
 * Check if current theme is dark
 * @returns true if dark theme is active
 */
export function isDark(): boolean {
  return themeState.current === 'dark';
}

/**
 * Get the current theme state (reactive)
 */
export function getThemeState(): ThemeState {
  return themeState;
}

export { themeState as theme };
