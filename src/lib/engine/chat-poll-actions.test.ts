import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { namespacedVoterId, startPoll, cancelPoll } from './chat-poll-actions.js';
import { chatPollState } from './chat-poll-store.svelte.js';

describe('namespacedVoterId', () => {
	it('prefixes the platform so the same user-id on two platforms cannot double-vote as if merged, nor collide', () => {
		expect(namespacedVoterId('twitch', '123')).toBe('twitch:123');
		expect(namespacedVoterId('kick', '123')).toBe('kick:123');
		expect(namespacedVoterId('twitch', '123')).not.toBe(namespacedVoterId('kick', '123'));
	});
});

// startPoll/cancelPoll touch no browser boundary (no window.electronAPI) — unlike
// connectTwitch/connectKick/registerChatMessageHandler, which this suite's
// `environment: 'node'` vitest config can't exercise, these are fully testable here.
describe('startPoll', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		cancelPoll();
	});

	afterEach(() => {
		cancelPoll();
		vi.useRealTimers();
	});

	it('starts a poll with a zeroed tally and resolves it with no votes after the duration elapses', () => {
		const onDone = vi.fn();
		startPoll(['A', 'B'], 5, onDone);
		expect(chatPollState.poll?.status).toBe('running');
		expect(chatPollState.poll?.tally).toEqual([0, 0]);

		vi.advanceTimersByTime(5000);

		expect(onDone).toHaveBeenCalledTimes(1);
		expect(onDone).toHaveBeenCalledWith(null);
		expect(chatPollState.poll?.status).toBe('resolved');
		expect(chatPollState.poll?.winnerIndex).toBeNull();
	});

	// Regression test for the Task 12 review carryover: a second startPoll() call
	// while one is already running used to overwrite state and spawn a second,
	// independent tick() chain, which later reset the already-resolved winnerIndex
	// back to null (see this file's header comment and startPoll's own comment).
	it('ignores a second startPoll call while one is already running', () => {
		const onDoneFirst = vi.fn();
		const onDoneSecond = vi.fn();
		startPoll(['A', 'B'], 5, onDoneFirst);
		startPoll(['X', 'Y', 'Z'], 5, onDoneSecond);

		// Second call was a no-op — state still reflects the first poll only.
		expect(chatPollState.poll?.options).toEqual(['A', 'B']);

		vi.advanceTimersByTime(5000);

		expect(onDoneFirst).toHaveBeenCalledTimes(1);
		expect(onDoneSecond).not.toHaveBeenCalled();
		expect(chatPollState.poll?.status).toBe('resolved');

		// No phantom second tick chain left running — advancing further (but still
		// within the auto-dismiss window, see the dismiss test below) must not call
		// either callback again or corrupt the resolved state.
		vi.advanceTimersByTime(3000);
		expect(onDoneFirst).toHaveBeenCalledTimes(1);
		expect(onDoneSecond).not.toHaveBeenCalled();
		expect(chatPollState.poll?.status).toBe('resolved');
	});

	it('auto-dismisses the resolved poll HUD after the dismiss delay', () => {
		startPoll(['A', 'B'], 5, vi.fn());
		vi.advanceTimersByTime(5000);
		expect(chatPollState.poll?.status).toBe('resolved');

		vi.advanceTimersByTime(8000);
		expect(chatPollState.poll).toBeNull();
	});

	it('a new poll started during the dismiss window is not nulled out by the stale dismiss timer', () => {
		startPoll(['A', 'B'], 5, vi.fn());
		vi.advanceTimersByTime(5000); // t=5000: resolved, dismiss timer armed for t=13000
		expect(chatPollState.poll?.status).toBe('resolved');

		vi.advanceTimersByTime(2000); // t=7000: still within the dismiss window
		const onDoneSecond = vi.fn();
		startPoll(['X', 'Y'], 100, onDoneSecond); // long duration — stays 'running' through this test

		vi.advanceTimersByTime(6000); // t=13000: the first poll's stale dismiss timer would fire here
		expect(chatPollState.poll?.status).toBe('running');
		expect(chatPollState.poll?.options).toEqual(['X', 'Y']);
	});

	it('allows starting a new poll once the previous one has resolved', () => {
		startPoll(['A', 'B'], 5, vi.fn());
		vi.advanceTimersByTime(5000);
		expect(chatPollState.poll?.status).toBe('resolved');

		const onDoneSecond = vi.fn();
		startPoll(['X', 'Y'], 5, onDoneSecond);
		expect(chatPollState.poll?.status).toBe('running');
		expect(chatPollState.poll?.options).toEqual(['X', 'Y']);
	});
});
