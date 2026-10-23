/// <reference types="vitest/config" />
import { readFileSync } from 'node:fs';
import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

/** The headers public/_headers gives every file (its `/*` rule), as Cloudflare Pages serves them. */
export function pagesHeaders(): Record<string, string> {
  const out: Record<string, string> = {};
  let inRule = false;
  for (const line of readFileSync(new URL('public/_headers', import.meta.url), 'utf8').split('\n')) {
    if (!line.trim() || line.trimStart().startsWith('#')) continue;
    if (!/^\s/.test(line)) {
      inRule = line.trim() === '/*';
      continue;
    }
    const i = line.indexOf(':');
    if (inRule && i > 0) out[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  return out;
}

const headers = pagesHeaders();
// The development server keeps cross-origin isolation only: its module reloading needs more
// than the policy allows. `vite preview` (and so the browser tests) serves all of them.
const isolation = { 'Cross-Origin-Opener-Policy': headers['Cross-Origin-Opener-Policy'], 'Cross-Origin-Embedder-Policy': headers['Cross-Origin-Embedder-Policy'] };

export default defineConfig({
  plugins: [solid()],
  server: { headers: isolation, fs: { allow: ['..'] } },
  preview: { headers },
  worker: { format: 'es' },
  build: { target: 'es2022' },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
});
