// workers/presets-cloud/src/handlers.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { readIndex, handleUpload, handleGetPreset, handleRename, handleDelete, type Bucket } from './handlers.js';

class FakeBucket implements Bucket {
	private store = new Map<string, string>();
	async get(key: string) {
		const v = this.store.get(key);
		return v === undefined ? null : { text: async () => v };
	}
	async put(key: string, value: string) { this.store.set(key, value); }
	async delete(key: string) { this.store.delete(key); }
}

describe('readIndex', () => {
	it('retourne [] quand index.json n\'existe pas', async () => {
		const bucket = new FakeBucket();
		expect(await readIndex(bucket, 'tok1')).toEqual([]);
	});

	it('retourne le contenu parsé quand index.json existe', async () => {
		const bucket = new FakeBucket();
		await bucket.put('presets/tok1/index.json', JSON.stringify([{ id: 'a', name: 'X', sizeBytes: 10, uploadedAt: 1 }]));
		expect(await readIndex(bucket, 'tok1')).toEqual([{ id: 'a', name: 'X', sizeBytes: 10, uploadedAt: 1 }]);
	});

	it('retourne [] si index.json est corrompu (jamais une exception)', async () => {
		const bucket = new FakeBucket();
		await bucket.put('presets/tok1/index.json', 'not json');
		expect(await readIndex(bucket, 'tok1')).toEqual([]);
	});
});

describe('handleUpload', () => {
	let bucket: FakeBucket;
	beforeEach(() => { bucket = new FakeBucket(); });

	it('upload avec succès : écrit le preset + met à jour index.json', async () => {
		const result = await handleUpload(bucket, 'tok1', '☁ Mon preset', { frame_eqs_str: 'a.zoom=1;' });
		expect('id' in result).toBe(true);
		const id = (result as { id: string }).id;
		const index = await readIndex(bucket, 'tok1');
		expect(index).toHaveLength(1);
		expect(index[0].id).toBe(id);
		expect(index[0].name).toBe('☁ Mon preset');
		const stored = await bucket.get(`presets/tok1/${id}.json`);
		expect(JSON.parse(await stored!.text())).toEqual({ frame_eqs_str: 'a.zoom=1;' });
	});

	it('token manquant -> erreur 400', async () => {
		const result = await handleUpload(bucket, '', 'name', { a: 1 });
		expect(result).toEqual({ error: 'missing token', status: 400 });
	});

	it('nom manquant -> erreur 400', async () => {
		const result = await handleUpload(bucket, 'tok1', '', { a: 1 });
		expect(result).toEqual({ error: 'missing name', status: 400 });
	});

	it('data invalide (pas un objet) -> erreur 400', async () => {
		const result = await handleUpload(bucket, 'tok1', 'name', 'not-an-object' as unknown as object);
		expect(result).toEqual({ error: 'invalid preset data', status: 400 });
	});

	it('quota nombre de presets dépassé -> erreur 413', async () => {
		const existing = Array.from({ length: 300 }, (_, i) => ({ id: `p${i}`, name: `P${i}`, sizeBytes: 10, uploadedAt: 1 }));
		await bucket.put('presets/tok1/index.json', JSON.stringify(existing));
		const result = await handleUpload(bucket, 'tok1', 'one more', { a: 1 });
		expect(result).toEqual({ error: 'quota exceeded: max 300 presets', status: 413 });
	});

	it('quota taille totale dépassé -> erreur 413', async () => {
		await bucket.put('presets/tok1/index.json', JSON.stringify([{ id: 'big', name: 'Big', sizeBytes: 30 * 1024 * 1024, uploadedAt: 1 }]));
		const result = await handleUpload(bucket, 'tok1', 'one more', { a: 1 });
		expect(result).toEqual({ error: 'quota exceeded: max 31457280 bytes', status: 413 });
	});
});

describe('handleGetPreset', () => {
	it('retourne le preset stocké', async () => {
		const bucket = new FakeBucket();
		const { id } = await handleUpload(bucket, 'tok1', 'name', { x: 1 }) as { id: string };
		expect(await handleGetPreset(bucket, 'tok1', id)).toEqual({ x: 1 });
	});

	it('id inconnu -> null', async () => {
		const bucket = new FakeBucket();
		expect(await handleGetPreset(bucket, 'tok1', 'nope')).toBeNull();
	});

	it('token ou id manquant -> null', async () => {
		const bucket = new FakeBucket();
		expect(await handleGetPreset(bucket, '', 'x')).toBeNull();
		expect(await handleGetPreset(bucket, 'tok1', '')).toBeNull();
	});
});

describe('handleRename', () => {
	it('renomme l\'entrée dans index.json, ne touche pas le fichier preset', async () => {
		const bucket = new FakeBucket();
		const { id } = await handleUpload(bucket, 'tok1', 'old name', { x: 1 }) as { id: string };
		const result = await handleRename(bucket, 'tok1', id, 'new name');
		expect(result).toEqual({ ok: true });
		const index = await readIndex(bucket, 'tok1');
		expect(index[0].name).toBe('new name');
		expect(await handleGetPreset(bucket, 'tok1', id)).toEqual({ x: 1 });
	});

	it('id inconnu -> erreur 404', async () => {
		const bucket = new FakeBucket();
		const result = await handleRename(bucket, 'tok1', 'nope', 'x');
		expect(result).toEqual({ error: 'not found', status: 404 });
	});
});

describe('handleDelete', () => {
	it('supprime le preset et l\'entrée d\'index', async () => {
		const bucket = new FakeBucket();
		const { id } = await handleUpload(bucket, 'tok1', 'name', { x: 1 }) as { id: string };
		const result = await handleDelete(bucket, 'tok1', id);
		expect(result).toEqual({ ok: true });
		expect(await readIndex(bucket, 'tok1')).toEqual([]);
		expect(await handleGetPreset(bucket, 'tok1', id)).toBeNull();
	});

	it('id inconnu -> ok quand même (idempotent), index inchangé', async () => {
		const bucket = new FakeBucket();
		const result = await handleDelete(bucket, 'tok1', 'nope');
		expect(result).toEqual({ ok: true });
	});
});
