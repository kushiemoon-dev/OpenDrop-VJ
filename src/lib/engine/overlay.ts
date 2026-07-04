export interface Overlay {
	id: string;
	name: string;
	x: number;           // centre X normalisé 0-1
	y: number;           // centre Y normalisé 0-1
	scale: number;       // 1 = taille originale
	rotation: number;    // degrés
	opacity: number;     // 0-1
	blendMode: string;   // CSS mix-blend-mode
	beatReactive: boolean;
	beatScale: number;   // multiplicateur d'échelle sur le beat (ex: 1.2)
	video: boolean;      // true = asset vidéo (rendu <video> au lieu de <img>) — ignoré si kind='text'
	spin: number;        // deg/s, 0 = pas de rotation continue
	driftX: number;      // fraction largeur/s, dérive horizontale
	driftY: number;      // fraction hauteur/s, dérive verticale
	kind: 'media' | 'text';        // 'media' = image/vidéo (défaut), 'text' = overlay texte
	text: string;                  // contenu du texte (vide pour kind='media')
	fontFamily: 'sans' | 'serif' | 'mono' | 'impact' | 'comic';
	fontSize: number;              // vh — indépendant de la résolution, multiplié par `scale`
	color: string;                 // couleur du texte, hex
	inQueue: boolean;              // fait partie de la rotation auto-cyclante (queue overlay)
}

export function makeOverlay(name: string, partial: Partial<Overlay> = {}): Overlay {
	return {
		id: crypto.randomUUID(),
		name,
		x: 0.5,
		y: 0.5,
		scale: 1,
		rotation: 0,
		opacity: 1,
		blendMode: 'screen',
		beatReactive: false,
		beatScale: 1.25,
		video: false,
		spin: 0,
		driftX: 0,
		driftY: 0,
		kind: 'media',
		text: '',
		fontFamily: 'sans',
		fontSize: 8,
		color: '#ffffff',
		inQueue: false,
		...partial
	};
}

// ── IndexedDB — stockage des images (data URL) ────────────────────────────

const DB_NAME = 'opendrop-overlays';
const STORE = 'assets';
const DB_VERSION = 1;

function openDB(): Promise<IDBDatabase> {
	return new Promise((res, rej) => {
		const req = indexedDB.open(DB_NAME, DB_VERSION);
		req.onupgradeneeded = () => req.result.createObjectStore(STORE);
		req.onsuccess = () => res(req.result);
		req.onerror = () => rej(req.error);
	});
}

export async function saveAsset(id: string, dataUrl: string): Promise<void> {
	const db = await openDB();
	await new Promise<void>((res, rej) => {
		const tx = db.transaction(STORE, 'readwrite');
		const req = tx.objectStore(STORE).put(dataUrl, id);
		req.onsuccess = () => res();
		req.onerror = () => rej(req.error);
	});
	db.close();
}

export async function loadAsset(id: string): Promise<string | null> {
	const db = await openDB();
	const result = await new Promise<string | null>((res, rej) => {
		const req = db.transaction(STORE, 'readonly').objectStore(STORE).get(id);
		req.onsuccess = () => res((req.result as string | undefined) ?? null);
		req.onerror = () => rej(req.error);
	});
	db.close();
	return result;
}

export async function deleteAsset(id: string): Promise<void> {
	const db = await openDB();
	await new Promise<void>((res, rej) => {
		const tx = db.transaction(STORE, 'readwrite');
		const req = tx.objectStore(STORE).delete(id);
		req.onsuccess = () => res();
		req.onerror = () => rej(req.error);
	});
	db.close();
}
