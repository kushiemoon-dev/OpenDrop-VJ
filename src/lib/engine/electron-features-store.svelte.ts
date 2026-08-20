/**
 * electron-features-store.svelte.ts — reactive wrapper around the
 * Electron-only feature toggles (NDI/OSC/Remote/Ableton Link/v4l2/Spout).
 * Extracted from +page.svelte, same shape as color-store.svelte.ts — plain
 * $state, mutated directly by the toggleX() functions that stay in
 * +page.svelte (they call window.electronAPI, a browser-API boundary never
 * unit tested in this codebase).
 *
 * Extension .svelte.ts is required to compile Svelte 5 $state runes.
 */

export const electronFeaturesState = $state({
  ndi: { active: false, error: '' },
  osc: { active: false, port: 7000, error: '' },
  remote: { active: false, url: '', error: '' },
  link: { active: false, peers: 0, error: '' },
  v4l2: { active: false, error: '' },
  spout: { active: false, error: '' },
})
