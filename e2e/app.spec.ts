import { test, expect, type Page } from '@playwright/test'

// Helper : démarre le visualiseur et ouvre le browser de presets
async function startVisualizer(page: Page) {
  await page.goto('/')
  const startBtn = page.getByRole('button', { name: '▶ Start' })
  await expect(startBtn).toBeVisible()
  await startBtn.click()
  await expect(startBtn).not.toBeVisible({ timeout: 10000 })
  // Ouvrir la preset drawer (fermée par défaut — translateY(100%) sinon)
  await page.locator('.preset-browser-toggle').click()
  // Attend que les presets soient chargés dans la drawer ouverte
  await expect(page.locator('.preset-item').first()).toBeVisible({ timeout: 20000 })
}

test.describe('Page principale', () => {
  test('affiche le bouton Start au chargement', async ({ page }) => {
    await page.goto('/')
    await expect(page.getByRole('button', { name: '▶ Start' })).toBeVisible()
    await expect(page.getByText('OpenDrop')).toBeVisible()
  })

  test('les canvas de rendu sont présents après Start', async ({ page }) => {
    // Les canvas sont créés dans onMount des Deck — ils n'existent pas avant Start
    await startVisualizer(page)
    const canvases = page.locator('canvas.deck-canvas')
    const count = await canvases.count()
    expect(count).toBeGreaterThanOrEqual(2)
  })

  test('Start lance le visualiseur sans erreur', async ({ page }) => {
    // Surveille les erreurs console
    const errors: string[] = []
    page.on('console', (msg) => {
      if (msg.type() === 'error') errors.push(msg.text())
    })

    await startVisualizer(page)

    // La sidebar est visible (audio source, mixer, etc.)
    await expect(page.getByText('Audio source')).toBeVisible()
    // Deux éléments contiennent "Mixer" (bouton + label) — on prend le premier
    await expect(page.getByText('Mixer').first()).toBeVisible()
    expect(errors.filter((e) => !e.includes('favicon'))).toHaveLength(0)
  })
})

test.describe('Mixer', () => {
  test.beforeEach(async ({ page }) => {
    await startVisualizer(page)
  })

  test('deck A actif par défaut', async ({ page }) => {
    // Le composant DeckCard utilise .deck-card et .deck-card--active
    const deckA = page.locator('.deck-card').first()
    await expect(deckA).toHaveClass(/deck-card--active/)
  })

  test('clic deck B bascule le deck actif', async ({ page }) => {
    const deckB = page.locator('.deck-card').nth(1)
    await deckB.click()
    await expect(deckB).toHaveClass(/deck-card--active/)
  })

  test('le crossfader est à 0 par défaut', async ({ page }) => {
    const slider = page.locator('.crossfader').first()
    await expect(slider).toHaveValue('0')
  })
})

test.describe('Browser de presets', () => {
  test.beforeEach(async ({ page }) => {
    await startVisualizer(page)
  })

  test('la liste de presets est chargée (> 0 items)', async ({ page }) => {
    // startVisualizer attend déjà les presets — on vérifie juste le count
    const count = await page.locator('.preset-item').count()
    expect(count).toBeGreaterThan(0)
  })

  test('la recherche filtre les presets', async ({ page }) => {
    const search = page.locator('.search-input')

    await search.fill('Geiss')
    // Débounce = 150 ms — on attend 500 ms pour être sûr
    await page.waitForTimeout(500)

    // Le compteur du drawer (.preset-drawer__count) doit montrer un sous-ensemble
    const countText = await page.locator('.preset-drawer__count').textContent()
    const nums = (countText ?? '').match(/\d+/g)?.map(Number)
    if (nums && nums.length >= 2) {
      expect(nums[0]).toBeLessThan(nums[1])
    }

    // Les items visibles contiennent tous "Geiss" (virtualscroll)
    const names = await page.locator('.preset-item').allTextContents()
    expect(names.length).toBeGreaterThan(0)
    for (const name of names) {
      expect(name.toLowerCase()).toContain('geiss')
    }
  })

  test('clic preset met à jour le deck actif', async ({ page }) => {
    const firstPreset = page.locator('.preset-item').first()
    const fullName = (await firstPreset.textContent()) ?? ''
    // force:true contourne l'instabilité du virtualscroll pendant le click
    await firstPreset.click({ force: true })
    // .deck-card__name affiche le nom court (après le dernier '/')
    const shortName = (fullName.split('/').at(-1) ?? fullName).split(' - ')[0]
    const deckALabel = page.locator('.deck-card').first().locator('.deck-card__name')
    await expect(deckALabel).toContainText(shortName, { timeout: 5000 })
  })
})

test.describe('Playlist', () => {
  test.beforeEach(async ({ page }) => {
    await startVisualizer(page)
  })

  test('ajouter un preset à la playlist A', async ({ page }) => {
    const addBtnA = page.locator('.pl-add').first() // premier preset, bouton A
    await addBtnA.click()

    // La playlist A doit afficher "1 preset"
    await expect(page.getByText('1 preset').first()).toBeVisible()
  })

  test('supprimer un preset de la playlist', async ({ page }) => {
    // Ajoute d'abord
    await page.locator('.pl-add').first().click()
    await expect(page.getByText('1 preset').first()).toBeVisible()

    // Supprime
    await page.locator('.pl-remove').first().click()
    await expect(
      page
        .locator('.pl-deck')
        .first()
        .getByText(/0 presets?/)
    ).toBeVisible()
  })

  test('bouton play actif après ajout + start', async ({ page }) => {
    await page.locator('.pl-add').first().click()
    await page.locator('.pl-add').nth(2).click() // deuxième preset

    const playBtn = page.locator('.pl-transport .btn-sm').nth(1)
    await expect(playBtn).not.toBeDisabled()
    await playBtn.click()
    // Le bouton passe en "⏹"
    await expect(playBtn).toContainText('⏹')
  })

  test('les playlists sont persistées en localStorage', async ({ page }) => {
    await page.locator('.pl-add').first().click()
    const name = await page.locator('.pl-item-name').first().textContent()

    // Rechargement
    await page.reload()
    await startVisualizer(page)

    // Le preset doit toujours être dans la liste
    await expect(page.locator('.pl-item-name').first()).toHaveText(name ?? '')
  })
})

test.describe('Fenêtre output', () => {
  test.beforeEach(async ({ page }) => {
    await startVisualizer(page)
  })

  test("le bouton 'Open output window' est actif après Start", async ({ page }) => {
    const btn = page.getByRole('button', { name: /Open output window/i })
    await expect(btn).not.toBeDisabled()
  })

  test('ouvre une nouvelle fenêtre /output', async ({ page, context }) => {
    // La preset drawer couvre le bouton — la fermer et attendre qu'elle soit vraiment fermée
    await page.locator('.preset-browser-toggle').click()
    await expect(page.locator('.preset-drawer--open')).toHaveCount(0)
    const [popup] = await Promise.all([
      context.waitForEvent('page'),
      page.getByRole('button', { name: /Open output window/i }).click(),
    ])
    // Vérifie l'URL — le rendu cross-fenêtre est trop fragile en headless
    await popup.waitForLoadState('load', { timeout: 20000 })
    expect(popup.url()).toContain('/output')
    // Vérifie que la page a au moins reçu le JS Svelte (title ou body non vide)
    const bodyClass = await popup.evaluate(() => document.body.innerHTML.length)
    expect(bodyClass).toBeGreaterThan(0)
  })

  test("l'overlay de diagnostic audio est présent sur la page /output", async ({
    page,
    context,
  }) => {
    // La preset drawer couvre le bouton — la fermer et attendre qu'elle soit vraiment fermée
    await page.locator('.preset-browser-toggle').click()
    await expect(page.locator('.preset-drawer--open')).toHaveCount(0)
    const [popup] = await Promise.all([
      context.waitForEvent('page'),
      page.getByRole('button', { name: /Open output window/i }).click(),
    ])
    await popup.waitForLoadState('load', { timeout: 20000 })
    expect(popup.url()).toContain('/output')
    // L'overlay diagnostique est rendu dès que Svelte monte le composant
    // (le SPA hydrate après load → timeout généreux)
    await expect(popup.locator('.diag-overlay')).toBeVisible({ timeout: 15000 })
    await expect(popup.locator('.diag-overlay')).toContainText('PCM rx:0')
    await expect(popup.locator('.diag-overlay')).toContainText('acq:N')
  })
})
