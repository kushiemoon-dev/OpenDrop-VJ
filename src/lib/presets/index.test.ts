import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Mock $app/paths avant d'importer le module
vi.mock('$app/paths', () => ({ base: '' }))

// Mock fetch avant d'importer initPresets
const FAKE_MANIFEST = {
	entries: [
		{ slug: 'category/cool-preset', name: 'Cool Category - Cool Preset' },
		{ slug: 'category/other-preset', name: 'Cool Category - Other Preset' },
		{ slug: 'another/preset', name: 'Another - Preset' },
	],
}

import { initPresets, getSlug } from './index.js'

describe('getSlug', () => {
	beforeEach(() => {
		vi.resetModules()
		vi.doMock('$app/paths', () => ({ base: '' }))
	})

	afterEach(() => {
		vi.unstubAllGlobals()
	})

	it('retourne le slug pour un nom connu après initPresets', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({
			ok: true,
			json: async () => FAKE_MANIFEST,
		}))
		const { initPresets: init, getSlug: slug } = await import('./index.js')

		await init()

		expect(slug('Cool Category - Cool Preset')).toBe('category/cool-preset')
		expect(slug('Cool Category - Other Preset')).toBe('category/other-preset')
		expect(slug('Another - Preset')).toBe('another/preset')
	})

	it('retourne undefined pour un nom inconnu', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({
			ok: true,
			json: async () => FAKE_MANIFEST,
		}))
		const { initPresets: init, getSlug: slug } = await import('./index.js')

		await init()

		expect(slug('inexistant/preset')).toBeUndefined()
		expect(slug('Unknown - Preset')).toBeUndefined()
	})

	it('retourne undefined avant initPresets', async () => {
		// Sans appeler initPresets, _nameToSlug est vide
		const { getSlug: slug } = await import('./index.js')
		expect(slug('Cool Category - Cool Preset')).toBeUndefined()
	})
})
