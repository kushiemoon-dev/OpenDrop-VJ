import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  fullyParallel: false,
  retries: 0,
  use: {
    baseURL: 'http://localhost:1420',
    headless: true,
    // Permission micro nécessaire pour AudioContext
    permissions: ['microphone'],
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  // Lance le dev server avant les tests
  webServer: {
    command: 'node_modules/.bin/vite dev --port 1420',
    url: 'http://localhost:1420',
    reuseExistingServer: true,
    timeout: 30_000,
  },
})
