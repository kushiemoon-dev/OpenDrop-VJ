import { describe, it, expect } from 'vitest';
import { filterShareableOverlays, encodeSharedSet, decodeSharedSet, type SharedSet } from './share-set.js';
import { makeOverlay } from './overlay.js';
import { DEFAULT_COLOR_PARAMS, DEFAULT_SLOT_COMPOSITE } from './sync.js';
import { defaultTimeParams } from './time-params.js';
import { defaultBeatTriggerConfig } from './beat-trigger.js';

function fixtureSet(): SharedSet {
	return {
		version: 1,
		name: 'Mon set de test',
		presetA: 'preset-a-slug', presetB: 'preset-b-slug',
		deckBus: ['A', 'B', 'off', 'off'],
		crossfader: 0.3, transitionTime: 1.5,
		colorParamsA: { ...DEFAULT_COLOR_PARAMS, hueRotate: 0.2 }, colorParamsB: { ...DEFAULT_COLOR_PARAMS },
		slotComposites: [DEFAULT_SLOT_COMPOSITE, DEFAULT_SLOT_COMPOSITE, DEFAULT_SLOT_COMPOSITE, DEFAULT_SLOT_COMPOSITE],
		timeParams: [defaultTimeParams(), defaultTimeParams(), defaultTimeParams(), defaultTimeParams()],
		snapshots: [{ name: 'Slot 0', values: { 'color-hue-a': 0.5 } }, null, null, null, null, null, null, null],
		snapshotRecallDuration: 2,
		timelineKeyframes: [{ slot: 0, timeSec: 0 }, { slot: 0, timeSec: 5 }],
		overlays: [makeOverlay('Texte', { kind: 'text', text: 'Hello' }), makeOverlay('img.png')],
		beatTriggerA: defaultBeatTriggerConfig(), beatTriggerB: defaultBeatTriggerConfig(),
		beatSyncA: false, beatSyncB: true,
		overlayQueueEnabled: false, overlayQueueTrigger: defaultBeatTriggerConfig(),
	};
}

describe('filterShareableOverlays', () => {
	it('garde uniquement les overlays texte', () => {
		const text = makeOverlay('Texte', { kind: 'text', text: 'Hello' });
		const media = makeOverlay('img.png');
		expect(filterShareableOverlays([text, media])).toEqual([text]);
	});

	it('liste vide → liste vide', () => {
		expect(filterShareableOverlays([])).toEqual([]);
	});
});

describe('encodeSharedSet / decodeSharedSet round-trip', () => {
	it('un SharedSet complet survit encode -> decode intact', async () => {
		const set = fixtureSet();
		const encoded = await encodeSharedSet(set);
		const decoded = await decodeSharedSet(encoded);
		expect(decoded).toEqual(set);
	});

	it("l'encodage produit une chaîne url-safe (pas de +, /, =)", async () => {
		const encoded = await encodeSharedSet(fixtureSet());
		expect(encoded).not.toMatch(/[+/=]/);
	});

	it('chaîne corrompue -> null, ne jette jamais', async () => {
		await expect(decodeSharedSet('!!!pas-du-tout-du-base64-valide!!!')).resolves.toBeNull();
	});

	it('chaîne vide -> null', async () => {
		await expect(decodeSharedSet('')).resolves.toBeNull();
	});

	it('version inconnue -> null', async () => {
		const set = { ...fixtureSet(), version: 2 as unknown as 1 };
		const encoded = await encodeSharedSet(set);
		await expect(decodeSharedSet(encoded)).resolves.toBeNull();
	});
});
