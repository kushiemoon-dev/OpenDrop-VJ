<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';

	// URL params: ?host=IP&port=PORT&token=TOKEN
	let host = $state('');
	let port = $state(0);
	let token = $state('');
	let connected = $state(false);
	let connecting = $state(false);
	let error = $state('');

	let ws: WebSocket | null = null;
	let crossfader = $state(0.5);

	function send(cmd: string, value: number = 0) {
		if (!ws || ws.readyState !== WebSocket.OPEN) return;
		ws.send(JSON.stringify({ token, cmd, value }));
	}

	function connect() {
		if (!host || !port || !token) {
			error = 'Paramètres manquants dans l\'URL (?host=&port=&token=)';
			return;
		}
		if (ws) { ws.close(); ws = null; }
		connecting = true;
		error = '';
		const url = `ws://${host}:${port}`;
		ws = new WebSocket(url);
		ws.onopen = () => { connected = true; connecting = false; };
		ws.onclose = () => { connected = false; connecting = false; };
		ws.onerror = () => {
			error = `Connexion échouée — vérifier que OpenDrop est ouvert et sur le même réseau.`;
			connected = false;
			connecting = false;
		};
	}

	function disconnect() {
		ws?.close();
		ws = null;
		connected = false;
	}

	onMount(() => {
		// Read URL params — works both in browser and SPA navigation
		const params = new URLSearchParams(window.location.search);
		host = params.get('host') ?? '';
		port = Number(params.get('port') ?? '0');
		token = params.get('token') ?? '';
		if (host && port && token) connect();
	});

	onDestroy(() => { ws?.close(); ws = null; });

	function onCrossfaderInput(e: Event) {
		const v = Number((e.target as HTMLInputElement).value);
		crossfader = v;
		send('crossfader', v);
	}
</script>

<svelte:head><title>OpenDrop Remote</title></svelte:head>

<div class="remote">
	<header>
		<span class="logo">OpenDrop</span>
		{#if connecting}
			<span class="badge connecting">Connexion…</span>
		{:else if connected}
			<span class="badge ok">Connecté</span>
			<button class="btn-sm" onclick={disconnect}>Déconnecter</button>
		{:else}
			<span class="badge off">Déconnecté</span>
			{#if host && port && token}
				<button class="btn-sm" onclick={connect}>Reconnecter</button>
			{/if}
		{/if}
	</header>

	{#if error}
		<div class="error-box">{error}</div>
	{/if}

	{#if !host || !port || !token}
		<div class="guide">
			<p>Pour utiliser la télécommande :</p>
			<ol>
				<li>Ouvrez OpenDrop sur votre ordinateur</li>
				<li>Dans le panneau <strong>Remote</strong>, cliquez <strong>Démarrer</strong></li>
				<li>Scannez le QR code ou copiez le lien affiché</li>
			</ol>
		</div>
	{:else}
		<div class="controls" class:disabled={!connected}>

			<!-- Crossfader -->
			<section class="section">
				<div class="label">Crossfader</div>
				<div class="xfade-row">
					<span class="deck-tag">A</span>
					<input
						type="range" min="0" max="1" step="0.001"
						value={crossfader}
						oninput={onCrossfaderInput}
						class="slider xfade"
					/>
					<span class="deck-tag">B</span>
				</div>
			</section>

			<!-- Deck A -->
			<section class="section">
				<div class="label">Deck A</div>
				<div class="btn-row">
					<button class="btn" onclick={() => send('preset-prev-a')}>◀ Prev</button>
					<button class="btn" onclick={() => send('preset-next-a')}>Next ▶</button>
				</div>
			</section>

			<!-- Deck B -->
			<section class="section">
				<div class="label">Deck B</div>
				<div class="btn-row">
					<button class="btn" onclick={() => send('preset-prev-b')}>◀ Prev</button>
					<button class="btn" onclick={() => send('preset-next-b')}>Next ▶</button>
				</div>
			</section>

			<!-- Strobe -->
			<section class="section">
				<div class="label">Strobe</div>
				<div class="btn-row">
					<button class="btn wide" onclick={() => send('strobe-toggle')}>⚡ Toggle Strobe</button>
				</div>
			</section>

			<!-- Auto-change -->
			<section class="section">
				<div class="label">Auto</div>
				<div class="btn-row">
					<button class="btn" onclick={() => send('auto-change-toggle-a')}>Auto A</button>
					<button class="btn" onclick={() => send('auto-change-toggle-b')}>Auto B</button>
				</div>
			</section>
		</div>
	{/if}
</div>

<style>
	:global(*, *::before, *::after) { box-sizing: border-box; margin: 0; padding: 0; }
	:global(html, body) {
		width: 100%; height: 100%;
		background: #0a0a0a;
		color: #e0e0e0;
		font-family: system-ui, -apple-system, sans-serif;
		font-size: 16px;
		-webkit-tap-highlight-color: transparent;
		touch-action: manipulation;
	}

	.remote {
		min-height: 100dvh;
		display: flex;
		flex-direction: column;
		padding: 0 0 env(safe-area-inset-bottom);
	}

	header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 16px;
		border-bottom: 1px solid #1e1e1e;
		background: #111;
	}
	.logo { font-size: 15px; font-weight: 700; color: #fff; flex: 1; }
	.badge {
		font-size: 11px;
		padding: 3px 8px;
		border-radius: 10px;
		font-weight: 600;
	}
	.badge.ok { background: #1a3a1a; color: #4ade80; }
	.badge.off { background: #2a1a1a; color: #888; }
	.badge.connecting { background: #2a2a1a; color: #fbbf24; }

	.btn-sm {
		font-size: 11px;
		padding: 4px 10px;
		background: #222;
		border: 1px solid #333;
		border-radius: 6px;
		color: #bbb;
		cursor: pointer;
	}

	.error-box {
		margin: 16px;
		padding: 12px 16px;
		background: #2a0a0a;
		border: 1px solid #5a1a1a;
		border-radius: 8px;
		color: #f87171;
		font-size: 14px;
		line-height: 1.4;
	}

	.guide {
		padding: 32px 24px;
		color: #888;
		font-size: 15px;
		line-height: 1.8;
	}
	.guide strong { color: #ccc; }
	.guide ol { margin: 12px 0 0 20px; }
	.guide li { margin-bottom: 6px; }

	.controls {
		flex: 1;
		padding: 12px 16px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.controls.disabled { opacity: 0.4; pointer-events: none; }

	.section {
		background: #111;
		border: 1px solid #1e1e1e;
		border-radius: 12px;
		padding: 14px 16px;
	}
	.label {
		display: block;
		font-size: 11px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: #555;
		margin-bottom: 10px;
	}

	.xfade-row {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.deck-tag {
		font-size: 13px;
		font-weight: 700;
		color: #666;
		min-width: 14px;
	}
	.slider {
		-webkit-appearance: none;
		appearance: none;
		flex: 1;
		height: 40px;
		background: #1a1a1a;
		border-radius: 20px;
		outline: none;
		cursor: pointer;
	}
	.slider::-webkit-slider-thumb {
		-webkit-appearance: none;
		width: 36px;
		height: 36px;
		border-radius: 50%;
		background: #fff;
		cursor: pointer;
	}

	.btn-row {
		display: flex;
		gap: 10px;
	}
	.btn {
		flex: 1;
		padding: 18px 12px;
		font-size: 16px;
		font-weight: 600;
		background: #1a1a1a;
		border: 1px solid #2a2a2a;
		border-radius: 10px;
		color: #ccc;
		cursor: pointer;
		transition: background 0.1s;
		-webkit-tap-highlight-color: transparent;
	}
	.btn:active { background: #2a2a2a; }
	.btn.wide { flex: 1; }
</style>
