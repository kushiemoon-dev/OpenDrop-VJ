import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';

vi.mock('$env/static/public', () => ({ PUBLIC_CLOUD_PRESETS_API: 'https://presets-cloud.example' }));

describe('parsePresetFile', () => {
	it('parse un JSON valide représentant un objet', async () => {
		const { parsePresetFile } = await import('./cloud-presets.js');
		expect(parsePresetFile('{"frame_eqs_str":"a.zoom=1;"}')).toEqual({ frame_eqs_str: 'a.zoom=1;' });
	});

	it('lève une erreur sur du JSON invalide', async () => {
		const { parsePresetFile } = await import('./cloud-presets.js');
		expect(() => parsePresetFile('not json')).toThrow();
	});

	it('lève une erreur si le JSON parsé n\'est pas un objet (ex: un tableau ou un nombre)', async () => {
		const { parsePresetFile } = await import('./cloud-presets.js');
		expect(() => parsePresetFile('[1,2,3]')).toThrow();
		expect(() => parsePresetFile('42')).toThrow();
	});
});

describe('getOrCreateCloudToken / setCloudToken', () => {
	// Node's vitest environment here is 'node' (no jsdom), so `localStorage` isn't
	// a real global — stub an in-memory implementation before each test.
	beforeEach(() => {
		const store = new Map<string, string>();
		vi.stubGlobal('localStorage', {
			getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
			setItem: (k: string, v: string) => { store.set(k, v); },
			removeItem: (k: string) => { store.delete(k); },
			clear: () => { store.clear(); },
		});
		localStorage.clear();
	});
	afterEach(() => { vi.unstubAllGlobals(); });

	it('génère et persiste un token au premier appel', async () => {
		const { getOrCreateCloudToken } = await import('./cloud-presets.js');
		const token = getOrCreateCloudToken();
		expect(token).toMatch(/^[0-9a-f-]{36}$/);
		expect(localStorage.getItem('od-cloud-token')).toBe(token);
	});

	it('retourne le même token aux appels suivants', async () => {
		const { getOrCreateCloudToken } = await import('./cloud-presets.js');
		const first = getOrCreateCloudToken();
		const second = getOrCreateCloudToken();
		expect(second).toBe(first);
	});

	it('setCloudToken écrase le token existant', async () => {
		const { getOrCreateCloudToken, setCloudToken } = await import('./cloud-presets.js');
		getOrCreateCloudToken();
		setCloudToken('mon-token-a-moi');
		expect(localStorage.getItem('od-cloud-token')).toBe('mon-token-a-moi');
	});
});

describe('getCloudPresetIndex', () => {
	// Dynamic import() caches the module across tests unless modules are reset —
	// re-establish the default (non-empty) env mock before every test, matching
	// the pattern in src/lib/video-loops/index.test.ts.
	beforeEach(() => {
		vi.resetModules();
		vi.doMock('$env/static/public', () => ({ PUBLIC_CLOUD_PRESETS_API: 'https://presets-cloud.example' }));
	});
	afterEach(() => { vi.unstubAllGlobals(); });

	it('retourne [] si PUBLIC_CLOUD_PRESETS_API est vide', async () => {
		vi.resetModules();
		vi.doMock('$env/static/public', () => ({ PUBLIC_CLOUD_PRESETS_API: '' }));
		const { getCloudPresetIndex } = await import('./cloud-presets.js');
		expect(await getCloudPresetIndex('tok1')).toEqual([]);
	});

	it('retourne [] si le token est vide', async () => {
		const { getCloudPresetIndex } = await import('./cloud-presets.js');
		expect(await getCloudPresetIndex('')).toEqual([]);
	});

	it('retourne la liste sur succès', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({
			ok: true,
			json: async () => [{ id: 'a', name: '☁ X', sizeBytes: 10, uploadedAt: 1 }],
		}));
		const { getCloudPresetIndex } = await import('./cloud-presets.js');
		expect(await getCloudPresetIndex('tok1')).toEqual([{ id: 'a', name: '☁ X', sizeBytes: 10, uploadedAt: 1 }]);
	});

	it('retourne [] si le fetch échoue (jamais une exception)', async () => {
		vi.stubGlobal('fetch', vi.fn().mockRejectedValueOnce(new Error('network')));
		const { getCloudPresetIndex } = await import('./cloud-presets.js');
		expect(await getCloudPresetIndex('tok1')).toEqual([]);
	});

	it('retourne [] si la réponse n\'est pas ok', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({ ok: false }));
		const { getCloudPresetIndex } = await import('./cloud-presets.js');
		expect(await getCloudPresetIndex('tok1')).toEqual([]);
	});
});

describe('uploadPreset', () => {
	beforeEach(() => {
		vi.resetModules();
		vi.doMock('$env/static/public', () => ({ PUBLIC_CLOUD_PRESETS_API: 'https://presets-cloud.example' }));
	});
	afterEach(() => { vi.unstubAllGlobals(); });

	it('préfixe le nom avec CLOUD_PRESET_PREFIX avant l\'envoi', async () => {
		const fetchMock = vi.fn().mockResolvedValueOnce({ ok: true, json: async () => ({ id: 'new-id' }) });
		vi.stubGlobal('fetch', fetchMock);
		const { uploadPreset } = await import('./cloud-presets.js');
		const result = await uploadPreset('tok1', 'Mon Preset', { x: 1 });
		expect(result).toEqual({ id: 'new-id' });
		const body = JSON.parse(fetchMock.mock.calls[0][1].body);
		expect(body.name).toBe('☁ Mon Preset');
	});

	it('retourne {error} si le Worker répond une erreur', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({ ok: false, status: 413, json: async () => ({ error: 'quota exceeded: max 300 presets' }) }));
		const { uploadPreset } = await import('./cloud-presets.js');
		const result = await uploadPreset('tok1', 'name', { x: 1 });
		expect(result).toEqual({ error: 'quota exceeded: max 300 presets' });
	});

	it('retourne {error} si le fetch échoue (jamais une exception)', async () => {
		vi.stubGlobal('fetch', vi.fn().mockRejectedValueOnce(new Error('network')));
		const { uploadPreset } = await import('./cloud-presets.js');
		const result = await uploadPreset('tok1', 'name', { x: 1 });
		expect('error' in result).toBe(true);
	});
});

describe('loadCloudPresetData', () => {
	beforeEach(() => {
		vi.resetModules();
		vi.doMock('$env/static/public', () => ({ PUBLIC_CLOUD_PRESETS_API: 'https://presets-cloud.example' }));
	});
	afterEach(() => { vi.unstubAllGlobals(); });

	it('résout le preset par nom via l\'index puis fetch son contenu', async () => {
		const fetchMock = vi.fn()
			.mockResolvedValueOnce({ ok: true, json: async () => [{ id: 'a', name: '☁ X', sizeBytes: 10, uploadedAt: 1 }] })
			.mockResolvedValueOnce({ ok: true, json: async () => ({ frame_eqs_str: 'a.zoom=1;' }) });
		vi.stubGlobal('fetch', fetchMock);
		const { loadCloudPresetData } = await import('./cloud-presets.js');
		expect(await loadCloudPresetData('tok1', '☁ X')).toEqual({ frame_eqs_str: 'a.zoom=1;' });
	});

	it('nom absent de l\'index -> null', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({ ok: true, json: async () => [] }));
		const { loadCloudPresetData } = await import('./cloud-presets.js');
		expect(await loadCloudPresetData('tok1', '☁ Inconnu')).toBeNull();
	});
});
