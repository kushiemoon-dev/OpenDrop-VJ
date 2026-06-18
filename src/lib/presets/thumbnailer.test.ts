import { describe, it, expect, vi } from 'vitest'

// butterchurn is a browser-only UMD that references `window` at module load.
// Mock it before importing thumbnailer to keep this test in node environment.
vi.mock('butterchurn', () => ({
	default: { createVisualizer: vi.fn() },
}))

// Mock browser-only modules used by thumbnailer
vi.mock('./thumb-cache.js', () => ({
	getThumbUrl: vi.fn().mockResolvedValue(null),
	putThumbBlob: vi.fn().mockResolvedValue(undefined),
	cacheUrl: vi.fn().mockReturnValue('blob:mock'),
}))

vi.mock('./index.js', () => ({
	loadPresetData: vi.fn().mockResolvedValue(null),
}))

import { enqueueFront, dequeueJob } from './thumbnailer.svelte.js'

describe('enqueueFront', () => {
	it('ajoute un job en tête', () => {
		const q = [{ slug: 'a', name: 'A' }]
		const result = enqueueFront(q, { slug: 'b', name: 'B' })
		expect(result[0].slug).toBe('b')
		expect(result[1].slug).toBe('a')
	})

	it('déduplique un slug existant', () => {
		const q = [{ slug: 'a', name: 'A' }, { slug: 'b', name: 'B' }]
		const result = enqueueFront(q, { slug: 'a', name: 'A' })
		expect(result).toHaveLength(2)
		expect(result[0].slug).toBe('a')
		expect(result[1].slug).toBe('b')
	})

	it('file vide + ajout → [job]', () => {
		expect(enqueueFront([], { slug: 'x', name: 'X' })).toHaveLength(1)
	})
})

describe('dequeueJob', () => {
	it('retourne null pour file vide', () => {
		const [job, rest] = dequeueJob([])
		expect(job).toBeNull()
		expect(rest).toHaveLength(0)
	})

	it('retourne le premier job et le reste', () => {
		const q = [{ slug: 'a', name: 'A' }, { slug: 'b', name: 'B' }]
		const [job, rest] = dequeueJob(q)
		expect(job?.slug).toBe('a')
		expect(rest).toHaveLength(1)
		expect(rest[0].slug).toBe('b')
	})
})
