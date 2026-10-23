// Unit tests of the accounts API (server/, functions/) against Miniflare's D1 and R2. A config of
// its own because the app's Solid plugin makes Vitest resolve packages with the browser condition,
// which would load the browser builds of Miniflare's dependencies.
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: { environment: 'node', include: ['server/**/*.test.ts'] },
});
