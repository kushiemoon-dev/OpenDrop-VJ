import { test, expect, type Page } from '@playwright/test';

// Helper : démarre le visualiseur et attend presets chargés
async function startVisualizer(page: Page) {
	await page.goto('/');
	const startBtn = page.getByRole('button', { name: '▶ Start' });
	await expect(startBtn).toBeVisible();
	await startBtn.click();
	await expect(startBtn).not.toBeVisible({ timeout: 10000 });
	// Attend que les presets soient chargés (butterchurn-presets est lourd)
	await expect(page.locator('.preset-item').first()).toBeVisible({ timeout: 20000 });
}

test.describe('Page principale', () => {
	test('affiche le bouton Start au chargement', async ({ page }) => {
		await page.goto('/');
		await expect(page.getByRole('button', { name: '▶ Start' })).toBeVisible();
		await expect(page.getByText('OpenDrop')).toBeVisible();
	});

	test('les deux canvas sont présents dans le DOM', async ({ page }) => {
		await page.goto('/');
		const canvases = page.locator('canvas.deck-canvas');
		await expect(canvases).toHaveCount(2);
	});

	test('Start lance le visualiseur sans erreur', async ({ page }) => {
		// Surveille les erreurs console
		const errors: string[] = [];
		page.on('console', (msg) => { if (msg.type() === 'error') errors.push(msg.text()); });

		await startVisualizer(page);

		// La sidebar est visible (audio source, mixer, etc.)
		await expect(page.getByText('Audio source')).toBeVisible();
		await expect(page.getByText('Mixer')).toBeVisible();
		expect(errors.filter((e) => !e.includes('favicon'))).toHaveLength(0);
	});
});

test.describe('Mixer', () => {
	test.beforeEach(async ({ page }) => { await startVisualizer(page); });

	test('deck A actif par défaut', async ({ page }) => {
		const deckA = page.locator('.deck-tab').first();
		await expect(deckA).toHaveClass(/active/);
	});

	test('clic deck B bascule le deck actif', async ({ page }) => {
		const deckB = page.locator('.deck-tab').nth(1);
		await deckB.click();
		await expect(deckB).toHaveClass(/active/);
	});

	test('le crossfader est à 0 par défaut', async ({ page }) => {
		const slider = page.locator('.crossfader').first();
		await expect(slider).toHaveValue('0');
	});
});

test.describe('Browser de presets', () => {
	test.beforeEach(async ({ page }) => { await startVisualizer(page); });

	test('la liste de presets est chargée (> 0 items)', async ({ page }) => {
		// startVisualizer attend déjà les presets — on vérifie juste le count
		const count = await page.locator('.preset-item').count();
		expect(count).toBeGreaterThan(0);
	});

	test('la recherche filtre les presets', async ({ page }) => {
		const search = page.locator('.search-input');
		const countBefore = await page.locator('.preset-item').count();

		await search.fill('Geiss');
		await page.waitForTimeout(100);

		const countAfter = await page.locator('.preset-item').count();
		expect(countAfter).toBeLessThan(countBefore);
		// Tous les résultats contiennent "Geiss"
		const names = await page.locator('.preset-item').allTextContents();
		for (const name of names) {
			expect(name.toLowerCase()).toContain('geiss');
		}
	});

	test('clic preset met à jour le deck actif', async ({ page }) => {
		const firstPreset = page.locator('.preset-item').first();
		const name = (await firstPreset.textContent()) ?? '';
		await firstPreset.click();
		// Le preset apparaît dans le tab A
		const deckALabel = page.locator('.deck-tab').first().locator('.deck-preset-name');
		await expect(deckALabel).toContainText(name.split(' - ')[0]);
	});
});

test.describe('Playlist', () => {
	test.beforeEach(async ({ page }) => { await startVisualizer(page); });

	test('ajouter un preset à la playlist A', async ({ page }) => {
		const addBtnA = page.locator('.pl-add').first(); // premier preset, bouton A
		await addBtnA.click();

		// La playlist A doit afficher "1 preset"
		await expect(page.getByText('1 preset').first()).toBeVisible();
	});

	test('supprimer un preset de la playlist', async ({ page }) => {
		// Ajoute d'abord
		await page.locator('.pl-add').first().click();
		await expect(page.getByText('1 preset').first()).toBeVisible();

		// Supprime
		await page.locator('.pl-remove').first().click();
		await expect(page.locator('.pl-deck').first().getByText(/0 presets?/)).toBeVisible();
	});

	test('bouton play actif après ajout + start', async ({ page }) => {
		await page.locator('.pl-add').first().click();
		await page.locator('.pl-add').nth(2).click(); // deuxième preset

		const playBtn = page.locator('.pl-transport .btn-sm').nth(1).first();
		await expect(playBtn).not.toBeDisabled();
		await playBtn.click();
		// Le bouton passe en "⏹"
		await expect(playBtn).toContainText('⏹');
	});

	test('les playlists sont persistées en localStorage', async ({ page }) => {
		await page.locator('.pl-add').first().click();
		const name = await page.locator('.pl-item-name').first().textContent();

		// Rechargement
		await page.reload();
		await startVisualizer(page);

		// Le preset doit toujours être dans la liste
		await expect(page.locator('.pl-item-name').first()).toHaveText(name ?? '');
	});
});

test.describe('Fenêtre output', () => {
	test.beforeEach(async ({ page }) => { await startVisualizer(page); });

	test("le bouton 'Open output window' est actif après Start", async ({ page }) => {
		const btn = page.getByRole('button', { name: /Open output window/i });
		await expect(btn).not.toBeDisabled();
	});

	test('ouvre une nouvelle fenêtre /output', async ({ page, context }) => {
		const [popup] = await Promise.all([
			context.waitForEvent('page'),
			page.getByRole('button', { name: /Open output window/i }).click(),
		]);
		// Vérifie l'URL — le rendu cross-fenêtre est trop fragile en headless
		await popup.waitForLoadState('load', { timeout: 20000 });
		expect(popup.url()).toContain('/output');
		// Vérifie que la page a au moins reçu le JS Svelte (title ou body non vide)
		const bodyClass = await popup.evaluate(() => document.body.innerHTML.length);
		expect(bodyClass).toBeGreaterThan(0);
	});
});
