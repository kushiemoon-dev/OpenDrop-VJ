/**
 * obs-link-actions.ts — connect/disconnect, sense-1 (OpenDrop → OBS) watcher,
 * sense-2 (OBS → OpenDrop) handler, and the anti-echo guard preventing the two
 * from ping-ponging.
 *
 * OBS fires `CurrentProgramSceneChanged` for every scene change, including
 * ones OpenDrop itself just caused via `obsSetScene` — so every successful
 * sense-1 emission inevitably triggers sense-2 for the very same scene
 * shortly after. A one-shot suppression flag consumed by the sense-1 $effect
 * re-running doesn't work here: when the echo maps to a target OpenDrop is
 * already in (the common case), re-applying it is a same-value write to a
 * Svelte 5 $state field, which the reactive proxy doesn't mark dirty — so
 * the $effect never re-runs, the flag is never consumed, and it lingers to
 * wrongly swallow the next unrelated, real outbound change.
 *
 * Fixed with a direct scene-name comparison instead: `lastOutboundScene`
 * records the scene name OpenDrop itself last told OBS to switch to.
 * `isOwnEcho` (the one pure piece, exported and tested directly) checks the
 * incoming scene name against it synchronously, with no dependency on any
 * reactive effect re-running — it can't be starved by equality-gated
 * reactivity because it isn't reactive at all.
 */

import { obsLinkState } from './obs-link-store.svelte.js';
import { findSceneForTarget, findTargetForScene } from './obs-mapping.js';
import { frontSlotIndex, frontSlotMood } from './front-slot.js';

/** True when `sceneName` is the scene OpenDrop itself last set on OBS — i.e. this
 * incoming CurrentProgramSceneChanged is our own echo, not an externally-initiated change. */
export function isOwnEcho(sceneName: string, lastOutboundScene: string | null): boolean {
	return sceneName === lastOutboundScene;
}

let lastOutboundScene: string | null = null;
let lastFrontSlot = -1;
let lastMood: number | null = null;

/** Unsubscribe for the renderer-side onObsSceneChanged listener registered by
 * connectObs — torn down in disconnectObs so reconnecting never stacks listeners. */
let unsubObsSceneChanged: (() => void) | null = null;

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

	unsubObsSceneChanged =
		window.electronAPI?.onObsSceneChanged((sceneName) => {
			if (isOwnEcho(sceneName, lastOutboundScene)) return;
			const target = findTargetForScene(obsLinkState.mapping, sceneName);
			if (!target) return;
			applyIncomingTarget(target);
		}) ?? null;
}

export async function disconnectObs(): Promise<void> {
	unsubObsSceneChanged?.();
	unsubObsSceneChanged = null;
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

	const sceneBySlot = findSceneForTarget(obsLinkState.mapping, { type: 'slot', slot: front as 0 | 1 | 2 | 3 });
	const sceneByMood = mood ? findSceneForTarget(obsLinkState.mapping, { type: 'mood', colorIndex: mood as 1 | 2 | 3 | 4 | 5 }) : undefined;
	const scene = sceneByMood ?? sceneBySlot;
	if (scene) {
		lastOutboundScene = scene;
		await window.electronAPI?.obsSetScene(scene);
	}
}
