/** Favorite colors: index 1-5, 0 = not a favorite */
export const FAV_COLORS: readonly string[] = [
  '',
  '#ff4444',
  '#ff8800',
  '#ffdd00',
  '#44ff88',
  '#4488ff',
]

const FAV_KEY = 'od-preset-favorites'

export function loadFavColors(): Record<string, number> {
  try {
    return JSON.parse(localStorage.getItem(FAV_KEY) ?? '{}')
  } catch {
    return {}
  }
}

export function saveFavColors(favs: Record<string, number>): void {
  localStorage.setItem(FAV_KEY, JSON.stringify(favs))
}

const MOOD_LABELS_KEY = 'od-mood-labels'

export function loadMoodLabels(): Record<number, string> {
  try {
    return JSON.parse(localStorage.getItem(MOOD_LABELS_KEY) ?? '{}')
  } catch {
    return {}
  }
}

export function saveMoodLabels(labels: Record<number, string>): void {
  localStorage.setItem(MOOD_LABELS_KEY, JSON.stringify(labels))
}
