import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { defaultQVarParams, getGlobalQVarParams } from './q-vars.js';
import { qvarState, updateQVarValue, addQVarWatch, removeQVarWatch } from './q-vars-store.svelte.js';

function resetState() {
	qvarState.params = [defaultQVarParams(), defaultQVarParams(), defaultQVarParams(), defaultQVarParams()];
}

describe('q-vars-store', () => {
	beforeEach(() => {
		vi.stubGlobal('window', {});
		resetState();
	});
	afterEach(() => vi.unstubAllGlobals());

	it('starts with 4 slots of default q-var params, all disabled', () => {
		expect(qvarState.params).toHaveLength(4);
		expect(qvarState.params[0]).toEqual(defaultQVarParams());
	});

	it('addQVarWatch enables a q-var (1-indexed) and resets its value to 0', () => {
		updateQVarValue(0, 5, 1.7); // pre-set a value before watching
		addQVarWatch(0, 5);
		expect(qvarState.params[0].enabled[4]).toBe(true);
		expect(qvarState.params[0].value[4]).toBe(0);
		expect(getGlobalQVarParams()[0].enabled[4]).toBe(true);
	});

	it('updateQVarValue writes through to the window-backed global without touching enabled', () => {
		addQVarWatch(1, 3);
		updateQVarValue(1, 3, -1.2);
		expect(qvarState.params[1].value[2]).toBe(-1.2);
		expect(getGlobalQVarParams()[1].value[2]).toBe(-1.2);
		expect(qvarState.params[1].enabled[2]).toBe(true);
	});

	it('removeQVarWatch disables without touching the last value', () => {
		addQVarWatch(2, 1);
		updateQVarValue(2, 1, 2);
		removeQVarWatch(2, 1);
		expect(qvarState.params[2].enabled[0]).toBe(false);
		expect(qvarState.params[2].value[0]).toBe(2);
		expect(getGlobalQVarParams()[2].enabled[0]).toBe(false);
	});
});
