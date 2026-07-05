/**
 * Share-set — encodes a curated, machine-agnostic subset of the app's visual
 * state into a URL-safe string (gzip + base64url), and decodes it back. Pure
 * functions only — no DOM/location/clipboard access here, that lives in
 * +page.svelte.
 */

import type { ColorParams, SlotComposite } from './sync.js';
import type { DeckTimeParams } from './time-params.js';
import type { Snapshot } from './snapshot.js';
import type { TimelineKeyframe } from './timeline.js';
import type { Overlay } from './overlay.js';
import type { BeatTriggerConfig } from './beat-trigger.js';

export interface SharedSet {
	version: 1;
	name: string;
	presetA: string;
	presetB: string;
	deckBus: Array<'A' | 'B' | 'off'>;
	crossfader: number;
	transitionTime: number;
	colorParamsA: ColorParams;
	colorParamsB: ColorParams;
	slotComposites: [SlotComposite, SlotComposite, SlotComposite, SlotComposite];
	timeParams: DeckTimeParams[];
	snapshots: (Snapshot | null)[];
	snapshotRecallDuration: number;
	timelineKeyframes: TimelineKeyframe[];
	overlays: Overlay[];
	beatTriggerA: BeatTriggerConfig;
	beatTriggerB: BeatTriggerConfig;
	beatSyncA: boolean;
	beatSyncB: boolean;
	overlayQueueEnabled: boolean;
	overlayQueueTrigger: BeatTriggerConfig;
}

/** Overlays referencing a local IndexedDB asset (image/video) can never fit in a URL. */
export function filterShareableOverlays(overlays: Overlay[]): Overlay[] {
	return overlays.filter((o) => o.kind === 'text');
}

function bytesToBase64Url(bytes: Uint8Array): string {
	let binary = '';
	for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
	return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function base64UrlToBytes(b64url: string): Uint8Array<ArrayBuffer> {
	const b64 = b64url.replace(/-/g, '+').replace(/_/g, '/');
	const binary = atob(b64);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
	return bytes;
}

export async function encodeSharedSet(set: SharedSet): Promise<string> {
	const input = new TextEncoder().encode(JSON.stringify(set));
	// pipeThrough (not a manual writer.write()/close() + separate read) — the manual sequence
	// deadlocks under real browser backpressure: write()/close() wait for the transform's
	// internal queue to drain, but nothing reads cs.readable until after they've already
	// resolved. pipeThrough pumps both sides concurrently, which is exactly what avoids that.
	const stream = new Blob([input]).stream().pipeThrough(new CompressionStream('gzip'));
	const compressed = new Uint8Array(await new Response(stream).arrayBuffer());
	return bytesToBase64Url(compressed);
}

export async function decodeSharedSet(encoded: string): Promise<SharedSet | null> {
	try {
		if (!encoded) return null;
		const bytes = base64UrlToBytes(encoded);
		const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
		const decompressed = await new Response(stream).arrayBuffer();
		const parsed = JSON.parse(new TextDecoder().decode(decompressed));
		if (parsed?.version !== 1) return null;
		return parsed as SharedSet;
	} catch {
		return null;
	}
}
