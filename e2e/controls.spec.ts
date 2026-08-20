import { test, expect, type Page } from '@playwright/test'

async function startVisualizer(page: Page) {
  await page.goto('/')
  const startBtn = page.getByRole('button', { name: '▶ Start' })
  await expect(startBtn).toBeVisible()
  await startBtn.click()
  await expect(startBtn).not.toBeVisible({ timeout: 10000 })
  await page.locator('.preset-browser-toggle').click()
  await expect(page.locator('.preset-item').first()).toBeVisible({ timeout: 20000 })
}

/** Localise une section de la sidebar via le label exact dans .pl-header .label */
function section(page: Page, label: string) {
  return page
    .locator('.controls-section')
    .filter({
      has: page.locator('.pl-header .label', { hasText: label }),
    })
    .first()
}

// ─── Strobe ──────────────────────────────────────────────────────────────────

test.describe('Strobe', () => {
  test.beforeEach(async ({ page }) => {
    await startVisualizer(page)
  })

  test('toggle strobe ON affiche les contrôles', async ({ page }) => {
    const sec = section(page, 'Strobe')
    await sec.scrollIntoViewIfNeeded()
    const toggle = sec.locator('.pl-header .btn-sm')
    await expect(toggle).toHaveText('OFF')

    await toggle.click()
    await expect(toggle).toHaveText('ON')

    // Les contrôles Rate, Intensité, Couleur apparaissent
    await expect(sec.getByText('Rate')).toBeVisible()
    await expect(sec.getByText('Intensity')).toBeVisible()
    await expect(sec.getByText('Color')).toBeVisible()
  })

  test('toggle strobe OFF masque les contrôles', async ({ page }) => {
    const sec = section(page, 'Strobe')
    await sec.scrollIntoViewIfNeeded()
    const toggle = sec.locator('.pl-header .btn-sm')

    await toggle.click() // ON
    await toggle.click() // OFF
    await expect(toggle).toHaveText('OFF')
    await expect(sec.getByText('Rate')).not.toBeVisible()
  })

  test('sélection du rate 2×', async ({ page }) => {
    const sec = section(page, 'Strobe')
    await sec.scrollIntoViewIfNeeded()
    await sec.locator('.pl-header .btn-sm').click() // ON

    const btn2x = sec.getByRole('button', { name: '2×' })
    await btn2x.click()
    await expect(btn2x).toHaveClass(/active/)
  })

  test('slider intensité change la valeur affichée', async ({ page }) => {
    const sec = section(page, 'Strobe')
    await sec.scrollIntoViewIfNeeded()
    await sec.locator('.pl-header .btn-sm').click() // ON

    const slider = sec.locator('input[type="range"]').first()
    await slider.fill('0.8')
    await slider.dispatchEvent('input')
    await expect(sec.getByText('80%')).toBeVisible()
  })
})

// ─── LFO ─────────────────────────────────────────────────────────────────────

test.describe('LFO', () => {
  test.beforeEach(async ({ page }) => {
    await startVisualizer(page)
  })

  test('activer LFO 1 affiche les contrôles', async ({ page }) => {
    const sec = section(page, 'LFO')
    await sec.scrollIntoViewIfNeeded()

    const checkbox = sec.locator('input[type="checkbox"]').first()
    await checkbox.check()
    await expect(checkbox).toBeChecked()

    // Les sliders Rate et Amount apparaissent
    await expect(sec.getByText('Rate').first()).toBeVisible()
    await expect(sec.getByText('Amount')).toBeVisible()
  })

  test('changer shape sine → saw', async ({ page }) => {
    const sec = section(page, 'LFO')
    await sec.scrollIntoViewIfNeeded()

    // Activer LFO 1
    await sec.locator('input[type="checkbox"]').first().check()

    const sawBtn = sec.getByRole('button', { name: 'saw' }).first()
    await sawBtn.click()
    await expect(sawBtn).toHaveClass(/active/)

    // sine ne doit plus être actif
    const sineBtn = sec.getByRole('button', { name: 'sine' }).first()
    await expect(sineBtn).not.toHaveClass(/active/)
  })

  test('désactiver LFO 1 masque les contrôles', async ({ page }) => {
    const sec = section(page, 'LFO')
    await sec.scrollIntoViewIfNeeded()

    const checkbox = sec.locator('input[type="checkbox"]').first()
    await checkbox.check()
    await expect(sec.getByText('Amount')).toBeVisible()

    await checkbox.uncheck()
    await expect(sec.getByText('Amount')).not.toBeVisible()
  })

  test('sélectionner une cible dans le select', async ({ page }) => {
    const sec = section(page, 'LFO')
    await sec.scrollIntoViewIfNeeded()

    await sec.locator('input[type="checkbox"]').first().check()
    const select = sec.locator('select').first()
    // Sélectionner la première option non-vide
    const options = await select.locator('option').allTextContents()
    const firstReal = options.find((o) => o.trim() !== '—')
    if (firstReal) {
      await select.selectOption({ label: firstReal.trim() })
      await expect(select).not.toHaveValue('')
    }
  })
})

// ─── Color controls ──────────────────────────────────────────────────────────

test.describe('Color controls', () => {
  test.beforeEach(async ({ page }) => {
    await startVisualizer(page)
  })

  test('slider Hue Color A met à jour la valeur affichée', async ({ page }) => {
    const sec = section(page, 'Color A')
    await sec.scrollIntoViewIfNeeded()

    // Premier slider = Hue (hueRotate)
    const hueSlider = sec.locator('input[type="range"]').first()
    await hueSlider.fill('0.5')
    await hueSlider.dispatchEvent('input')
    // 0.5 × 360 = 180°
    await expect(sec.getByText('180°')).toBeVisible()
  })

  test('reset Color A remet les valeurs par défaut', async ({ page }) => {
    const sec = section(page, 'Color A')
    await sec.scrollIntoViewIfNeeded()

    // Modifier Hue
    const hueSlider = sec.locator('input[type="range"]').first()
    await hueSlider.fill('0.5')
    await hueSlider.dispatchEvent('input')
    await expect(sec.getByText('180°')).toBeVisible()

    // Reset ↺ — cibler le pl-header "Color A" spécifiquement (Color B a aussi un .btn-sm)
    const resetBtn = sec.locator('.pl-header').filter({ hasText: 'Color A' }).locator('.btn-sm')
    await resetBtn.click()
    // Valeur par défaut = 0°
    await expect(sec.getByText('0°').first()).toBeVisible()
  })

  test('slider Sat Color B met à jour la valeur', async ({ page }) => {
    const colorASec = section(page, 'Color A')
    await colorASec.scrollIntoViewIfNeeded()

    // Color B est dans la même section (même controls-section)
    const colorBHeader = page.locator('.pl-header').filter({ hasText: 'Color B' })
    await colorBHeader.scrollIntoViewIfNeeded()

    // Sat = deuxième slider dans Color B → on cherche par position relative
    const satSlider = colorBHeader.locator('~ div input[type="range"]').first()
    await satSlider.fill('0.5')
    await satSlider.dispatchEvent('input')
    // 0.5 × 200 = 100%
    await expect(page.getByText('100%').first()).toBeVisible()
  })
})

// ─── Keyboard learn ───────────────────────────────────────────────────────────

test.describe('Keyboard learn', () => {
  test.beforeEach(async ({ page }) => {
    await startVisualizer(page)
  })

  test('clic Learn active le mode apprentissage', async ({ page }) => {
    const sec = section(page, 'Keyboard')
    await sec.scrollIntoViewIfNeeded()

    // .pl-btn est la classe stable du bouton Learn (distinct du "Reset" .pl-header .btn-sm)
    const learnBtn = sec.locator('.pl-btn').first()
    await learnBtn.click()
    // Le bouton passe en "…"
    await expect(learnBtn).toHaveText('…')
  })

  test('Escape annule le learn', async ({ page }) => {
    const sec = section(page, 'Keyboard')
    await sec.scrollIntoViewIfNeeded()

    const learnBtn = sec.locator('.pl-btn').first()
    await learnBtn.click()
    await expect(learnBtn).toHaveText('…')

    await page.keyboard.press('Escape')
    await expect(learnBtn).toHaveText('Learn')
  })
})

// ─── Page /remote ─────────────────────────────────────────────────────────────

test.describe('Page /remote', () => {
  test('charge sans params et affiche le guide de connexion', async ({ page }) => {
    await page.goto('/remote')
    // Sans params → affiche le guide (.guide), pas un message d'erreur
    await expect(page.getByText(/To use the remote/i)).toBeVisible({ timeout: 10000 })
  })

  test('avec params invalides affiche erreur de connexion', async ({ page }) => {
    await page.goto('/remote?host=127.0.0.1&port=9999&token=fake')
    // Tente de se connecter — WebSocket vers un port fermé → ws.onerror → affiche l'erreur
    await expect(page.getByText(/Connection failed/i)).toBeVisible({ timeout: 15000 })
  })

  test('crossfader est visible avec params (même déconnecté)', async ({ page }) => {
    await page.goto('/remote?host=127.0.0.1&port=9999&token=fake')
    // Les contrôles sont rendus dès que host+port+token sont présents, même si déconnecté
    const xfader = page.locator('.xfade')
    await expect(xfader).toBeVisible({ timeout: 10000 })
  })
})
