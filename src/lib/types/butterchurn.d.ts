/**
 * Minimal type declarations for butterchurn, butterchurn-presets, and
 * milkdrop-preset-converter. These packages ship no TypeScript definitions.
 */

declare module 'butterchurn' {
  export interface VisualizerOptions {
    width?: number
    height?: number
    meshWidth?: number
    meshHeight?: number
    pixelRatio?: number
    textureRatio?: number
    outputFXAA?: boolean
    deterministic?: boolean
    testMode?: boolean
    onlyUseWASM?: boolean
  }

  export interface AudioLevels {
    bass: number
    mid: number
    treb: number
    bass_att: number
    mid_att: number
    treb_att: number
  }

  export interface RendererSizeOptions {
    meshWidth?: number
    meshHeight?: number
    pixelRatio?: number
    textureRatio?: number
  }

  export interface Visualizer {
    connectAudio(audioNode: AudioNode): void
    loadPreset(preset: object, blendTime?: number): void
    setRendererSize(width: number, height: number, opts?: RendererSizeOptions): void
    setInternalMeshSize(meshWidth: number, meshHeight: number): void
    setOutputAA(enabled: boolean): void
    render(opts?: { audioLevels?: Float32Array; elapsedTime?: number }): AudioLevels
    dispose(): void
  }

  // The class itself is the default export; createVisualizer is a static method.
  // Vite may also wrap it as { default: typeof butterchurn } depending on bundling.
  class butterchurnClass {
    static createVisualizer(
      audioContext: AudioContext,
      canvas: HTMLCanvasElement,
      opts?: VisualizerOptions
    ): Visualizer
  }

  export default butterchurnClass
}

declare module 'butterchurn-presets' {
  const butterchurnPresets: {
    getPresets(): Record<string, object>
  }
  export default butterchurnPresets
}

declare module 'butterchurn/lib/isSupported.min' {
  const isSupported: () => boolean
  export default isSupported
}

declare module 'milkdrop-preset-converter' {
  export function convertPreset(text: string): Promise<object>
}
