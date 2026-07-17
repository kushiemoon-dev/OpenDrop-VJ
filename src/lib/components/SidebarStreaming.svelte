<script lang="ts">
	import { obsLinkState, saveObsConfig } from '$lib/engine/obs-link-store.svelte.js';
	import { connectObs, disconnectObs } from '$lib/engine/obs-link-actions.js';
	import { findTargetForScene, type MappingTarget } from '$lib/engine/obs-mapping.js';
	import { FAV_COLORS, loadMoodLabels, saveMoodLabels } from '$lib/presets/favorites.js';
	import { chatPollState } from '$lib/engine/chat-poll-store.svelte.js';
	import { connectTwitch, connectKick } from '$lib/engine/chat-poll-actions.js';

	type NdiSlot = { active: boolean; error: string };

	interface Props {
		slots: NdiSlot[];
		slotLabels: string[];
		toggleNdiDeck: (slot: number) => void;
		onStartPoll: () => void;
	}

	let { slots, slotLabels, toggleNdiDeck, onStartPoll }: Props = $props();

	let moodLabels = $state(loadMoodLabels());
	let obsHost = $state(obsLinkState.host);
	let obsPort = $state(obsLinkState.port);
	let twitchChannel = $state('');
	let kickChannel = $state('');

	function updateMoodLabel(colorIndex: number, label: string): void {
		moodLabels = { ...moodLabels, [colorIndex]: label };
		saveMoodLabels(moodLabels);
	}

	function targetToValue(target: MappingTarget | undefined): string {
		if (!target) return '';
		return target.type === 'slot' ? `slot:${target.slot}` : `mood:${target.colorIndex}`;
	}

	function valueToTarget(value: string): MappingTarget | null {
		if (!value) return null;
		const [type, n] = value.split(':');
		const num = Number(n);
		if (type === 'slot') return { type: 'slot', slot: num as 0 | 1 | 2 | 3 };
		if (type === 'mood') return { type: 'mood', colorIndex: num as 1 | 2 | 3 | 4 | 5 };
		return null;
	}

	function updateSceneMapping(sceneName: string, value: string): void {
		const rest = obsLinkState.mapping.filter((entry) => entry.sceneName !== sceneName);
		const target = valueToTarget(value);
		obsLinkState.mapping = target ? [...rest, { sceneName, target }] : rest;
		saveObsConfig();
	}

	async function handleConnectObs(): Promise<void> {
		await connectObs(obsHost, obsPort);
		saveObsConfig();
	}
</script>

<div class="controls-section">
	<span class="label">NDI per deck</span>
	{#each slots as slot, i (i)}
		<div class="midi-row">
			<span class="midi-label">{slotLabels[i]}</span>
			<button class="btn-sm" class:active={slot.active} onclick={() => toggleNdiDeck(i)}>
				{slot.active ? 'Stop' : 'Start'}
			</button>
		</div>
		{#if slot.error}<div class="ndi-error">{slot.error}</div>{/if}
	{/each}
</div>

<div class="controls-section">
	<span class="label">OBS WebSocket</span>
	{#if !obsLinkState.connected}
		<div class="midi-row">
			<input class="obs-input" type="text" bind:value={obsHost} placeholder="localhost" />
			<input class="obs-input obs-input-port" type="number" bind:value={obsPort} placeholder="4455" />
			<button class="btn-sm" onclick={handleConnectObs}>Connecter</button>
		</div>
		<p class="hint">
			Si l'instance OBS a un mot de passe (Outils → Paramètres du serveur WebSocket
			dans OBS, activé par défaut depuis OBS 28), enregistre-le avant de connecter,
			dans la console devtools de OpenDrop : <code>window.electronAPI.setSecret('obs-password', '...')</code>.
		</p>
	{:else}
		<button class="btn-sm" onclick={disconnectObs}>Déconnecter</button>
	{/if}
	{#if obsLinkState.error}<div class="ndi-error">{obsLinkState.error}</div>{/if}
</div>

{#if obsLinkState.connected}
	<div class="controls-section">
		<span class="label">Mapping scène → slot/mood</span>
		{#each obsLinkState.scenes as scene (scene)}
			<div class="midi-row">
				<span class="obs-scene-label" title={scene}>{scene}</span>
				<select
					class="obs-select"
					value={targetToValue(findTargetForScene(obsLinkState.mapping, scene))}
					onchange={(e) => updateSceneMapping(scene, e.currentTarget.value)}
				>
					<option value="">—</option>
					<option value="slot:0">Slot A</option>
					<option value="slot:1">Slot B</option>
					<option value="slot:2">Slot C</option>
					<option value="slot:3">Slot D</option>
					<option value="mood:1">Mood 1</option>
					<option value="mood:2">Mood 2</option>
					<option value="mood:3">Mood 3</option>
					<option value="mood:4">Mood 4</option>
					<option value="mood:5">Mood 5</option>
				</select>
			</div>
		{/each}
	</div>

	<div class="controls-section">
		<span class="label">Labels des moods</span>
		{#each FAV_COLORS.slice(1) as color, i (i)}
			<div class="midi-row">
				<span class="mood-swatch" style:background={color}></span>
				<input
					class="obs-input"
					type="text"
					value={moodLabels[i + 1] ?? ''}
					oninput={(e) => updateMoodLabel(i + 1, e.currentTarget.value)}
					placeholder={`Mood ${i + 1}`}
				/>
			</div>
		{/each}
	</div>
{/if}

<div class="controls-section">
	<span class="label">Chat poll (Twitch + Kick)</span>
	<div class="midi-row">
		<input class="obs-input" type="text" bind:value={twitchChannel} placeholder="Twitch channel" />
		<button class="btn-sm" class:active={chatPollState.twitch.connected} onclick={() => connectTwitch(twitchChannel)}>
			{chatPollState.twitch.connected ? 'Connecté' : 'Connecter'}
		</button>
	</div>
	{#if chatPollState.twitch.error}<div class="ndi-error">{chatPollState.twitch.error}</div>{/if}

	<p class="hint">
		Twitch nécessite un token OAuth avec le scope <code>chat:read</code> — génère-le
		via twitchtokengenerator.com ou la console développeur Twitch, puis dans la console
		devtools de OpenDrop enregistre-le : <code>window.electronAPI.setSecret('twitch-oauth-token', '...')</code>.
	</p>

	<div class="midi-row">
		<input class="obs-input" type="text" bind:value={kickChannel} placeholder="Kick channel" />
		<button class="btn-sm" class:active={chatPollState.kick.connected} onclick={() => connectKick(kickChannel)}>
			{chatPollState.kick.connected ? 'Connecté' : 'Connecter'}
		</button>
	</div>
	{#if chatPollState.kick.error}<div class="ndi-error">{chatPollState.kick.error}</div>{/if}

	<p class="hint">
		Kick n'a pas d'API publique de lecture du chat — connecte-toi à kick.com dans un
		navigateur, ouvre les devtools (onglet Application → Cookies pour le cookie de
		session, Network pour le bearer token et le token XSRF), puis dans la console de
		OpenDrop enregistre les trois valeurs : <code>window.electronAPI.setSecret('kick-bearer-token', '...')</code>,
		<code>window.electronAPI.setSecret('kick-xsrf-token', '...')</code> et
		<code>window.electronAPI.setSecret('kick-cookies', '...')</code>. Méthode non
		officielle, peut cesser de fonctionner sans préavis.
	</p>

	<button
		class="btn-sm poll-trigger"
		onclick={onStartPoll}
		disabled={chatPollState.poll?.status === 'running'}
	>
		{chatPollState.poll?.status === 'running'
			? `Vote en cours… ${chatPollState.poll.secondsLeft}s`
			: 'Lancer un vote de preset'}
	</button>
</div>

<style>
	.controls-section {
		padding: var(--sp-3);
		border-bottom: 1px solid var(--border-subtle);
		display: flex; flex-direction: column; gap: 0.4rem;
	}

	.label {
		font-size: 10px; text-transform: uppercase; letter-spacing: 0.08em;
		color: var(--accent); font-weight: 600;
	}

	.midi-row { display: flex; align-items: center; gap: 3px; }

	.midi-label { font-size: 10px; color: var(--text-muted); width: 80px; flex-shrink: 0; white-space: nowrap; }

	.btn-sm {
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.6rem; font-size: 12px; cursor: pointer;
		transition: border-color var(--t-fast), color var(--t-fast);
	}

	.btn-sm:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }

	.btn-sm.active {
		background: var(--accent-dim); border-color: var(--accent); color: var(--accent);
		box-shadow: 0 0 8px var(--accent-dim);
	}

	.btn-sm:disabled { opacity: 0.3; cursor: not-allowed; }

	.poll-trigger { width: 100%; text-align: center; margin-top: 0.2rem; }

	.hint { margin: 0.2rem 0; font-size: 11px; color: var(--text-secondary); line-height: 1.5; }

	.hint code { background: var(--bg-hover); padding: 0.1rem 0.3rem; border-radius: var(--r-sm); font-size: 10px; }

	.ndi-error {
		font-size: 10px;
		color: var(--error);
		margin-top: 2px;
	}

	.obs-input {
		flex: 1; min-width: 0;
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.5rem; font-size: 11px;
	}

	.obs-input:focus { outline: none; border-color: var(--accent); }

	.obs-input-port { flex: 0 0 70px; }

	.obs-select {
		flex: 1;
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border); border-radius: var(--r-sm);
		padding: 0.25rem 0.4rem; font-size: 11px; cursor: pointer;
	}

	.obs-scene-label {
		font-size: 10px; color: var(--text-muted); flex: 1;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.mood-swatch {
		width: 12px; height: 12px; border-radius: 50%; flex-shrink: 0;
		border: 1px solid var(--border);
	}
</style>
