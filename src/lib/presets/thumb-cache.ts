/**
 * IndexedDB cache for preset thumbnails (WebP 192x108).
 *
 * Unbounded growth is accepted (~16,374 presets x ~8KB ≈ 130MB) for a
 * desktop app. No LRU (YAGNI).
 */

// Browser guard — IndexedDB doesn't exist on the Node/SSR side
// (SPA mode: ssr=false, but module imports are still evaluated at build time)

const DB_NAME = 'opendrop-thumbs';
const STORE = 'thumbs';
const DB_VERSION = 1;

// In-memory memo: slug -> object URL (avoids recreating object URLs)
const _urlCache = new Map<string, string>();

function openDB(): Promise<IDBDatabase> {
	return new Promise((res, rej) => {
		const req = indexedDB.open(DB_NAME, DB_VERSION);
		req.onupgradeneeded = () => req.result.createObjectStore(STORE);
		req.onsuccess = () => res(req.result);
		req.onerror = () => rej(req.error);
	});
}

/** Read a WebP blob from IndexedDB. Null if missing or outside the browser. */
async function getThumbBlob(slug: string): Promise<Blob | null> {
	if (typeof indexedDB === 'undefined') return null;
	const db = await openDB();
	const result = await new Promise<Blob | null>((res, rej) => {
		const req = db.transaction(STORE, 'readonly').objectStore(STORE).get(slug);
		req.onsuccess = () => res((req.result as Blob | undefined) ?? null);
		req.onerror = () => rej(req.error);
	});
	db.close();
	return result;
}

/** Persist a WebP blob to IndexedDB. No-op outside the browser. */
export async function putThumbBlob(slug: string, blob: Blob): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	const db = await openDB();
	await new Promise<void>((res, rej) => {
		const tx = db.transaction(STORE, 'readwrite');
		const req = tx.objectStore(STORE).put(blob, slug);
		req.onsuccess = () => res();
		req.onerror = () => rej(req.error);
	});
	db.close();
}

/**
 * Get the object URL for a thumbnail (creates it from IndexedDB, memoizes it).
 * Returns null if the slug isn't cached.
 */
export async function getThumbUrl(slug: string): Promise<string | null> {
	if (_urlCache.has(slug)) return _urlCache.get(slug)!;
	const blob = await getThumbBlob(slug);
	if (!blob) return null;
	const url = URL.createObjectURL(blob);
	_urlCache.set(slug, url);
	return url;
}

/**
 * Memoize a URL immediately after generation (blob already available, no need to hit IDB).
 * Returns the created object URL.
 */
export function cacheUrl(slug: string, blob: Blob): string {
	if (_urlCache.has(slug)) return _urlCache.get(slug)!;
	const url = URL.createObjectURL(blob);
	_urlCache.set(slug, url);
	return url;
}
