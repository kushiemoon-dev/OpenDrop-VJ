// workers/presets-cloud/src/index.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import worker, { type Env } from './index.js';
import type { Bucket } from './handlers.js';

class FakeBucket implements Bucket {
	private store = new Map<string, string>();
	async get(key: string) {
		const v = this.store.get(key);
		return v === undefined ? null : { text: async () => v };
	}
	async put(key: string, value: string) { this.store.set(key, value); }
	async delete(key: string) { this.store.delete(key); }
}

function makeEnv(): Env {
	return { PRESETS_BUCKET: new FakeBucket() as unknown as Env['PRESETS_BUCKET'] };
}

describe('router (index.ts fetch)', () => {
	let env: Env;
	beforeEach(() => { env = makeEnv(); });

	it('POST /presets avec un corps JSON invalide -> 400 avec {error}', async () => {
		const req = new Request('https://worker.example/presets', {
			method: 'POST',
			headers: { 'X-Cloud-Token': 'tok1', 'Content-Type': 'application/json' },
			body: 'not json',
		});
		const res = await worker.fetch(req, env);
		expect(res.status).toBe(400);
		expect(await res.json()).toEqual({ error: 'invalid JSON body' });
	});

	it('PATCH /presets/:id avec un corps JSON invalide -> 400', async () => {
		const req = new Request('https://worker.example/presets/abc', {
			method: 'PATCH',
			headers: { 'X-Cloud-Token': 'tok1', 'Content-Type': 'application/json' },
			body: 'not json',
		});
		const res = await worker.fetch(req, env);
		expect(res.status).toBe(400);
		expect(await res.json()).toEqual({ error: 'invalid JSON body' });
	});

	it('OPTIONS -> 204 avec les en-têtes CORS', async () => {
		const req = new Request('https://worker.example/presets', { method: 'OPTIONS' });
		const res = await worker.fetch(req, env);
		expect(res.status).toBe(204);
		expect(res.headers.get('Access-Control-Allow-Origin')).toBe('*');
		expect(res.headers.get('Access-Control-Allow-Headers')).toContain('X-Cloud-Token');
	});

	it('POST /presets puis GET /presets : round-trip via le routeur, token lu depuis le header X-Cloud-Token', async () => {
		const postReq = new Request('https://worker.example/presets', {
			method: 'POST',
			headers: { 'X-Cloud-Token': 'tok1', 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: 'Mon preset', data: { x: 1 } }),
		});
		const postRes = await worker.fetch(postReq, env);
		expect(postRes.status).toBe(200);
		const { id } = await postRes.json() as { id: string };
		expect(id).toBeTruthy();

		const getReq = new Request('https://worker.example/presets', {
			method: 'GET',
			headers: { 'X-Cloud-Token': 'tok1' },
		});
		const getRes = await worker.fetch(getReq, env);
		const index = await getRes.json() as Array<{ id: string; name: string }>;
		expect(index).toHaveLength(1);
		expect(index[0].id).toBe(id);
		expect(index[0].name).toBe('Mon preset');

		// A different token must see nothing — proves the router reads the token from the
		// X-Cloud-Token header (and scopes storage by it), not from a query string or body field.
		const otherReq = new Request('https://worker.example/presets', {
			method: 'GET',
			headers: { 'X-Cloud-Token': 'tok2' },
		});
		const otherRes = await worker.fetch(otherReq, env);
		expect(await otherRes.json()).toEqual([]);
	});

	it('chemin inconnu -> 404', async () => {
		const req = new Request('https://worker.example/nope', { method: 'GET' });
		const res = await worker.fetch(req, env);
		expect(res.status).toBe(404);
	});
});
