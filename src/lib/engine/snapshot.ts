/**
 * Snapshot — capture/recall of a subset of live parameter state with smooth
 * interpolation. Same split as compositor.ts: pure, unit-testable functions
 * first, a thin RAF-owning class after (not unit tested, verified in a real
 * browser — same precedent as Compositor).
 */

import type { CommandId } from './commands.js';

export interface Snapshot {
	name: string;
	values: Partial<Record<CommandId, number>>;
}

/** Ease-in-out (smoothstep). t is clamped to [0,1]. */
export function smoothstep(t: number): number {
	const x = t < 0 ? 0 : t > 1 ? 1 : t;
	return x * x * (3 - 2 * x);
}

/**
 * Lerp each key of `target` from its value in `start` toward `target`, at the
 * (already-eased) progress `progress01` in [0,1].
 *
 * Edge cases (decided):
 *  - A key present in `target` but missing from `start` starts from the
 *    target value itself (`from = to`) — never invent an unknown start, so a
 *    recall can't produce a wild jump on a value that wasn't captured live.
 *  - A key present in `start` but missing from `target` is skipped entirely
 *    (the loop runs over `target` only). This is the data-level expression of
 *    the crossfader-exclusion decision: a recall only ever drives what it
 *    captured.
 */
export function interpolateSnapshot(
	start: Partial<Record<CommandId, number>>,
	target: Partial<Record<CommandId, number>>,
	progress01: number,
): Partial<Record<CommandId, number>> {
	const out: Partial<Record<CommandId, number>> = {};
	for (const key in target) {
		const id = key as CommandId;
		const to = target[id]!;
		const from = start[id] ?? to;
		out[id] = from + (to - from) * progress01;
	}
	return out;
}

/**
 * Owns a requestAnimationFrame loop that drives one snapshot recall. Holds no
 * start/target/startTime as instance state — each recall() captures them in
 * its own frame closure, so a restart is fully independent of the animation
 * it replaces.
 */
export class SnapshotEngine {
	private rafId: number | null = null;

	recall(
		start: Partial<Record<CommandId, number>>,
		target: Partial<Record<CommandId, number>>,
		durationMs: number,
		onTick: (values: Partial<Record<CommandId, number>>) => void,
	): void {
		this.cancel(); // clean restart: kill any in-flight animation first

		if (durationMs <= 0) {
			onTick(interpolateSnapshot(start, target, 1)); // instant jump to target
			return;
		}

		const startTime = performance.now();
		const frame = (now: number) => {
			const raw = (now - startTime) / durationMs;
			if (raw >= 1) {
				this.rafId = null;
				onTick(interpolateSnapshot(start, target, 1)); // exact target, no float drift
				return;
			}
			onTick(interpolateSnapshot(start, target, smoothstep(raw)));
			this.rafId = requestAnimationFrame(frame);
		};
		this.rafId = requestAnimationFrame(frame);
	}

	cancel(): void {
		if (this.rafId !== null) {
			cancelAnimationFrame(this.rafId);
			this.rafId = null;
		}
	}
}
