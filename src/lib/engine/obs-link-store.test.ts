import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

describe('obs-link-store persistence', () => {
	beforeEach(() => {
		const store = new Map<string, string>();
		vi.stubGlobal('localStorage', {
			getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
			setItem: (k: string, v: string) => { store.set(k, v); },
			removeItem: (k: string) => { store.delete(k); },
			clear: () => { store.clear(); },
		});
	});
	afterEach(() => { vi.unstubAllGlobals(); vi.resetModules(); });

	it('hydrates obsLinkState from persisted config on module load', async () => {
		localStorage.setItem('od-obs-config', JSON.stringify({
			host: '192.168.1.50', port: 4444,
			mapping: [{ sceneName: 'Chill', target: { type: 'slot', slot: 0 } }],
		}));
		const { obsLinkState } = await import('./obs-link-store.svelte.js');
		expect(obsLinkState.host).toBe('192.168.1.50');
		expect(obsLinkState.port).toBe(4444);
		expect(obsLinkState.mapping).toEqual([{ sceneName: 'Chill', target: { type: 'slot', slot: 0 } }]);
	});

	it('falls back to defaults when nothing is persisted', async () => {
		const { obsLinkState } = await import('./obs-link-store.svelte.js');
		expect(obsLinkState.host).toBe('localhost');
		expect(obsLinkState.port).toBe(4455);
		expect(obsLinkState.mapping).toEqual([]);
	});

	it('saveObsConfig persists the current state', async () => {
		const { obsLinkState, saveObsConfig } = await import('./obs-link-store.svelte.js');
		obsLinkState.host = '10.0.0.5';
		obsLinkState.port = 4455;
		obsLinkState.mapping = [{ sceneName: 'Hype', target: { type: 'mood', colorIndex: 3 } }];
		saveObsConfig();
		const persisted = JSON.parse(localStorage.getItem('od-obs-config')!);
		expect(persisted).toEqual({
			host: '10.0.0.5', port: 4455,
			mapping: [{ sceneName: 'Hype', target: { type: 'mood', colorIndex: 3 } }],
		});
	});
});
