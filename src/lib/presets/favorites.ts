/** Couleurs de favoris : index 1-5, 0 = pas favori */
export const FAV_COLORS: readonly string[] = ['', '#ff4444', '#ff8800', '#ffdd00', '#44ff88', '#4488ff']

const FAV_KEY = 'od-preset-favorites'

export function loadFavColors(): Record<string, number> {
  try { return JSON.parse(localStorage.getItem(FAV_KEY) ?? '{}') }
  catch { return {} }
}

export function saveFavColors(favs: Record<string, number>): void {
  localStorage.setItem(FAV_KEY, JSON.stringify(favs))
}
