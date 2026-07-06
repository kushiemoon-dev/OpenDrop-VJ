/**
 * Q-var live editing (Track 2) — generic q1-q32 overrides per deck, matching
 * NestDrop's "Q Var" knobs. Presets define/use these internally with no
 * universal meaning (unlike Time's documented speed/zoom/etc — see
 * time-params.ts), so there is no neutral numeric default. The override is
 * opt-in per slot via `enabled`, checked live every frame rather than baked
 * into which lines get compiled — so toggling a q-var on/off never requires
 * reloading the preset (same injection technique as time-params.ts, applied
 * to a boolean gate instead of a x1 multiplier).
 */

export interface DeckQVarParams {
	enabled: boolean[]; // length 32, index 0 = q1
	value: number[];    // length 32, bounded [-2, 2] by the UI/command layer
}

export function defaultQVarParams(): DeckQVarParams {
	return { enabled: new Array(32).fill(false), value: new Array(32).fill(0) };
}

/**
 * Shallow-clones `preset` and appends 32 guard lines to its frame_eqs_str,
 * each referencing window.__odQVarParams[slot] — pure string manipulation,
 * no window access here, unit-testable without a DOM. All 32 lines are
 * always compiled; the `enabled` check is evaluated at runtime by the
 * compiled preset code every frame, not at compile time — this is what lets
 * enabling/disabling a q-var take effect without a second loadPreset() call.
 */
export function injectQVarParams(preset: object, slot: number): object {
	const patched: Record<string, unknown> = { ...preset };
	const original = typeof patched.frame_eqs_str === 'string' ? patched.frame_eqs_str : '';
	const p = `window.__odQVarParams[${slot}]`;
	const guards = Array.from({ length: 32 }, (_, i) =>
		`if (${p}.enabled[${i}]) { q${i + 1} = ${p}.value[${i}]; }`
	).join('\n');
	patched.frame_eqs_str = `${original}\n${guards}`;
	return patched;
}

/**
 * Lazily initializes and returns the global per-slot params array Butterchurn's
 * compiled preset code reads from. Not unit tested (touches `window`) —
 * verified in a real browser, same precedent as Compositor/SnapshotEngine/
 * time-params.ts.
 */
export function getGlobalQVarParams(): DeckQVarParams[] {
	const w = window as unknown as { __odQVarParams?: DeckQVarParams[] };
	if (!w.__odQVarParams) {
		w.__odQVarParams = [defaultQVarParams(), defaultQVarParams(), defaultQVarParams(), defaultQVarParams()];
	}
	return w.__odQVarParams;
}

// Eager init at module load, in whichever window (control or output) imports this
// module — both do, transitively, via deck-manager.ts. Ensures the compiled preset
// code's window.__odQVarParams[slot] reference is never undefined.
if (typeof window !== 'undefined') getGlobalQVarParams();
