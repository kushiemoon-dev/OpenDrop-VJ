import { describe, it, expect } from 'vitest'
import { isMilkPresetFilename, convertMilkPreset } from './milk-import.js'

// A real, minimal community MilkDrop preset (Rocke - Cold Love, public MilkDrop
// original pack, 912 bytes) — used to prove the converter round-trips through
// this project's actual toolchain, not just its own upstream test suite.
const SAMPLE_MILK = `[preset00]
fRating=3.000000
fGammaAdj=1.9
fDecay=0.982
fVideoEchoZoom=1.00011
fVideoEchoAlpha=0.5
nVideoEchoOrientation=1
nWaveMode=5
bAdditiveWaves=1
bWaveDots=1
bModWaveAlphaByVolume=0
bMaximizeWaveColor=1
bTexWrap=0
bDarkenCenter=0
bMotionVectorsOn=0
bRedBlueStereo=0
nMotionVectorsX=2
nMotionVectorsY=2
bBrighten=0
bDarken=1
bSolarize=0
bInvert=0
fWaveAlpha=1.22
fWaveScale=1.1704
fWaveSmoothing=0.75
fWaveParam=0
fModWaveAlphaStart=0.75
fModWaveAlphaEnd=0.95
fWarpAnimSpeed=1
fWarpScale=1
fZoomExponent=1
fShader=0
zoom=1.01
rot=0
cx=0.5
cy=0.5
dx=0
dy=0
warp=0.01
sx=1
sy=1
wave_r=1
wave_g=1
wave_b=1
wave_x=0.5
wave_y=0.5
ob_size=0.01
ob_r=0
ob_g=0
ob_b=0
ob_a=0
ib_size=0
ib_r=0.25
ib_g=0.25
ib_b=0.25
ib_a=0
per_frame_1=wave_r = wave_r + 0.4*sin(time*3.14) + 0.2*mid;
per_frame_2=wave_b = wave_b + 0.2*sin(time*1.5);
per_pixel_1=zoom = zoom + 0.001*sin(rad*6.0+time);
`

describe('isMilkPresetFilename', () => {
  it('accepts .milk and .prjm, case-insensitively', () => {
    expect(isMilkPresetFilename('Geiss - Feedback.milk')).toBe(true)
    expect(isMilkPresetFilename('SomePreset.MILK')).toBe(true)
    expect(isMilkPresetFilename('SomePreset.prjm')).toBe(true)
    expect(isMilkPresetFilename('SomePreset.PRJM')).toBe(true)
  })

  it('rejects other extensions', () => {
    expect(isMilkPresetFilename('clip.mp4')).toBe(false)
    expect(isMilkPresetFilename('overlay.png')).toBe(false)
    expect(isMilkPresetFilename('milk.txt')).toBe(false)
  })
})

describe('convertMilkPreset', () => {
  it('converts a real .milk file into Butterchurn preset JSON', async () => {
    const preset = (await convertMilkPreset(SAMPLE_MILK)) as Record<string, unknown>
    expect(typeof preset.frame_eqs_str).toBe('string')
    expect(typeof preset.pixel_eqs_str).toBe('string')
    expect(Array.isArray(preset.shapes)).toBe(true)
    expect(Array.isArray(preset.waves)).toBe(true)
  })

  it('rejects text with no [preset00] header (the underlying converter silently returns an empty preset otherwise)', async () => {
    await expect(convertMilkPreset('not a preset')).rejects.toThrow()
  })
})
