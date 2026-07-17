import { describe, it, expect } from 'vitest';
import { namespacedVoterId } from './chat-poll-actions.js';

describe('namespacedVoterId', () => {
	it('prefixes the platform so the same user-id on two platforms cannot double-vote as if merged, nor collide', () => {
		expect(namespacedVoterId('twitch', '123')).toBe('twitch:123');
		expect(namespacedVoterId('kick', '123')).toBe('kick:123');
		expect(namespacedVoterId('twitch', '123')).not.toBe(namespacedVoterId('kick', '123'));
	});
});
