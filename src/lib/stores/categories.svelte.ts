/**
 * Categories store for preset management
 *
 * Automatically extracts categories from preset paths.
 * Path structure: /base/path/<category>/[<subcategory>/]<preset.milk>
 *
 * Usage:
 *   import { extractCategory, getAllCategories, getCategoryCount } from '$lib/stores/categories';
 *   const category = extractCategory('/presets/milkdrop/Acid Trip.milk');
 *   // Returns: 'milkdrop'
 */

interface CategoriesState {
  /** Map of category name to preset count */
  categories: Map<string, number>;
  /** Base path to strip from preset paths (detected automatically) */
  basePath: string;
}

let categoriesState = $state<CategoriesState>({
  categories: new Map(),
  basePath: ''
});

/**
 * Extract category from a preset path
 * @param path - Full preset path
 * @returns Category name or 'Uncategorized'
 */
export function extractCategory(path: string): string {
  if (!path) return 'Uncategorized';

  // Normalize path separators
  const normalizedPath = path.replace(/\\/g, '/');

  // Split path into segments
  const segments = normalizedPath.split('/').filter(Boolean);

  // We expect at least: basePath/category/preset.milk
  // Find the preset file (ends with .milk or .prjm)
  const presetIndex = segments.findIndex(
    (s) => s.endsWith('.milk') || s.endsWith('.prjm')
  );

  if (presetIndex < 0) return 'Uncategorized';

  // Category is typically 1-2 levels above the preset file
  // Example: /home/user/presets/milkdrop/classic/preset.milk
  // Category: milkdrop, Subcategory: classic

  // If we have enough segments, extract category
  if (presetIndex >= 1) {
    // Get parent directory as category
    return segments[presetIndex - 1];
  }

  return 'Uncategorized';
}

/**
 * Extract both category and subcategory from a preset path
 * @param path - Full preset path
 * @returns Object with category and optional subcategory
 */
export function extractCategoryDetails(path: string): {
  category: string;
  subcategory: string | null;
} {
  if (!path) return { category: 'Uncategorized', subcategory: null };

  const normalizedPath = path.replace(/\\/g, '/');
  const segments = normalizedPath.split('/').filter(Boolean);

  const presetIndex = segments.findIndex(
    (s) => s.endsWith('.milk') || s.endsWith('.prjm')
  );

  if (presetIndex < 0) return { category: 'Uncategorized', subcategory: null };

  // If we have 2+ levels above preset, first is category, second is subcategory
  if (presetIndex >= 2) {
    return {
      category: segments[presetIndex - 2],
      subcategory: segments[presetIndex - 1]
    };
  }

  // If we have 1 level above preset, it's the category
  if (presetIndex >= 1) {
    return {
      category: segments[presetIndex - 1],
      subcategory: null
    };
  }

  return { category: 'Uncategorized', subcategory: null };
}

/**
 * Build categories index from preset list
 * @param presets - Array of presets with path property
 */
export function buildCategoriesIndex(presets: Array<{ path: string }>): void {
  const newCategories = new Map<string, number>();

  for (const preset of presets) {
    const category = extractCategory(preset.path);
    const count = newCategories.get(category) || 0;
    newCategories.set(category, count + 1);
  }

  // Sort categories alphabetically, but put 'Uncategorized' last
  const sortedCategories = new Map(
    [...newCategories.entries()].sort(([a], [b]) => {
      if (a === 'Uncategorized') return 1;
      if (b === 'Uncategorized') return -1;
      return a.localeCompare(b);
    })
  );

  categoriesState.categories = sortedCategories;
}

/**
 * Get all category names
 * @returns Array of category names
 */
export function getAllCategories(): string[] {
  return [...categoriesState.categories.keys()];
}

/**
 * Get the count of presets in a category
 * @param category - Category name
 * @returns Number of presets in the category
 */
export function getCategoryCount(category: string): number {
  return categoriesState.categories.get(category) || 0;
}

/**
 * Get total number of categories
 * @returns Number of unique categories
 */
export function getTotalCategories(): number {
  return categoriesState.categories.size;
}

/**
 * Get category with counts as array
 * @returns Array of [category, count] tuples
 */
export function getCategoriesWithCounts(): Array<[string, number]> {
  return [...categoriesState.categories.entries()];
}

/**
 * Get the current categories state (reactive)
 */
export function getCategories(): CategoriesState {
  return categoriesState;
}

export { categoriesState as categories };
