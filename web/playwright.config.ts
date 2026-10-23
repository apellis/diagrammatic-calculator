import { defineConfig, devices } from '@playwright/test';

// CHROMIUM_PATH points at a preinstalled Chromium when the matching Playwright browser is not
// downloaded (for example in a container without network access to the browser CDN).
const executablePath = process.env.CHROMIUM_PATH || undefined;
const port = Number(process.env.E2E_PORT ?? 4173);

export default defineConfig({
  testDir: 'tests',
  timeout: 60_000,
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  // on CI, failures also become annotations of the run, readable without its log
  reporter: process.env.CI ? [['list'], ['github'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: `http://localhost:${port}`,
    trace: 'retain-on-failure',
  },
  projects: [
    { name: 'chromium', grepInvert: /@timing/, use: { ...devices['Desktop Chrome'], launchOptions: { executablePath } } },
    // tests that measure time run alone, once every other test has finished (with --no-deps they
    // run without waiting for the others, for example `npx playwright test --project timing --no-deps`)
    { name: 'timing', grep: /@timing/, dependencies: ['chromium'], workers: 1, use: { ...devices['Desktop Chrome'], launchOptions: { executablePath } } },
  ],
  webServer: {
    command: `npx vite preview --port ${port} --strictPort`,
    port,
    // A different project can occupy the default port. Never test it by accident.
    reuseExistingServer: process.env.E2E_REUSE_SERVER === '1',
  },
});
