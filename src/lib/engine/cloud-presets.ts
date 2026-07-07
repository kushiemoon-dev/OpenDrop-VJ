/**
 * Cloud presets (Track 2) — private personal library of custom presets,
 * stored on a separate Cloudflare Worker + R2 (workers/presets-cloud/). Identity
 * = one anonymous token per device, no account. The CLOUD_PRESET_PREFIX
 * is the sole anti-collision guard against the ~16k static preset names —
 * never duplicate this prefixing logic elsewhere.
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

/** Defensive JSON.parse — the uploaded file must already be in Butterchurn format. */
export function parsePresetFile(jsonText: string): object {
	const parsed = JSON.parse(jsonText);
	if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
		throw new Error('Invalid preset file: must be a JSON object');
	}
	return parsed;
}

// Token always travels in this header, never a query string or request body — a query
// string would land in access logs, browser history, and any Referer header.
function tokenHeaders(token: string, extra?: Record<string, string>): Record<string, string> {
	return { 'X-Cloud-Token': token, ...extra };
}

export async function getCloudPresetIndex(token: string): Promise<CloudPresetEntry[]> {
	if (!PUBLIC_CLOUD_PRESETS_API || !token) return [];
	try {
		const res = await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets`, { headers: tokenHeaders(token) });
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
	if (!PUBLIC_CLOUD_PRESETS_API) return { error: 'Cloud presets not configured' };
	try {
		const res = await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets`, {
			method: 'POST',
			headers: tokenHeaders(token, { 'Content-Type': 'application/json' }),
			body: JSON.stringify({ name: `${CLOUD_PRESET_PREFIX}${name}`, data: presetData }),
		});
		const body = await res.json();
		if (!res.ok) return { error: body?.error ?? `Error ${res.status}` };
		return body as { id: string };
	} catch {
		return { error: 'Network error' };
	}
}

export async function loadCloudPresetData(token: string, name: string): Promise<object | null> {
	const index = await getCloudPresetIndex(token);
	const entry = index.find((e) => e.name === name);
	if (!entry) return null;
	try {
		const res = await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets/${entry.id}`, { headers: tokenHeaders(token) });
		if (!res.ok) return null;
		return await res.json();
	} catch {
		return null;
	}
}

export async function deleteCloudPreset(token: string, id: string): Promise<void> {
	if (!PUBLIC_CLOUD_PRESETS_API) return;
	try {
		await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets/${id}`, { method: 'DELETE', headers: tokenHeaders(token) });
	} catch {
		/* best-effort */
	}
}

export async function renameCloudPreset(token: string, id: string, name: string): Promise<void> {
	if (!PUBLIC_CLOUD_PRESETS_API) return;
	try {
		await fetch(`${PUBLIC_CLOUD_PRESETS_API}/presets/${id}`, {
			method: 'PATCH',
			headers: tokenHeaders(token, { 'Content-Type': 'application/json' }),
			body: JSON.stringify({ name: `${CLOUD_PRESET_PREFIX}${name}` }),
		});
	} catch {
		/* best-effort */
	}
}
