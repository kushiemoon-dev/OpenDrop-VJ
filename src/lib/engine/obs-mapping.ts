/** obs-mapping.ts — pure OBS scene ⇄ {slot|mood} lookup table. No I/O, no state. */

export type MappingTarget =
	| { type: 'slot'; slot: 0 | 1 | 2 | 3 }
	| { type: 'mood'; colorIndex: 1 | 2 | 3 | 4 | 5 };

export interface MappingEntry {
	sceneName: string;
	target: MappingTarget;
}

function targetsEqual(a: MappingTarget, b: MappingTarget): boolean {
	if (a.type !== b.type) return false;
	if (a.type === 'slot' && b.type === 'slot') return a.slot === b.slot;
	if (a.type === 'mood' && b.type === 'mood') return a.colorIndex === b.colorIndex;
	return false;
}

export function findSceneForTarget(mapping: MappingEntry[], target: MappingTarget): string | undefined {
	return mapping.find((entry) => targetsEqual(entry.target, target))?.sceneName;
}

export function findTargetForScene(mapping: MappingEntry[], sceneName: string): MappingTarget | undefined {
	return mapping.find((entry) => entry.sceneName === sceneName)?.target;
}
