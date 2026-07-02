export type CommandId =
	// Deck A/B controls (existing MidiAction set)
	| 'crossfader'
	| 'preset-prev-a' | 'preset-next-a'
	| 'preset-prev-b' | 'preset-next-b'
	| 'playlist-toggle-a' | 'playlist-toggle-b'
	| 'playlist-prev-a' | 'playlist-next-a'
	| 'playlist-prev-b' | 'playlist-next-b'
	// Active-deck shortcuts (keyboard-friendly)
	| 'crossfader-left' | 'crossfader-right'
	| 'deck-switch'
	| 'preset-prev-active' | 'preset-next-active'
	| 'playlist-toggle-active'
	| 'playlist-prev-active' | 'playlist-next-active'
	// M2 — Strobe / LFO (wired in M2, declared here for keymap/MIDI pre-mapping)
	| 'strobe-toggle'
	| 'lfo-rate-up' | 'lfo-rate-down'
	// M3 — Color controls (5 params × 2 decks)
	| 'color-hue-a' | 'color-sat-a' | 'color-bright-a' | 'color-contrast-a' | 'color-invert-a'
	| 'color-hue-b' | 'color-sat-b' | 'color-bright-b' | 'color-contrast-b' | 'color-invert-b'
	// 1.1 — Compositing controls (blend + lumaKey + colorKey × 4 slots)
	| 'composite-blend-0' | 'composite-blend-1' | 'composite-blend-2' | 'composite-blend-3'
	| 'lumakey-black-0' | 'lumakey-black-1' | 'lumakey-black-2' | 'lumakey-black-3'
	| 'lumakey-white-0' | 'lumakey-white-1' | 'lumakey-white-2' | 'lumakey-white-3'
	| 'colorkey-hue-0' | 'colorkey-hue-1' | 'colorkey-hue-2' | 'colorkey-hue-3'
	| 'colorkey-tolerance-0' | 'colorkey-tolerance-1' | 'colorkey-tolerance-2' | 'colorkey-tolerance-3'
	;

export interface CommandContext {
	getCrossfader(): number;
	setCrossfader(v: number): void;
	getActiveDeck(): 'A' | 'B';
	switchActiveDeck(): void;
	navigatePreset(deck: 'A' | 'B', direction: 1 | -1): void;
	togglePlaylist(deck: 'A' | 'B'): void;
	playlistNext(deck: 'A' | 'B'): void;
	playlistPrev(deck: 'A' | 'B'): void;
}

export interface Command {
	readonly id: CommandId;
	readonly label: string;
	/** 'range' uses value01 (0..1); 'trigger' ignores it. */
	readonly kind: 'trigger' | 'range';
	run(value01: number, ctx: CommandContext): void;
}

export class CommandRegistry {
	private readonly _commands = new Map<CommandId, Command>();

	register(cmd: Command): void {
		this._commands.set(cmd.id, cmd);
	}

	/** Dispatch a command. value01 must be 0..1 (callers normalize MIDI 0-127 before calling). */
	dispatch(id: CommandId, value01: number, ctx: CommandContext): void {
		this._commands.get(id)?.run(value01, ctx);
	}

	get(id: CommandId): Command | undefined {
		return this._commands.get(id);
	}

	all(): Command[] {
		return [...this._commands.values()];
	}
}

const CROSSFADER_STEP = 0.05;

const DEFAULT_COMMANDS: Command[] = [
	// Range
	{
		id: 'crossfader',
		label: 'Crossfader',
		kind: 'range',
		run(v, ctx) { ctx.setCrossfader(v); },
	},
	// Absolute preset navigation
	{
		id: 'preset-prev-a',
		label: '◀ Preset A',
		kind: 'trigger',
		run(_, ctx) { ctx.navigatePreset('A', -1); },
	},
	{
		id: 'preset-next-a',
		label: '▶ Preset A',
		kind: 'trigger',
		run(_, ctx) { ctx.navigatePreset('A', 1); },
	},
	{
		id: 'preset-prev-b',
		label: '◀ Preset B',
		kind: 'trigger',
		run(_, ctx) { ctx.navigatePreset('B', -1); },
	},
	{
		id: 'preset-next-b',
		label: '▶ Preset B',
		kind: 'trigger',
		run(_, ctx) { ctx.navigatePreset('B', 1); },
	},
	// Playlist absolute
	{
		id: 'playlist-toggle-a',
		label: '⏯ Playlist A',
		kind: 'trigger',
		run(_, ctx) { ctx.togglePlaylist('A'); },
	},
	{
		id: 'playlist-toggle-b',
		label: '⏯ Playlist B',
		kind: 'trigger',
		run(_, ctx) { ctx.togglePlaylist('B'); },
	},
	{
		id: 'playlist-prev-a',
		label: '⏮ Playlist A',
		kind: 'trigger',
		run(_, ctx) { ctx.playlistPrev('A'); },
	},
	{
		id: 'playlist-next-a',
		label: '⏭ Playlist A',
		kind: 'trigger',
		run(_, ctx) { ctx.playlistNext('A'); },
	},
	{
		id: 'playlist-prev-b',
		label: '⏮ Playlist B',
		kind: 'trigger',
		run(_, ctx) { ctx.playlistPrev('B'); },
	},
	{
		id: 'playlist-next-b',
		label: '⏭ Playlist B',
		kind: 'trigger',
		run(_, ctx) { ctx.playlistNext('B'); },
	},
	// Active-deck shortcuts
	{
		id: 'crossfader-left',
		label: 'Crossfader ←',
		kind: 'trigger',
		run(_, ctx) {
			ctx.setCrossfader(Math.max(0, parseFloat((ctx.getCrossfader() - CROSSFADER_STEP).toFixed(2))));
		},
	},
	{
		id: 'crossfader-right',
		label: 'Crossfader →',
		kind: 'trigger',
		run(_, ctx) {
			ctx.setCrossfader(Math.min(1, parseFloat((ctx.getCrossfader() + CROSSFADER_STEP).toFixed(2))));
		},
	},
	{
		id: 'deck-switch',
		label: 'Switch active deck',
		kind: 'trigger',
		run(_, ctx) { ctx.switchActiveDeck(); },
	},
	{
		id: 'preset-prev-active',
		label: '◀ Preset (active deck)',
		kind: 'trigger',
		run(_, ctx) { ctx.navigatePreset(ctx.getActiveDeck(), -1); },
	},
	{
		id: 'preset-next-active',
		label: '▶ Preset (active deck)',
		kind: 'trigger',
		run(_, ctx) { ctx.navigatePreset(ctx.getActiveDeck(), 1); },
	},
	{
		id: 'playlist-toggle-active',
		label: '⏯ Playlist (active deck)',
		kind: 'trigger',
		run(_, ctx) { ctx.togglePlaylist(ctx.getActiveDeck()); },
	},
	{
		id: 'playlist-prev-active',
		label: '⏮ Playlist (active deck)',
		kind: 'trigger',
		run(_, ctx) { ctx.playlistPrev(ctx.getActiveDeck()); },
	},
	{
		id: 'playlist-next-active',
		label: '⏭ Playlist (active deck)',
		kind: 'trigger',
		run(_, ctx) { ctx.playlistNext(ctx.getActiveDeck()); },
	},
	// M2 stubs — wired in M2
	{ id: 'strobe-toggle', label: 'Strobe ON/OFF', kind: 'trigger', run() {} },
	{ id: 'lfo-rate-up', label: 'LFO Rate +', kind: 'trigger', run() {} },
	{ id: 'lfo-rate-down', label: 'LFO Rate −', kind: 'trigger', run() {} },
	// M3 stubs — wired in M3 via registry.register() in +page.svelte
	{ id: 'color-hue-a', label: 'Hue A', kind: 'range', run() {} },
	{ id: 'color-sat-a', label: 'Saturation A', kind: 'range', run() {} },
	{ id: 'color-bright-a', label: 'Brightness A', kind: 'range', run() {} },
	{ id: 'color-contrast-a', label: 'Contrast A', kind: 'range', run() {} },
	{ id: 'color-invert-a', label: 'Invert A', kind: 'range', run() {} },
	{ id: 'color-hue-b', label: 'Hue B', kind: 'range', run() {} },
	{ id: 'color-sat-b', label: 'Saturation B', kind: 'range', run() {} },
	{ id: 'color-bright-b', label: 'Brightness B', kind: 'range', run() {} },
	{ id: 'color-contrast-b', label: 'Contrast B', kind: 'range', run() {} },
	{ id: 'color-invert-b', label: 'Invert B', kind: 'range', run() {} },
	// 1.1 stubs — wired in +page.svelte once the Compositor lands
	{ id: 'composite-blend-0', label: 'Blend 0', kind: 'range', run() {} },
	{ id: 'composite-blend-1', label: 'Blend 1', kind: 'range', run() {} },
	{ id: 'composite-blend-2', label: 'Blend 2', kind: 'range', run() {} },
	{ id: 'composite-blend-3', label: 'Blend 3', kind: 'range', run() {} },
	{ id: 'lumakey-black-0', label: 'Luma Black 0', kind: 'range', run() {} },
	{ id: 'lumakey-black-1', label: 'Luma Black 1', kind: 'range', run() {} },
	{ id: 'lumakey-black-2', label: 'Luma Black 2', kind: 'range', run() {} },
	{ id: 'lumakey-black-3', label: 'Luma Black 3', kind: 'range', run() {} },
	{ id: 'lumakey-white-0', label: 'Luma White 0', kind: 'range', run() {} },
	{ id: 'lumakey-white-1', label: 'Luma White 1', kind: 'range', run() {} },
	{ id: 'lumakey-white-2', label: 'Luma White 2', kind: 'range', run() {} },
	{ id: 'lumakey-white-3', label: 'Luma White 3', kind: 'range', run() {} },
	{ id: 'colorkey-hue-0', label: 'Key Hue 0', kind: 'range', run() {} },
	{ id: 'colorkey-hue-1', label: 'Key Hue 1', kind: 'range', run() {} },
	{ id: 'colorkey-hue-2', label: 'Key Hue 2', kind: 'range', run() {} },
	{ id: 'colorkey-hue-3', label: 'Key Hue 3', kind: 'range', run() {} },
	{ id: 'colorkey-tolerance-0', label: 'Key Tolerance 0', kind: 'range', run() {} },
	{ id: 'colorkey-tolerance-1', label: 'Key Tolerance 1', kind: 'range', run() {} },
	{ id: 'colorkey-tolerance-2', label: 'Key Tolerance 2', kind: 'range', run() {} },
	{ id: 'colorkey-tolerance-3', label: 'Key Tolerance 3', kind: 'range', run() {} },
];

export function createDefaultRegistry(): CommandRegistry {
	const reg = new CommandRegistry();
	for (const cmd of DEFAULT_COMMANDS) reg.register(cmd);
	return reg;
}
