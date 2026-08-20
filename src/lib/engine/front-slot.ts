/**
 * front-slot.ts — pure helpers to answer "which deck is at the front right now"
 * and "what mood is it tagged with". Built on the existing `opacities` derived
 * array (deckState.deckBus × crossfader) and the existing favorites color map
 * (src/lib/presets/favorites.ts) — no new state introduced.
 */

export function frontSlotIndex(opacities: number[]): 0 | 1 | 2 | 3 {
  let best = 0
  for (let i = 1; i < opacities.length; i++) {
    if (opacities[i]! > opacities[best]!) best = i
  }
  return best as 0 | 1 | 2 | 3
}

export function frontSlotMood(
  favColors: Record<string, number>,
  presets4: string[],
  frontSlot: number
): 1 | 2 | 3 | 4 | 5 | null {
  const presetName = presets4[frontSlot]
  if (!presetName) return null
  const colorIndex = favColors[presetName]
  return colorIndex ? (colorIndex as 1 | 2 | 3 | 4 | 5) : null
}
