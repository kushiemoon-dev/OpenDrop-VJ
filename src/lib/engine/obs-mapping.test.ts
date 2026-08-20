import { describe, it, expect } from 'vitest'
import { findSceneForTarget, findTargetForScene, type MappingEntry } from './obs-mapping.js'

const mapping: MappingEntry[] = [
  { sceneName: 'Chill', target: { type: 'slot', slot: 0 } },
  { sceneName: 'Hype', target: { type: 'mood', colorIndex: 3 } },
]

describe('obs-mapping', () => {
  it('finds the scene for a slot target', () => {
    expect(findSceneForTarget(mapping, { type: 'slot', slot: 0 })).toBe('Chill')
  })

  it('finds the scene for a mood target', () => {
    expect(findSceneForTarget(mapping, { type: 'mood', colorIndex: 3 })).toBe('Hype')
  })

  it('returns undefined when no entry matches', () => {
    expect(findSceneForTarget(mapping, { type: 'slot', slot: 2 })).toBeUndefined()
  })

  it('finds the target for a known scene name', () => {
    expect(findTargetForScene(mapping, 'Hype')).toEqual({ type: 'mood', colorIndex: 3 })
  })

  it('returns undefined for an unknown scene name', () => {
    expect(findTargetForScene(mapping, 'Unknown Scene')).toBeUndefined()
  })
})
