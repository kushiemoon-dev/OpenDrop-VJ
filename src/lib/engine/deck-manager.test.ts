import { describe, it, expect, vi, beforeEach } from 'vitest'

// mock relatif — pas d'alias $lib en env:node
const mockDeckInstance = {
  init: vi.fn().mockResolvedValue(undefined),
  connectAudio: vi.fn(),
  loadPreset: vi.fn(),
  startRenderLoop: vi.fn(),
  resume: vi.fn(),
  pause: vi.fn(),
  applyQuality: vi.fn(),
  resize: vi.fn(),
  destroy: vi.fn(),
  state: 'running' as 'idle' | 'running' | 'stopped',
}

const MockDeck = vi.fn(() => mockDeckInstance)

vi.mock('./deck.js', () => ({ Deck: MockDeck }))

// import APRÈS le mock
const { DeckManager } = await import('./deck-manager.js')

describe('DeckManager', () => {
  let manager: InstanceType<typeof DeckManager>
  const mockCanvas = { clientWidth: 1280, clientHeight: 720 } as HTMLCanvasElement
  const audioCtx = {} as AudioContext
  const audioNode = {} as AudioNode

  beforeEach(() => {
    vi.clearAllMocks()
    mockDeckInstance.state = 'running'
    manager = new DeckManager()
    manager.attachCanvas(0, mockCanvas)
    manager.attachCanvas(1, mockCanvas)
  })

  it('start() crée et initialise un Deck au premier appel', async () => {
    await manager.start(0, audioCtx, audioNode, {}, null)
    expect(MockDeck).toHaveBeenCalledWith(mockCanvas, 'deck-0')
    expect(mockDeckInstance.init).toHaveBeenCalledTimes(1)
    expect(mockDeckInstance.startRenderLoop).toHaveBeenCalledTimes(1)
  })

  it('start() charge le preset si fourni', async () => {
    const preset = { name: 'test' }
    await manager.start(0, audioCtx, audioNode, {}, preset)
    expect(mockDeckInstance.loadPreset).toHaveBeenCalledWith(preset, 0.0)
  })

  it('start() appelle resume() au deuxième appel, sans re-init', async () => {
    await manager.start(0, audioCtx, audioNode, {}, null)
    MockDeck.mockClear()
    mockDeckInstance.init.mockClear()
    await manager.start(0, audioCtx, audioNode, {}, null)
    expect(MockDeck).not.toHaveBeenCalled()
    expect(mockDeckInstance.init).not.toHaveBeenCalled()
    expect(mockDeckInstance.resume).toHaveBeenCalledTimes(1)
  })

  it('pause() appelle deck.pause()', async () => {
    await manager.start(0, audioCtx, audioNode, {}, null)
    manager.pause(0)
    expect(mockDeckInstance.pause).toHaveBeenCalledTimes(1)
  })

  it('pause() sur slot non initialisé ne plante pas', () => {
    expect(() => manager.pause(3)).not.toThrow()
  })

  it('isRunning() retourne false sur slot non initialisé', () => {
    expect(manager.isRunning(0)).toBe(false)
  })

  it('isRunning() retourne true après start()', async () => {
    await manager.start(0, audioCtx, audioNode, {}, null)
    expect(manager.isRunning(0)).toBe(true)
  })

  it('runningCount() retourne le nombre de slots running', async () => {
    await manager.start(0, audioCtx, audioNode, {}, null)
    await manager.start(1, audioCtx, audioNode, {}, null)
    expect(manager.runningCount()).toBe(2)
  })

  it('runningCount() = 0 si aucun slot init', () => {
    expect(manager.runningCount()).toBe(0)
  })

  it('connectAudio() re-route tous les slots initialisés', async () => {
    await manager.start(0, audioCtx, audioNode, {}, null)
    mockDeckInstance.connectAudio.mockClear()
    const newNode = {} as AudioNode
    manager.connectAudio(newNode)
    expect(mockDeckInstance.connectAudio).toHaveBeenCalledWith(newNode)
  })

  it('loadPreset() sur slot non initialisé ne plante pas', () => {
    expect(() => manager.loadPreset(2, { name: 'test' }, 2.0)).not.toThrow()
  })

  it('destroyAll() détruit tous les slots et les reset', async () => {
    await manager.start(0, audioCtx, audioNode, {}, null)
    manager.destroyAll()
    expect(mockDeckInstance.destroy).toHaveBeenCalledTimes(1)
    expect(manager.runningCount()).toBe(0)
    expect(manager.isRunning(0)).toBe(false)
  })
})
