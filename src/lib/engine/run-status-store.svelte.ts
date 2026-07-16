/**
 * run-status-store.svelte.ts — reactive wrapper around the visualizer's
 * idle/running/error state machine and its two error messages (fatal start
 * failure vs. audio-source connection failure). Extracted from +page.svelte,
 * same shape as color-store.svelte.ts — plain $state, mutated directly by
 * the guard checks and connection functions that stay in +page.svelte.
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

export const runStatusState = $state({
	status: 'idle' as 'idle' | 'running' | 'error',
	errorMsg: '',
	sourceError: '',
});
