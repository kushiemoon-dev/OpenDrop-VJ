/**
 * chat-poll-actions.ts — connects Twitch/Kick, runs one poll at a time, and
 * hands the winning option index to a caller-supplied callback (wired to
 * commands.ts at the +page.svelte call site, not here — this module has no
 * knowledge of decks/presets, only of "option N won").
 */

import { chatPollState } from './chat-poll-store.svelte.js';
import { createPoll, castVote, resolveWinner, parseVote, type PollState } from './chat-poll-engine.js';

export function namespacedVoterId(platform: 'twitch' | 'kick', userId: string): string {
	return `${platform}:${userId}`;
}

let currentPoll: PollState | null = null;
let resolveTimer: ReturnType<typeof setTimeout> | null = null;
let onResolved: ((winnerIndex: number | null) => void) | null = null;

export async function connectTwitch(channel: string): Promise<void> {
	chatPollState.twitch.error = '';
	const res = await window.electronAPI?.twitchConnect(channel);
	if (!res?.ok) { chatPollState.twitch.error = res?.error ?? 'Connexion Twitch impossible.'; return; }
	chatPollState.twitch.connected = true;
}

// In-flight guard, same shape as ndi-deck-actions.ts's `pendingSlots`: the main
// process's kick:connect handler computes its generation counter synchronously
// before login() resolves (see electron/main.cjs), so two concurrent
// kick:connect calls issued before the first resolves could both commit the
// same generation number — reopening kick-js's double-message-delivery bug
// (kick-js has no real disconnect; the generation counter is the only guard
// against a stale listener still forwarding messages). Set before the await,
// cleared in `finally`, so a second call arriving mid-flight is a no-op
// instead of racing the first call's IPC round-trip.
let kickConnectPending = false;

export async function connectKick(channel: string): Promise<void> {
	if (kickConnectPending) return;
	kickConnectPending = true;
	try {
		chatPollState.kick.error = '';
		const res = await window.electronAPI?.kickConnect(channel);
		if (!res?.ok) { chatPollState.kick.error = res?.error ?? 'Connexion Kick impossible.'; return; }
		chatPollState.kick.connected = true;
	} finally {
		kickConnectPending = false;
	}
}

/** Register once (in +page.svelte) — routes every incoming chat message into the active poll, if any. */
export function registerChatMessageHandler(): void {
	window.electronAPI?.onChatMessage((msg) => {
		if (!currentPoll) return;
		const optionIndex = parseVote(msg.content, currentPoll.optionCount);
		if (optionIndex === null) return;
		currentPoll = castVote(currentPoll, namespacedVoterId(msg.platform, msg.userId), optionIndex);
	});
}

export function startPoll(options: string[], durationSeconds: number, onDone: (winnerIndex: number | null) => void): void {
	currentPoll = createPoll(options.length);
	onResolved = onDone;
	chatPollState.poll = { status: 'running', options, secondsLeft: durationSeconds, winnerIndex: null };

	const tick = () => {
		if (!chatPollState.poll) return;
		chatPollState.poll.secondsLeft -= 1;
		if (chatPollState.poll.secondsLeft <= 0) {
			resolvePoll();
		} else {
			resolveTimer = setTimeout(tick, 1000);
		}
	};
	resolveTimer = setTimeout(tick, 1000);
}

function resolvePoll(): void {
	const winnerIndex = currentPoll ? resolveWinner(currentPoll) : null;
	if (chatPollState.poll) {
		chatPollState.poll.status = 'resolved';
		chatPollState.poll.winnerIndex = winnerIndex;
	}
	onResolved?.(winnerIndex);
	onResolved = null;
	currentPoll = null;
}

export function cancelPoll(): void {
	if (resolveTimer) clearTimeout(resolveTimer);
	resolveTimer = null;
	currentPoll = null;
	onResolved = null;
	chatPollState.poll = null;
}
