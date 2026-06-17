export type ClipRef =
	| { kind: 'builtin'; src: string }
	| { kind: 'user'; id: string };

export interface VideoClipMeta {
	ref: ClipRef;
	name: string;
}

const DB_NAME = 'opendrop-videos';
const STORE = 'clips';
const DB_VERSION = 1;

function openDB(): Promise<IDBDatabase> {
	return new Promise((res, rej) => {
		const req = indexedDB.open(DB_NAME, DB_VERSION);
		req.onupgradeneeded = () => req.result.createObjectStore(STORE);
		req.onsuccess = () => res(req.result);
		req.onerror = () => rej(req.error);
	});
}

export async function saveVideo(id: string, blob: Blob): Promise<void> {
	const db = await openDB();
	await new Promise<void>((res, rej) => {
		const tx = db.transaction(STORE, 'readwrite');
		const req = tx.objectStore(STORE).put(blob, id);
		req.onsuccess = () => res();
		req.onerror = () => rej(req.error);
	});
	db.close();
}

export async function loadVideo(id: string): Promise<Blob | null> {
	const db = await openDB();
	const result = await new Promise<Blob | null>((res, rej) => {
		const req = db.transaction(STORE, 'readonly').objectStore(STORE).get(id);
		req.onsuccess = () => res((req.result as Blob | undefined) ?? null);
		req.onerror = () => rej(req.error);
	});
	db.close();
	return result;
}

export async function deleteVideo(id: string): Promise<void> {
	const db = await openDB();
	await new Promise<void>((res, rej) => {
		const tx = db.transaction(STORE, 'readwrite');
		const req = tx.objectStore(STORE).delete(id);
		req.onsuccess = () => res();
		req.onerror = () => rej(req.error);
	});
	db.close();
}
