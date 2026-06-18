/**
 * Vitest global setup — runs before any test module is evaluated.
 *
 * Svelte 5 runes ($state, $derived, etc.) are compiler macros that get
 * rewritten during the Vite+SvelteKit transform pipeline. In vitest's
 * node environment (no Svelte compiler plugin), they remain as bare
 * identifiers and throw ReferenceError at module load time.
 *
 * Stub them as identity functions so that .svelte.ts modules can be
 * imported and their pure-function exports can be tested without a browser.
 */

/* eslint-disable @typescript-eslint/no-explicit-any */
;(globalThis as any).$state = <T>(v: T): T => v
;(globalThis as any).$derived = <T>(fn: () => T): T => fn()
;(globalThis as any).$effect = (_fn: () => void): void => {}
;(globalThis as any).$props = <T>(v?: T): T => (v ?? {}) as T
