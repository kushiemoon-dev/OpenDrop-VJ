import { describe, it, expect } from 'vitest'
import { computeGrid } from './grid-virtual'

describe('computeGrid', () => {
  it('calculates columns correctly from container width', () => {
    // containerW=900, cardMinW=150, gap=8 → cols = floor((900+8)/(150+8)) = floor(908/158) = 5
    const result1 = computeGrid({
      count: 100,
      containerW: 900,
      containerH: 400,
      scrollTop: 0,
      cardMinW: 150,
      cardH: 100,
      gap: 8,
    })
    expect(result1.cols).toBe(5)

    // containerW=500, cardMinW=200, gap=8 → cols = floor(508/208) = 2
    const result2 = computeGrid({
      count: 100,
      containerW: 500,
      containerH: 400,
      scrollTop: 0,
      cardMinW: 200,
      cardH: 100,
      gap: 8,
    })
    expect(result2.cols).toBe(2)

    // containerW=100, cardMinW=200, gap=8 → max(1, ...) = 1 (force minimum 1)
    const result3 = computeGrid({
      count: 100,
      containerW: 100,
      containerH: 400,
      scrollTop: 0,
      cardMinW: 200,
      cardH: 100,
      gap: 8,
    })
    expect(result3.cols).toBe(1)
  })

  it('returns vStart=0 when scrollTop=0', () => {
    const result = computeGrid({
      count: 100,
      containerW: 900,
      containerH: 400,
      scrollTop: 0,
      cardMinW: 150,
      cardH: 100,
      gap: 8,
      overscanRows: 2,
    })
    expect(result.vStart).toBe(0)
  })

  it('computes correct window for large scroll', () => {
    // 1000 items, 5 cols, cardH=100, gap=8, containerH=400, scrollTop=500
    // rowH=108, firstRow = max(0, floor(500/108)-2) = max(0, 4-2) = 2
    // vStart = 2*5 = 10
    // visRows = ceil(400/108)+4 = 4+4 = 8 → vEnd = min(1000, (2+8)*5) = min(1000, 50) = 50
    const result = computeGrid({
      count: 1000,
      containerW: 900,
      containerH: 400,
      scrollTop: 500,
      cardMinW: 150,
      cardH: 100,
      gap: 8,
      overscanRows: 2,
    })
    expect(result.cols).toBe(5)
    expect(result.vStart).toBe(10)
    expect(result.vEnd).toBe(50)
  })

  it('handles partial last row (count not multiple of cols)', () => {
    // 11 items, 5 cols : rows = ceil(11/5) = 3, totalH = 3*rowH
    // vEnd never exceeds count=11
    const result = computeGrid({
      count: 11,
      containerW: 900,
      containerH: 400,
      scrollTop: 0,
      cardMinW: 150,
      cardH: 100,
      gap: 8,
    })
    expect(result.cols).toBe(5)
    const rowH = 108
    const totalH = Math.ceil(11 / 5) * rowH
    expect(result.totalH).toBe(totalH)
    expect(result.vEnd).toBeLessThanOrEqual(11)
  })

  it('handles empty grid (count=0)', () => {
    const result = computeGrid({
      count: 0,
      containerW: 900,
      containerH: 400,
      scrollTop: 0,
      cardMinW: 150,
      cardH: 100,
      gap: 8,
    })
    expect(result.cols).toBeGreaterThanOrEqual(1)
    expect(result.vStart).toBe(0)
    expect(result.vEnd).toBe(0)
    expect(result.totalH).toBe(0)
  })

  it('respects overscan parameter', () => {
    // With overscanRows=3, scrollTop=0 : firstRow=max(0,0-3)=0 → vStart=0
    const result1 = computeGrid({
      count: 1000,
      containerW: 900,
      containerH: 400,
      scrollTop: 0,
      cardMinW: 150,
      cardH: 100,
      gap: 8,
      overscanRows: 3,
    })
    expect(result1.vStart).toBe(0)

    // With overscanRows=3, large scrollTop : firstRow includes 3 rows in advance
    const result2 = computeGrid({
      count: 1000,
      containerW: 900,
      containerH: 400,
      scrollTop: 1000,
      cardMinW: 150,
      cardH: 100,
      gap: 8,
      overscanRows: 3,
    })
    expect(result2.vEnd).toBeGreaterThan(result2.vStart)
  })
})
