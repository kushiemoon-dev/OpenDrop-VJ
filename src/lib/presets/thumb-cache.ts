/**
 * Cache IndexedDB pour les miniatures de presets (WebP 192×108).
 *
 * Croissance non bornée assumée (~16 374 presets × ~8 Ko ≈ 130 Mo) pour une
 * app desktop. Pas de LRU (YAGNI). Utiliser clearThumbs() pour forcer un rebuild.
 */

// Garde browser — IndexedDB n'existe pas côté Node/SSR
// (SPA mode : ssr=false, mais les imports de modules sont évalués au build)

const DB_NAME = 'opendrop-thumbs';
const STORE = 'thumbs';
const DB_VERSION = 1;

// Mémo en mémoire : slug → object-URL (évite de recréer des object-URLs)
const _urlCache = new Map<string, string>();

function openDB(): Promise<IDBDatabase> {
	return new Promise((res, rej) => {
		const req = indexedDB.open(DB_NAME, DB_VERSION);
		req.onupgradeneeded = () => req.result.createObjectStore(STORE);
		req.onsuccess = () => res(req.result);
		req.onerror = () => rej(req.error);
	});
}

/** Lire un blob WebP depuis IndexedDB. Null si absent ou hors browser. */
export async function getThumbBlob(slug: string): Promise<Blob | null> {
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

/** Persister un blob WebP dans IndexedDB. No-op hors browser. */
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

/** Vider tout le store (permet un rebuild complet du cache). */
export async function clearThumbs(): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	const db = await openDB();
	await new Promise<void>((res, rej) => {
		const tx = db.transaction(STORE, 'readwrite');
		const req = tx.objectStore(STORE).clear();
		req.onsuccess = () => res();
		req.onerror = () => rej(req.error);
	});
	db.close();
}

/**
 * Récupérer l'object-URL d'une miniature (crée depuis IndexedDB, mémoïse).
 * Retourne null si le slug n'est pas en cache.
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
 * Mémoïser une URL immédiatement après génération (blob déjà disponible, pas besoin d'IDB).
 * Retourne l'object-URL créé.
 */
export function cacheUrl(slug: string, blob: Blob): string {
	if (_urlCache.has(slug)) return _urlCache.get(slug)!;
	const url = URL.createObjectURL(blob);
	_urlCache.set(slug, url);
	return url;
}
