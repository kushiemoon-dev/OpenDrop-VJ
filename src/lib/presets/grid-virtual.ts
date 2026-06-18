export interface GridWindow {
  cols: number
  vStart: number
  vEnd: number
  offsetY: number
  totalH: number
}

export function computeGrid(opts: {
  count: number
  containerW: number
  containerH: number
  scrollTop: number
  cardMinW: number
  cardH: number
  gap: number
  overscanRows?: number
}): GridWindow {
  const { count, containerW, containerH, scrollTop, cardMinW, cardH, gap } = opts
  const overscan = opts.overscanRows ?? 2

  const cols = Math.max(1, Math.floor((containerW + gap) / (cardMinW + gap)))
  const rowH = cardH + gap
  const rows = Math.ceil(count / cols)
  const totalH = rows * rowH

  const firstRow = Math.max(0, Math.floor(scrollTop / rowH) - overscan)
  const visRows = Math.ceil(containerH / rowH) + 2 * overscan
  const vStart = firstRow * cols
  const vEnd = Math.min(count, (firstRow + visRows) * cols)
  const offsetY = firstRow * rowH

  return { cols, vStart, vEnd, offsetY, totalH }
}
