#!/usr/bin/env node
/**
 * Capture les screenshots utilisés dans README.md.
 *
 * Prérequis : `pnpm dev` doit tourner sur http://localhost:1420
 *
 * Usage :
 *   pnpm dev &          # dans un terminal
 *   node scripts/screenshots.mjs
 */

import { chromium } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(__dirname, '..', 'docs', 'readme-assets');
const BASE_URL = 'http://localhost:1420';

// Clic DOM brut : Playwright's .click() hangs after dispatch on some buttons here
// (its actionability retry loop seems to get confused by an app re-render right
// after the click) -- bypass it entirely, we don't need real-user-input fidelity
// for a screenshot script.
async function rawClick(locator) {
	await locator.waitFor({ state: 'visible' });
	await locator.evaluate((el) => el.click());
}

async function startVisualizer(page) {
	await page.goto(BASE_URL);
	const startBtn = page.getByRole('button', { name: '▶ Start' });
	await rawClick(startBtn);
	await startBtn.waitFor({ state: 'detached', timeout: 10000 }).catch(() => {});
	await rawClick(page.locator('.preset-browser-toggle'));
	await page.locator('.preset-item').first().waitFor({ state: 'visible', timeout: 20000 });
	// Cherche un preset coloré plutôt que le premier de la liste (souvent terne/bruité)
	await page.locator('.search-input').fill('Rainbow Orb');
	await page.locator('.preset-item').first().waitFor({ state: 'visible', timeout: 10000 });
	await rawClick(page.locator('.preset-item').first());
	await page.locator('.search-input').fill('');
	await page.waitForTimeout(2000);
}

async function main() {
	await mkdir(OUT_DIR, { recursive: true });
	const browser = await chromium.launch();
	const context = await browser.newContext({
		viewport: { width: 1600, height: 900 },
		deviceScaleFactor: 2,
		permissions: ['microphone'],
	});
	const page = await context.newPage();

	await startVisualizer(page);

	// 1. Stage layout (hero)
	await rawClick(page.locator('.preset-browser-toggle')); // referme la drawer
	await page.waitForTimeout(500);
	await page.screenshot({ path: join(OUT_DIR, 'stage.png') });
	console.log('stage.png captured');

	// 2. Mixer layout (grid preset browser + sidebar composite/time/qvar panels)
	await rawClick(page.getByRole('button', { name: /Mixer/ }));
	await page.locator('.deck-card').first().waitFor({ state: 'visible', timeout: 10000 });
	await page.waitForTimeout(500);
	await page.screenshot({ path: join(OUT_DIR, 'mixer.png') });
	console.log('mixer.png captured');

	// 3. Compositor panel (cropped to the sidebar section, still in Mixer layout)
	const compositeSection = page.locator('.controls-section', { hasText: 'Composite' }).first();
	await compositeSection.waitFor({ state: 'visible', timeout: 10000 });
	await compositeSection.screenshot({ path: join(OUT_DIR, 'compositor.png') });
	console.log('compositor.png captured');

	// Note: no Output window screenshot. Its canvas reaches "ready" (the cross-window
	// BroadcastChannel handshake completes) but never actually paints a frame under
	// headless Chromium -- confirmed via direct WebGL readPixels, still fully blank
	// after 20s. Matches this project's own e2e test comment that cross-window
	// rendering is "too fragile in headless" -- a real browser renders it fine.

	await browser.close();
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});
