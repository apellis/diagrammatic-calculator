import { defineConfig, devices } from '@playwright/test';

// CHROMIUM_PATH points at a preinstalled Chromium when the matching Playwright browser is not
// downloaded (for example in a container without network access to the browser CDN).
const executablePath = process.env.CHROMIUM_PATH || undefined;

export default defineConfig({
  testDir: 'tests',
  timeout: 60_000,
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  // on CI, failures also become annotations of the run, readable without its log
  reporter: process.env.CI ? [['list'], ['github'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: 'http://localhost:4173',
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], launchOptions: { executablePath } } }],
  webServer: {
    command: 'npx vite preview --port 4173 --strictPort',
    port: 4173,
    reuseExistingServer: !process.env.CI,
  },
});
