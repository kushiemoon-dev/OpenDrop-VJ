<script lang="ts">
	import type { Overlay } from '$lib/engine/overlay.js';
	import { loadAsset } from '$lib/engine/overlay.js';

	// Local structural type, same convention as SidebarStreaming.svelte's `NdiSlot` —
	// mirrors chat-poll-store.svelte.ts's poll shape without importing the store here
	// (this component has no other dependency on chat-poll state).
	type ChatPoll = {
		status: 'running' | 'resolved';
		options: string[];
		secondsLeft: number;
		winnerIndex: number | null;
		tally: number[];
	};

	interface Props {
		overlays: Overlay[];
		beat: boolean;
		visibleIds: Set<string>;
		/** Current chat poll (Task 13), if one is running or just resolved. Optional —
		 * callers that don't wire chat polls (e.g. the output window, until Task 14) can omit it. */
		poll?: ChatPoll | null;
	}

	let { overlays, beat, visibleIds, poll = null }: Props = $props();

	function pollPercent(t: number[], i: number): number {
		const total = t.reduce((a, b) => a + b, 0);
		return total === 0 ? 0 : (t[i] / total) * 100;
	}

	// id → data URL, loaded from IndexedDB
	let srcs = $state<Record<string, string>>({});

	$effect(() => {
		for (const ov of overlays) {
			if (!(ov.id in srcs)) {
				loadAsset(ov.id).then((url) => {
					if (url) srcs = { ...srcs, [ov.id]: url };
				});
			}
		}
	});

	// spin/drift = pure CSS animations (no RAF).
	// ponytail: amplitude/speed approximated for a convincing VJ look,
	// not a physical simulation — good enough until proven otherwise.
	function spinStyle(spin: number): string {
		if (!spin) return '';
		const dur = 360 / Math.abs(spin);
		return `animation: od-spin ${dur}s linear infinite ${spin < 0 ? 'reverse' : 'normal'};`;
	}

	function driftStyle(driftX: number, driftY: number): string {
		if (!driftX && !driftY) return '';
		const speed = Math.max(Math.abs(driftX), Math.abs(driftY), 0.05);
		const dur = 1 / speed;
		return `--drift-x: ${(driftX * 60).toFixed(0)}px; --drift-y: ${(driftY * 60).toFixed(0)}px; animation: od-drift ${dur}s ease-in-out infinite alternate;`;
	}
</script>

{#each overlays.filter((o) => visibleIds.has(o.id)) as ov (ov.id)}
	{@const pulse = beat && ov.beatReactive}
	<div class="overlay-anchor" style="left:{ov.x * 100}%; top:{ov.y * 100}%; {spinStyle(ov.spin)}">
		<div class="overlay-drift" style={driftStyle(ov.driftX, ov.driftY)}>
			{#if ov.kind === 'text'}
				<div
					class="overlay-text"
					class:beat-pulse={pulse}
					style="
						transform: translate(-50%, -50%) scale({pulse ? ov.scale * ov.beatScale : ov.scale});
						font-size: {ov.fontSize}vh;
						font-family: var(--od-font-{ov.fontFamily});
						color: {ov.color};
						opacity: {ov.opacity};
						mix-blend-mode: {ov.blendMode};
					"
				>{ov.text}</div>
			{:else if srcs[ov.id]}
				{#if ov.video}
					<video
						src={srcs[ov.id]}
						class="overlay-media"
						class:beat-pulse={pulse}
						autoplay
						loop
						muted
						playsinline
						style="
							transform: translate(-50%, -50%) scale({pulse ? ov.scale * ov.beatScale : ov.scale}) rotate({ov.rotation}deg);
							opacity: {ov.opacity};
							mix-blend-mode: {ov.blendMode};
						"
					></video>
				{:else}
					<img
						src={srcs[ov.id]}
						alt={ov.name}
						class="overlay-media"
						class:beat-pulse={pulse}
						style="
							transform: translate(-50%, -50%) scale({pulse ? ov.scale * ov.beatScale : ov.scale}) rotate({ov.rotation}deg);
							opacity: {ov.opacity};
							mix-blend-mode: {ov.blendMode};
						"
					/>
				{/if}
			{/if}
		</div>
	</div>
{/each}

{#if poll}
	<div class="poll-hud">
		<div class="poll-header">
			{poll.status === 'running' ? `Vote — ${poll.secondsLeft}s` : 'Vote terminé'}
		</div>
		{#each poll.options as option, i (i)}
			<div class="poll-row" class:poll-winner={poll.status === 'resolved' && poll.winnerIndex === i}>
				<span class="poll-rank">{i + 1}</span>
				<span class="poll-name">{option}</span>
				<span class="poll-count">{poll.tally[i] ?? 0}</span>
				<div class="poll-bar" style="width:{pollPercent(poll.tally, i)}%"></div>
			</div>
		{/each}
	</div>
{/if}

<style>
	.overlay-anchor {
		position: absolute;
		transform-origin: 0 0;
	}

	.overlay-drift {
		position: relative;
		transform-origin: 0 0;
	}

	.overlay-media {
		position: absolute;
		pointer-events: none;
		/* vw/vh rather than % : the containing block here is the spin/drift wrapper
		   (size 0, needed so rotation pivots around the anchor point),
		   not the visualizer — % would resolve to 0. Both usages (stage, output) are fullscreen. */
		max-width: 80vw;
		max-height: 80vh;
		transition: transform 80ms ease-out;
		user-select: none;
	}

	.overlay-text {
		position: absolute;
		pointer-events: none;
		white-space: pre-wrap;
		text-align: center;
		max-width: 90vw;
		max-height: 80vh;
		overflow: hidden;
		transform-origin: 0 0;
		transition: transform 80ms ease-out;
		user-select: none;
		font-weight: 700;
		text-shadow: 0 0.15vh 0.4vh rgba(0, 0, 0, 0.85), 0 0 1vh rgba(0, 0, 0, 0.6);
		--od-font-sans: system-ui, -apple-system, 'Segoe UI', sans-serif;
		--od-font-serif: Georgia, 'Times New Roman', serif;
		--od-font-mono: 'Courier New', Consolas, monospace;
		--od-font-impact: Impact, 'Arial Black', sans-serif;
		--od-font-comic: 'Comic Sans MS', 'Comic Sans', cursive;
	}

	@keyframes od-spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	@keyframes od-drift {
		from { transform: translate(0, 0); }
		to { transform: translate(var(--drift-x, 0), var(--drift-y, 0)); }
	}

	/* Chat poll HUD (Task 13) — same corner-card treatment as the rest of the app's
	   HUD-ish surfaces, kept self-contained (no shared class names) since this isn't
	   an Overlay asset, just ephemeral poll state rendered in the same layer. */
	.poll-hud {
		position: absolute;
		top: 16px;
		right: 16px;
		z-index: 5;
		min-width: 180px;
		padding: 0.6rem 0.8rem;
		background: rgba(7, 7, 26, 0.82);
		border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
		border-radius: var(--r-md, 6px);
		font-family: system-ui, -apple-system, 'Segoe UI', sans-serif;
		color: var(--text-primary, #e0e0ff);
		pointer-events: none;
		user-select: none;
	}

	.poll-header {
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--accent, #ff2d78);
		margin-bottom: 0.4rem;
	}

	.poll-row {
		position: relative;
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.2rem 0;
		font-size: 12px;
		overflow: hidden;
	}

	.poll-rank { opacity: 0.6; width: 1em; flex-shrink: 0; }

	.poll-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.poll-count { font-weight: 700; min-width: 1.4em; text-align: right; flex-shrink: 0; }

	.poll-winner { color: var(--ok, #4ade80); }

	.poll-bar {
		position: absolute;
		left: 0;
		bottom: 0;
		height: 2px;
		background: currentColor;
		opacity: 0.5;
		transition: width 200ms ease-out;
	}
</style>
