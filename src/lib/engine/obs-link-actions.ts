/**
 * obs-link-actions.ts — connect/disconnect, sense-1 (OpenDrop → OBS) watcher,
 * sense-2 (OBS → OpenDrop) handler, and the anti-echo guard preventing the two
 * from ping-ponging. The guard is the one pure piece, exported and tested
 * directly; connect/disconnect/watch are thin orchestration over
 * window.electronAPI, same untested-boundary precedent as Task 1-2-5.
 */

import { obsLinkState } from './obs-link-store.svelte.js';
import { findSceneForTarget, findTargetForScene } from './obs-mapping.js';
import { frontSlotIndex, frontSlotMood } from './front-slot.js';

/**
 * One-shot suppression flag: markIncoming() is called right before applying a
 * scene change that came FROM OBS; the very next shouldSuppressOutbound() call
 * (made by the sense-1 watcher reacting to that same state change) reads true
 * exactly once, then resets — so the watcher skips re-emitting SetCurrentProgramScene
 * for a change that OBS itself just caused, without suppressing any later,
 * independently-caused change.
 */
export function createAntiEchoGuard() {
	let suppressNext = false;
	return {
		shouldSuppressOutbound(): boolean {
			const value = suppressNext;
			suppressNext = false;
			return value;
		},
		markIncoming(): void {
			suppressNext = true;
		},
	};
}

const guard = createAntiEchoGuard();
let lastFrontSlot = -1;
let lastMood: number | null = null;

export async function connectObs(host: string, port: number): Promise<void> {
	obsLinkState.error = '';
	const res = await window.electronAPI?.obsConnect(host, port);
	if (!res?.ok) {
		obsLinkState.error = res?.error ?? 'Connexion OBS impossible.';
		return;
	}
	obsLinkState.connected = true;
	obsLinkState.host = host;
	obsLinkState.port = port;
	const scenesRes = await window.electronAPI?.obsGetScenes();
	if (scenesRes?.ok) obsLinkState.scenes = scenesRes.scenes ?? [];

	window.electronAPI?.onObsSceneChanged((sceneName) => {
		const target = findTargetForScene(obsLinkState.mapping, sceneName);
		if (!target) return;
		guard.markIncoming();
		applyIncomingTarget(target);
	});
}

export async function disconnectObs(): Promise<void> {
	await window.electronAPI?.obsDisconnect();
	obsLinkState.connected = false;
}

/**
 * Sense 2 (per the approved design, now fully pinned down — see the research
 * note below, not left to the integrating task to improvise):
 *
 * - A slot-mapped scene assigns that slot to whichever bus currently has the
 *   dominant crossfader gain (crossfader < 0.5 → bus 'A' is dominant, else
 *   'B'), via `bringSlotToFront` in +page.svelte (Step 6) — this only touches
 *   the targeted slot's `deckBus` entry, no other slot's assignment or the
 *   crossfader position changes, so it can't clobber unrelated compositing.
 * - A mood-mapped scene loads a preset tagged with that mood onto the current
 *   front slot, via `selectPreset` (`src/lib/engine/deck-preset-actions.ts`).
 *   That function currently reads `deckState.activeSlot` internally rather
 *   than taking a slot parameter — Task 8 Step 6 refactors it to accept an
 *   explicit `slot: number` argument (its one existing call site in
 *   +page.svelte passes `deckState.activeSlot` explicitly, so existing
 *   behavior is unchanged). Task 12 reuses this same parameterized
 *   `selectPreset` for the chat-poll winner — do not add a second
 *   preset-loading path.
 */
function applyIncomingTarget(target: ReturnType<typeof findTargetForScene>): void {
	if (!target) return;
	obsIncomingTargetHandlers.forEach((cb) => cb(target));
}

type IncomingTargetHandler = (target: NonNullable<ReturnType<typeof findTargetForScene>>) => void;
const obsIncomingTargetHandlers: IncomingTargetHandler[] = [];
export function onIncomingObsTarget(cb: IncomingTargetHandler): void {
	obsIncomingTargetHandlers.push(cb);
}

/** Call reactively (e.g. from an $effect in +page.svelte) whenever opacities/presets4 change. */
export async function watchFrontSlotForObs(
	opacities: number[],
	presets4: string[],
	favColors: Record<string, number>,
): Promise<void> {
	if (!obsLinkState.connected) return;

	const front = frontSlotIndex(opacities);
	const mood = frontSlotMood(favColors, presets4, front);
	if (front === lastFrontSlot && mood === lastMood) return;
	lastFrontSlot = front;
	lastMood = mood;

	if (guard.shouldSuppressOutbound()) return;

	const sceneBySlot = findSceneForTarget(obsLinkState.mapping, { type: 'slot', slot: front as 0 | 1 | 2 | 3 });
	const sceneByMood = mood ? findSceneForTarget(obsLinkState.mapping, { type: 'mood', colorIndex: mood as 1 | 2 | 3 | 4 | 5 }) : undefined;
	const scene = sceneByMood ?? sceneBySlot;
	if (scene) await window.electronAPI?.obsSetScene(scene);
}
