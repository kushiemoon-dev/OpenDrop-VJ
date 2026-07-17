import { describe, it, expect } from 'vitest';
import { createAntiEchoGuard } from './obs-link-actions.js';

describe('createAntiEchoGuard', () => {
	it('does not suppress by default', () => {
		const guard = createAntiEchoGuard();
		expect(guard.shouldSuppressOutbound()).toBe(false);
	});

	it('suppresses exactly once after an incoming scene change is marked', () => {
		const guard = createAntiEchoGuard();
		guard.markIncoming();
		expect(guard.shouldSuppressOutbound()).toBe(true);
		expect(guard.shouldSuppressOutbound()).toBe(false); // one-shot, not sticky
	});
});
