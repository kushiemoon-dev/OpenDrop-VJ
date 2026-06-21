import type { CommandId } from './commands.js';

export type KeyBinding = Record<string, CommandId>;

export const DEFAULT_KEYMAP: KeyBinding = {
	'ArrowLeft': 'crossfader-left',
	'ArrowRight': 'crossfader-right',
	'Tab': 'deck-switch',
	'[': 'preset-prev-active',
	']': 'preset-next-active',
	' ': 'playlist-toggle-active',
	'n': 'playlist-next-active',
	'N': 'playlist-next-active',
	'p': 'playlist-prev-active',
	'P': 'playlist-prev-active',
};

const STORAGE_KEY = 'od-keymap';

export function loadKeymap(): KeyBinding {
	try {
		const saved = localStorage.getItem(STORAGE_KEY);
		if (saved) return { ...DEFAULT_KEYMAP, ...JSON.parse(saved) };
	} catch {
		// ignore parse errors
	}
	return { ...DEFAULT_KEYMAP };
}

export function saveKeymap(keymap: KeyBinding): void {
	localStorage.setItem(STORAGE_KEY, JSON.stringify(keymap));
}

export function resetKeymap(): KeyBinding {
	localStorage.removeItem(STORAGE_KEY);
	return { ...DEFAULT_KEYMAP };
}

/** Human-readable label for a key string (e.g. ' ' → 'Space', 'ArrowLeft' → '←'). */
export function formatKey(key: string): string {
	switch (key) {
		case ' ': return 'Space';
		case 'ArrowLeft': return '←';
		case 'ArrowRight': return '→';
		case 'ArrowUp': return '↑';
		case 'ArrowDown': return '↓';
		case 'Tab': return 'Tab';
		case 'Escape': return 'Esc';
		case 'Enter': return 'Enter';
		case 'Backspace': return '⌫';
		case 'Delete': return 'Del';
		default: return key.length === 1 ? key.toUpperCase() : key;
	}
}
