// workers/presets-cloud/src/handlers.ts
export interface Bucket {
	get(key: string): Promise<{ text(): Promise<string> } | null>;
	put(key: string, value: string): Promise<unknown>;
	delete(key: string): Promise<void>;
}

export interface IndexEntry {
	id: string;
	name: string;
	sizeBytes: number;
	uploadedAt: number;
}

const MAX_PRESETS = 300;
const MAX_TOTAL_BYTES = 30 * 1024 * 1024;

function indexKey(token: string): string {
	return `presets/${token}/index.json`;
}

function presetKey(token: string, id: string): string {
	return `presets/${token}/${id}.json`;
}

export async function readIndex(bucket: Bucket, token: string): Promise<IndexEntry[]> {
	const obj = await bucket.get(indexKey(token));
	if (!obj) return [];
	try {
		const parsed = JSON.parse(await obj.text());
		return Array.isArray(parsed) ? parsed : [];
	} catch {
		return [];
	}
}

export async function handleUpload(
	bucket: Bucket, token: string, name: string, data: unknown
): Promise<{ id: string } | { error: string; status: number }> {
	if (!token) return { error: 'missing token', status: 400 };
	if (!name) return { error: 'missing name', status: 400 };
	if (!data || typeof data !== 'object' || Array.isArray(data)) return { error: 'invalid preset data', status: 400 };

	const serialized = JSON.stringify(data);
	const sizeBytes = new TextEncoder().encode(serialized).length;
	const index = await readIndex(bucket, token);

	if (index.length >= MAX_PRESETS) return { error: `quota exceeded: max ${MAX_PRESETS} presets`, status: 413 };
	const totalBytes = index.reduce((sum, e) => sum + e.sizeBytes, 0) + sizeBytes;
	if (totalBytes > MAX_TOTAL_BYTES) return { error: `quota exceeded: max ${MAX_TOTAL_BYTES} bytes`, status: 413 };

	const id = crypto.randomUUID();
	await bucket.put(presetKey(token, id), serialized);
	const nextIndex = [...index, { id, name, sizeBytes, uploadedAt: Date.now() }];
	await bucket.put(indexKey(token), JSON.stringify(nextIndex));
	return { id };
}

export async function handleGetPreset(bucket: Bucket, token: string, id: string): Promise<object | null> {
	if (!token || !id) return null;
	const obj = await bucket.get(presetKey(token, id));
	if (!obj) return null;
	try {
		return JSON.parse(await obj.text());
	} catch {
		return null;
	}
}

export async function handleRename(
	bucket: Bucket, token: string, id: string, name: string
): Promise<{ ok: true } | { error: string; status: number }> {
	if (!token || !id || !name) return { error: 'missing field', status: 400 };
	const index = await readIndex(bucket, token);
	const entry = index.find((e) => e.id === id);
	if (!entry) return { error: 'not found', status: 404 };
	entry.name = name;
	await bucket.put(indexKey(token), JSON.stringify(index));
	return { ok: true };
}

export async function handleDelete(
	bucket: Bucket, token: string, id: string
): Promise<{ ok: true } | { error: string; status: number }> {
	if (!token || !id) return { error: 'missing field', status: 400 };
	const index = await readIndex(bucket, token);
	const next = index.filter((e) => e.id !== id);
	await bucket.delete(presetKey(token, id));
	await bucket.put(indexKey(token), JSON.stringify(next));
	return { ok: true };
}
