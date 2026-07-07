import { describe, it, expect } from 'vitest';
import { LfoEngine, defaultSlot } from './lfo.js';

describe('LfoEngine — sine', () => {
	it('sine equals 0.5 at phase=0', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'sine', center: 0.5, amount: 1, rate: 1 };
		const [{ value01 }] = engine.tick(0);
		expect(value01).toBeCloseTo(0.5); // sin(0)=0 → raw=0.5 → center=0.5+0=0.5
	});

	it('sine equals 1 at phase=0.25 (peak)', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'sine', center: 0.5, amount: 1, rate: 1 };
		const [{ value01 }] = engine.tick(0.25); // sin(π/2)=1 → raw=1 → val=0.5+(0.5)*1=1
		expect(value01).toBeCloseTo(1);
	});

	it('sine equals 0 at phase=0.75 (trough)', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'sine', center: 0.5, amount: 1, rate: 1 };
		const [{ value01 }] = engine.tick(0.75); // sin(3π/2)=-1 → raw=0 → val=0.5-0.5=0
		expect(value01).toBeCloseTo(0);
	});
});

describe('LfoEngine — saw', () => {
	it('saw with center=0.5, amount=1 → range 0..1', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'saw', center: 0.5, amount: 1, rate: 1 };
		// raw=0.6 → val=0.5+(0.6-0.5)*1=0.6
		expect(engine.tick(0.6)[0].value01).toBeCloseTo(0.6);
		expect(engine.tick(0)[0].value01).toBeCloseTo(0);   // raw=0 → 0.5-0.5=0
		expect(engine.tick(1)[0].value01).toBeCloseTo(0);   // raw=1%1=0 → 0
	});
});

describe('LfoEngine — square', () => {
	it('square equals 1 in the first half', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'square', center: 0.5, amount: 1, rate: 1 };
		const [{ value01 }] = engine.tick(0.25); // p=0.25 < 0.5 → raw=1 → val=0.5+0.5=1
		expect(value01).toBe(1);
	});

	it('square equals 0 in the second half', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'square', center: 0.5, amount: 1, rate: 1 };
		const [{ value01 }] = engine.tick(0.75); // raw=0 → val=0.5-0.5=0
		expect(value01).toBe(0);
	});
});

describe('LfoEngine — rate', () => {
	it('rate=2 doubles the frequency (full cycle at phase=0.5)', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'saw', center: 0.5, amount: 1, rate: 2 };
		// p = (0.5 * 2 + 0) % 1 = 0 → raw=0 → val=0
		expect(engine.tick(0.5)[0].value01).toBeCloseTo(0);
	});
});

describe('LfoEngine — amount + center', () => {
	it('amount=0.5 limits the deviation to ±0.25 of center', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'sine', center: 0.5, amount: 0.5, rate: 1 };
		const peak = engine.tick(0.25)[0].value01;   // raw=1 → 0.5+(1-0.5)*0.5=0.75
		const trough = engine.tick(0.75)[0].value01; // raw=0 → 0.5+(0-0.5)*0.5=0.25
		expect(peak).toBeCloseTo(0.75);
		expect(trough).toBeCloseTo(0.25);
	});

	it('values clamped to 0..1', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'sine', center: 0, amount: 2, rate: 1 };
		const values = [0, 0.25, 0.5, 0.75].map(p => engine.tick(p)[0].value01);
		for (const v of values) {
			expect(v).toBeGreaterThanOrEqual(0);
			expect(v).toBeLessThanOrEqual(1);
		}
	});
});

describe('LfoEngine — disabled slot', () => {
	it('disabled slot returns center as value, target null', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: false, center: 0.3, target: 'crossfader' };
		const [result] = engine.tick(0.5);
		expect(result.target).toBeNull();
		expect(result.value01).toBeCloseTo(0.3);
	});
});

describe('LfoEngine — randomizeSH', () => {
	it('s&h returns the memorized value', () => {
		const engine = new LfoEngine();
		engine.slots[0] = { ...defaultSlot(), enabled: true, shape: 'sh', center: 0.5, amount: 1 };
		engine.randomizeSH();
		const v1 = engine.tick(0)[0].value01;
		const v2 = engine.tick(0.5)[0].value01;
		expect(v1).toBe(v2); // same S&H value across phases
	});
});
