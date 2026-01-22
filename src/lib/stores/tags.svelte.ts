/**
 * Tags store for preset management
 *
 * Allows users to add custom tags to presets.
 * Tags are stored in localStorage for persistence.
 *
 * Usage:
 *   import { addTag, removeTag, getPresetTags, getTaggedPresets } from '$lib/stores/tags';
 *   addTag('/presets/test.milk', 'chill');
 *   const tags = getPresetTags('/presets/test.milk'); // ['chill']
 */

const STORAGE_KEY = 'opendrop-tags';

interface TagsState {
  /** Map of preset path to array of tags */
  presetTags: Map<string, string[]>;
  /** Map of tag to array of preset paths (inverse index) */
  tagPresets: Map<string, string[]>;
  /** All unique tags */
  allTags: Set<string>;
}

function loadFromStorage(): TagsState {
  const state: TagsState = {
    presetTags: new Map(),
    tagPresets: new Map(),
    allTags: new Set()
  };

  if (typeof localStorage === 'undefined') {
    return state;
  }

  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      if (typeof parsed === 'object' && parsed !== null) {
        // Rebuild state from stored presetTags
        for (const [path, tags] of Object.entries(parsed)) {
          if (Array.isArray(tags)) {
            state.presetTags.set(path, tags as string[]);
            for (const tag of tags as string[]) {
              state.allTags.add(tag);
              const presets = state.tagPresets.get(tag) || [];
              presets.push(path);
              state.tagPresets.set(tag, presets);
            }
          }
        }
      }
    }
  } catch {
    // Invalid stored data, start fresh
  }

  return state;
}

function saveToStorage(presetTags: Map<string, string[]>): void {
  if (typeof localStorage === 'undefined') {
    return;
  }
  try {
    const data: Record<string, string[]> = {};
    for (const [path, tags] of presetTags) {
      if (tags.length > 0) {
        data[path] = tags;
      }
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
  } catch {
    // Storage full or unavailable, silently fail
  }
}

let tagsState = $state<TagsState>(loadFromStorage());

function rebuildIndices(): void {
  const newTagPresets = new Map<string, string[]>();
  const newAllTags = new Set<string>();

  for (const [path, tags] of tagsState.presetTags) {
    for (const tag of tags) {
      newAllTags.add(tag);
      const presets = newTagPresets.get(tag) || [];
      presets.push(path);
      newTagPresets.set(tag, presets);
    }
  }

  tagsState.tagPresets = newTagPresets;
  tagsState.allTags = newAllTags;
}

/**
 * Add a tag to a preset
 * @param path - Preset path
 * @param tag - Tag to add
 */
export function addTag(path: string, tag: string): void {
  const normalizedTag = tag.toLowerCase().trim();
  if (!normalizedTag) return;

  const newPresetTags = new Map(tagsState.presetTags);
  const existingTags = newPresetTags.get(path) || [];

  if (!existingTags.includes(normalizedTag)) {
    newPresetTags.set(path, [...existingTags, normalizedTag]);
    tagsState.presetTags = newPresetTags;
    rebuildIndices();
    saveToStorage(newPresetTags);
  }
}

/**
 * Remove a tag from a preset
 * @param path - Preset path
 * @param tag - Tag to remove
 */
export function removeTag(path: string, tag: string): void {
  const normalizedTag = tag.toLowerCase().trim();
  const newPresetTags = new Map(tagsState.presetTags);
  const existingTags = newPresetTags.get(path) || [];

  const filteredTags = existingTags.filter((t) => t !== normalizedTag);
  if (filteredTags.length === 0) {
    newPresetTags.delete(path);
  } else {
    newPresetTags.set(path, filteredTags);
  }

  tagsState.presetTags = newPresetTags;
  rebuildIndices();
  saveToStorage(newPresetTags);
}

/**
 * Toggle a tag on a preset
 * @param path - Preset path
 * @param tag - Tag to toggle
 * @returns true if tag was added, false if removed
 */
export function toggleTag(path: string, tag: string): boolean {
  const normalizedTag = tag.toLowerCase().trim();
  if (!normalizedTag) return false;

  const existingTags = tagsState.presetTags.get(path) || [];
  if (existingTags.includes(normalizedTag)) {
    removeTag(path, normalizedTag);
    return false;
  } else {
    addTag(path, normalizedTag);
    return true;
  }
}

/**
 * Check if a preset has a specific tag
 * @param path - Preset path
 * @param tag - Tag to check
 * @returns true if preset has the tag
 */
export function hasTag(path: string, tag: string): boolean {
  const normalizedTag = tag.toLowerCase().trim();
  const tags = tagsState.presetTags.get(path) || [];
  return tags.includes(normalizedTag);
}

/**
 * Get all tags for a preset
 * @param path - Preset path
 * @returns Array of tags
 */
export function getPresetTags(path: string): string[] {
  return tagsState.presetTags.get(path) || [];
}

/**
 * Get all presets with a specific tag
 * @param tag - Tag to search
 * @returns Array of preset paths
 */
export function getTaggedPresets(tag: string): string[] {
  const normalizedTag = tag.toLowerCase().trim();
  return tagsState.tagPresets.get(normalizedTag) || [];
}

/**
 * Get all unique tags
 * @returns Array of all tags
 */
export function getAllTags(): string[] {
  return [...tagsState.allTags].sort();
}

/**
 * Get tag count
 * @returns Number of unique tags
 */
export function getTagCount(): number {
  return tagsState.allTags.size;
}

/**
 * Get preset count for a tag
 * @param tag - Tag to count
 * @returns Number of presets with this tag
 */
export function getTagPresetCount(tag: string): number {
  const normalizedTag = tag.toLowerCase().trim();
  return (tagsState.tagPresets.get(normalizedTag) || []).length;
}

/**
 * Clear all tags for a preset
 * @param path - Preset path
 */
export function clearPresetTags(path: string): void {
  const newPresetTags = new Map(tagsState.presetTags);
  newPresetTags.delete(path);
  tagsState.presetTags = newPresetTags;
  rebuildIndices();
  saveToStorage(newPresetTags);
}

/**
 * Clear all tags
 */
export function clearAllTags(): void {
  tagsState.presetTags = new Map();
  tagsState.tagPresets = new Map();
  tagsState.allTags = new Set();
  saveToStorage(tagsState.presetTags);
}

/**
 * Get the current tags state (reactive)
 */
export function getTags(): TagsState {
  return tagsState;
}

export { tagsState as tags };
