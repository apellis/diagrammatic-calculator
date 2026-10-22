/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

// Cross-origin isolation lets the worker pool share memory later; the same headers are in
// public/_headers for Cloudflare Pages.
const isolation = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
};

export default defineConfig({
  plugins: [solid()],
  server: { headers: isolation, fs: { allow: ['..'] } },
  preview: { headers: isolation },
  worker: { format: 'es' },
  build: { target: 'es2022' },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
});
