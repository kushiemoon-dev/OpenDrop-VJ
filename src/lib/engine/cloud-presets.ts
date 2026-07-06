/**
 * Presets cloud (Track 2) — bibliothèque personnelle privée de presets custom,
 * stockée sur un Worker Cloudflare + R2 séparé (workers/presets-cloud/). Identité
 * = un token anonyme par appareil, pas de compte. Le préfixe CLOUD_PRESET_PREFIX
 * est l'unique garde-fou anti-collision avec les ~16k noms de presets statiques —
 * ne jamais dupliquer cette logique de préfixage ailleurs.
 */

import { PUBLIC_CLOUD_PRESETS_API } from '$env/static/public';

export interface CloudPresetEntry {
	id: string;
	name: string;
	sizeBytes: number;
	uploadedAt: number;
}

export const CLOUD_PRESET_PREFIX = '☁ ';

const TOKEN_KEY = 'od-cloud-token';

export function getOrCreateCloudToken(): string {
	let token = localStorage.getItem(TOKEN_KEY);
	if (!token) {
		token = crypto.randomUUID();
		localStorage.setItem(TOKEN_KEY, token);
	}
	return token;
}

export function setCloudToken(token: string): void {
	localStorage.setItem(TOKEN_KEY, token);
}

/** JSON.parse défensif — le fichier uploadé doit déjà être au format Butterchurn. */
export function parsePresetFile(jsonText: string): object {
	const parsed = JSON.parse(jsonText);
	if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
		throw new Error('Fichier preset invalide : doit être un objet JSON');
	}
	return parsed;
}

export async function getCloudPresetIndex(token: string): Promise<CloudPresetEntry[]> {
	if (!PUBLIC_CLOUD_PRESETS_API || !token) return [];
	try {
		const res = await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets?token=${encodeURIComponent(token)}`);
		if (!res.ok) return [];
		const data = await res.json();
		return Array.isArray(data) ? data : [];
	} catch {
		return [];
	}
}

export async function uploadPreset(
	token: string, name: string, presetData: object
): Promise<{ id: string } | { error: string }> {
	if (!PUBLIC_CLOUD_PRESETS_API) return { error: 'Presets cloud non configuré' };
	try {
		const res = await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ token, name: `${CLOUD_PRESET_PREFIX}${name}`, data: presetData }),
		});
		const body = await res.json();
		if (!res.ok) return { error: body?.error ?? `Erreur ${res.status}` };
		return body as { id: string };
	} catch {
		return { error: 'Erreur réseau' };
	}
}

export async function loadCloudPresetData(token: string, name: string): Promise<object | null> {
	const index = await getCloudPresetIndex(token);
	const entry = index.find((e) => e.name === name);
	if (!entry) return null;
	try {
		const res = await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets/${entry.id}?token=${encodeURIComponent(token)}`);
		if (!res.ok) return null;
		return await res.json();
	} catch {
		return null;
	}
}

export async function deleteCloudPreset(token: string, id: string): Promise<void> {
	if (!PUBLIC_CLOUD_PRESETS_API) return;
	try {
		await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets/${id}?token=${encodeURIComponent(token)}`, { method: 'DELETE' });
	} catch {
		/* best-effort */
	}
}

export async function renameCloudPreset(token: string, id: string, name: string): Promise<void> {
	if (!PUBLIC_CLOUD_PRESETS_API) return;
	try {
		await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets/${id}?token=${encodeURIComponent(token)}`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: `${CLOUD_PRESET_PREFIX}${name}` }),
		});
	} catch {
		/* best-effort */
	}
}
