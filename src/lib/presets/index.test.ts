import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Mock $app/paths avant d'importer le module
vi.mock('$app/paths', () => ({ base: '' }))

// index.ts importe désormais cloud-presets.ts, qui importe $env/static/public —
// mock nécessaire pour que TOUS les imports de ce fichier (y compris ceux de
// describe('getSlug', ...) plus bas) résolvent, même sans usage direct du cloud.
vi.mock('$env/static/public', () => ({ PUBLIC_CLOUD_PRESETS_API: '' }))

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

describe('loadPresetData — cloud fallback', () => {
	beforeEach(() => {
		vi.resetModules()
		vi.doMock('$app/paths', () => ({ base: '' }))
		// Node's vitest environment here is 'node' (no jsdom), so `localStorage` isn't
		// a real global — stub an in-memory implementation, same as cloud-presets.test.ts.
		const store = new Map<string, string>()
		vi.stubGlobal('localStorage', {
			getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
			setItem: (k: string, v: string) => { store.set(k, v) },
			removeItem: (k: string) => { store.delete(k) },
			clear: () => { store.clear() },
		})
	})

	afterEach(() => {
		vi.unstubAllGlobals()
	})

	it('un nom absent du manifest statique et sans préfixe cloud -> null (comportement inchangé)', async () => {
		const { loadPresetData } = await import('./index.js')
		expect(await loadPresetData('Nom Inconnu')).toBeNull()
	})

	it('un nom préfixé ☁ sans token en localStorage -> null', async () => {
		const { loadPresetData } = await import('./index.js')
		expect(await loadPresetData('☁ Mon Preset')).toBeNull()
	})

	it('un nom préfixé ☁ avec token -> résout via loadCloudPresetData', async () => {
		localStorage.setItem('od-cloud-token', 'tok1')
		vi.stubGlobal('fetch', vi.fn()
			.mockResolvedValueOnce({ ok: true, json: async () => [{ id: 'a', name: '☁ Mon Preset', sizeBytes: 10, uploadedAt: 1 }] })
			.mockResolvedValueOnce({ ok: true, json: async () => ({ frame_eqs_str: 'a.zoom=1;' }) }))
		vi.doMock('$env/static/public', () => ({ PUBLIC_CLOUD_PRESETS_API: 'https://presets-cloud.example' }))
		const { loadPresetData } = await import('./index.js')
		expect(await loadPresetData('☁ Mon Preset')).toEqual({ frame_eqs_str: 'a.zoom=1;' })
	})
})
