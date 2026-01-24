/**
 * Settings store for application configuration
 *
 * Usage:
 *   import { settings, updateSettings, resetSettings, getPresetPaths, addPresetPath, removePresetPath } from '$lib/stores/settings';
 */

const STORAGE_KEY = 'opendrop-settings';

interface AppSettings {
  /** Custom preset directories to search (in addition to defaults) */
  customPresetPaths: string[];
  /** Custom texture directories to search (in addition to defaults) */
  customTexturePaths: string[];
  /** Whether to use only custom paths (ignore defaults) */
  useOnlyCustomPaths: boolean;
  /** Default window size for new decks */
  defaultDeckWidth: number;
  defaultDeckHeight: number;
  /** Auto-start audio on app launch */
  autoStartAudio: boolean;
  /** Preferred audio device name */
  preferredAudioDevice: string | null;
}

const DEFAULT_SETTINGS: AppSettings = {
  customPresetPaths: [],
  customTexturePaths: [],
  useOnlyCustomPaths: false,
  defaultDeckWidth: 1280,
  defaultDeckHeight: 720,
  autoStartAudio: false,
  preferredAudioDevice: null,
};

function loadSettings(): AppSettings {
  if (typeof localStorage === 'undefined') {
    return { ...DEFAULT_SETTINGS };
  }

  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      // Merge with defaults to ensure all fields exist
      return { ...DEFAULT_SETTINGS, ...parsed };
    }
  } catch (e) {
    console.warn('Failed to load settings:', e);
  }

  return { ...DEFAULT_SETTINGS };
}

function saveSettings(settings: AppSettings): void {
  if (typeof localStorage === 'undefined') {
    return;
  }
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch (e) {
    console.warn('Failed to save settings:', e);
  }
}

let settingsState = $state<AppSettings>(loadSettings());

/**
 * Reactive settings object - use settings.current to access values
 * Cannot export $state directly if reassigned, so we wrap in an object
 */
export const settings = {
  get current() {
    return settingsState;
  },
  get customPresetPaths() {
    return settingsState.customPresetPaths;
  },
  get customTexturePaths() {
    return settingsState.customTexturePaths;
  },
  get useOnlyCustomPaths() {
    return settingsState.useOnlyCustomPaths;
  },
  get defaultDeckWidth() {
    return settingsState.defaultDeckWidth;
  },
  get defaultDeckHeight() {
    return settingsState.defaultDeckHeight;
  },
  get autoStartAudio() {
    return settingsState.autoStartAudio;
  },
  get preferredAudioDevice() {
    return settingsState.preferredAudioDevice;
  },
};

/**
 * Update one or more settings
 * @param updates - Partial settings object with values to update
 */
export function updateSettings(updates: Partial<AppSettings>): void {
  settingsState = { ...settingsState, ...updates };
  saveSettings(settingsState);
}

/**
 * Reset all settings to defaults
 */
export function resetSettings(): void {
  settingsState = { ...DEFAULT_SETTINGS };
  saveSettings(settingsState);
}

/**
 * Get the list of custom preset paths
 * @returns Array of custom preset directory paths
 */
export function getCustomPresetPaths(): string[] {
  return settingsState.customPresetPaths;
}

/**
 * Add a custom preset path
 * @param path - Directory path to add
 */
export function addPresetPath(path: string): void {
  if (!settingsState.customPresetPaths.includes(path)) {
    settingsState = {
      ...settingsState,
      customPresetPaths: [...settingsState.customPresetPaths, path],
    };
    saveSettings(settingsState);
  }
}

/**
 * Remove a custom preset path
 * @param path - Directory path to remove
 */
export function removePresetPath(path: string): void {
  settingsState = {
    ...settingsState,
    customPresetPaths: settingsState.customPresetPaths.filter((p) => p !== path),
  };
  saveSettings(settingsState);
}

/**
 * Get the list of custom texture paths
 * @returns Array of custom texture directory paths
 */
export function getCustomTexturePaths(): string[] {
  return settingsState.customTexturePaths;
}

/**
 * Add a custom texture path
 * @param path - Directory path to add
 */
export function addTexturePath(path: string): void {
  if (!settingsState.customTexturePaths.includes(path)) {
    settingsState = {
      ...settingsState,
      customTexturePaths: [...settingsState.customTexturePaths, path],
    };
    saveSettings(settingsState);
  }
}

/**
 * Remove a custom texture path
 * @param path - Directory path to remove
 */
export function removeTexturePath(path: string): void {
  settingsState = {
    ...settingsState,
    customTexturePaths: settingsState.customTexturePaths.filter((p) => p !== path),
  };
  saveSettings(settingsState);
}

export type { AppSettings };
