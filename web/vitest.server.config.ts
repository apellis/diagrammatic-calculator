// Unit tests of the accounts API (server/, functions/) against Miniflare's D1 and R2. A config of
// its own because the app's Solid plugin makes Vitest resolve packages with the browser condition,
// which would load the browser builds of Miniflare's dependencies.
import { defineConfig } from 'vitest/config';

export default defineConfig({
  // Miniflare starts a workerd process per storage; under load (several test files, other builds)
  // that can take longer than Vitest's default 5 s, so the limits are generous.
  test: { environment: 'node', include: ['server/**/*.test.ts', 'live/**/*.test.ts'], maxWorkers: 1, testTimeout: 30_000, hookTimeout: 60_000 },
});
