/**
 * Favorites store for preset management
 *
 * Usage:
 *   import { favorites, toggleFavorite, isFavorite } from '$lib/stores/favorites';
 *   toggleFavorite('/path/to/preset.milk');
 *   const isPresetFavorite = isFavorite('/path/to/preset.milk');
 */

const STORAGE_KEY = 'opendrop-favorites';

interface FavoritesState {
  paths: Set<string>;
}

function loadFromStorage(): Set<string> {
  if (typeof localStorage === 'undefined') {
    return new Set();
  }
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      if (Array.isArray(parsed)) {
        return new Set(parsed);
      }
    }
  } catch {
    // Invalid stored data, start fresh
  }
  return new Set();
}

function saveToStorage(paths: Set<string>): void {
  if (typeof localStorage === 'undefined') {
    return;
  }
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...paths]));
  } catch {
    // Storage full or unavailable, silently fail
  }
}

let favoritesState = $state<FavoritesState>({
  paths: loadFromStorage()
});

/**
 * Add a preset path to favorites
 * @param path - The preset path to add
 */
export function addFavorite(path: string): void {
  const newPaths = new Set(favoritesState.paths);
  newPaths.add(path);
  favoritesState.paths = newPaths;
  saveToStorage(newPaths);
}

/**
 * Remove a preset path from favorites
 * @param path - The preset path to remove
 */
export function removeFavorite(path: string): void {
  const newPaths = new Set(favoritesState.paths);
  newPaths.delete(path);
  favoritesState.paths = newPaths;
  saveToStorage(newPaths);
}

/**
 * Toggle a preset path in favorites
 * @param path - The preset path to toggle
 * @returns true if the preset is now a favorite, false otherwise
 */
export function toggleFavorite(path: string): boolean {
  const newPaths = new Set(favoritesState.paths);
  const wasAdded = !newPaths.has(path);

  if (wasAdded) {
    newPaths.add(path);
  } else {
    newPaths.delete(path);
  }

  favoritesState.paths = newPaths;
  saveToStorage(newPaths);
  return wasAdded;
}

/**
 * Check if a preset path is a favorite
 * @param path - The preset path to check
 * @returns true if the preset is a favorite
 */
export function isFavorite(path: string): boolean {
  return favoritesState.paths.has(path);
}

/**
 * Get all favorite paths
 * @returns Array of favorite preset paths
 */
export function getFavoritePaths(): string[] {
  return [...favoritesState.paths];
}

/**
 * Get the count of favorites
 * @returns Number of favorites
 */
export function getFavoriteCount(): number {
  return favoritesState.paths.size;
}

/**
 * Clear all favorites
 */
export function clearFavorites(): void {
  favoritesState.paths = new Set();
  saveToStorage(favoritesState.paths);
}

/**
 * Get the current favorites state (reactive)
 */
export function getFavorites(): FavoritesState {
  return favoritesState;
}

export { favoritesState as favorites };
