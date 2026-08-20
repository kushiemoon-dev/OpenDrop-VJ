/**
 * ndi-deck-store.svelte.ts — per-slot NDI-out toggle state, same shape as
 * electron-features-store.svelte.ts. Slot indices match deckState (0=A 1=B 2=C 3=D).
 */

export const ndiDeckState = $state({
  slots: [
    { active: false, error: '' },
    { active: false, error: '' },
    { active: false, error: '' },
    { active: false, error: '' },
  ],
})
