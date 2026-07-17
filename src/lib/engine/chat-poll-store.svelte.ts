/** chat-poll-store.svelte.ts — Twitch/Kick connection state + current poll lifecycle. */

export const chatPollState = $state({
	twitch: { connected: false, error: '' },
	kick: { connected: false, error: '' },
	poll: null as { status: 'running' | 'resolved'; options: string[]; secondsLeft: number; winnerIndex: number | null } | null,
});
