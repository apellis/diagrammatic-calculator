/// <reference types="vitest/config" />
import { execSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { defineConfig, type Plugin } from 'vite';
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

/** The full hash of the commit being built (GitHub's on CI), or `unknown` outside a checkout. */
function commit(): string {
  const sha = process.env.GITHUB_SHA;
  if (sha) return sha;
  try {
    return execSync('git rev-parse HEAD', { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
  } catch {
    return 'unknown';
  }
}

const buildCommit = commit();
/** The About page shows the short hash. */
const shortCommit = buildCommit === 'unknown' ? buildCommit : buildCommit.slice(0, 7);

/**
 * `/build-info.json`: the full commit hash of this build, which the deploy workflow reads back from
 * the production site (.github/scripts/check_deployed.py) to confirm it serves what was deployed.
 */
function buildInfo(): Plugin {
  return {
    name: 'build-info',
    generateBundle() {
      this.emitFile({ type: 'asset', fileName: 'build-info.json', source: `${JSON.stringify({ commit: buildCommit })}\n` });
    },
  };
}

const escapeHtml = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/**
 * `/privacy`: the repository's PRIVACY.md as a static page, so the policy has an address on this
 * site of its own. Only a title and plain paragraphs are read; anything else stops the build, so the
 * page never shows the policy differently from the file.
 */
export function privacyPage(): string {
  const src = readFileSync(new URL('../PRIVACY.md', import.meta.url), 'utf8');
  const blocks = src.trim().split(/\n\s*\n/);
  const body = blocks.map((b, i) => {
    const text = b.replace(/\s*\n\s*/g, ' ').trim();
    if (i === 0 && text.startsWith('# ')) return `<h1>${escapeHtml(text.slice(2))}</h1>`;
    if (/^[#>*\-+|`\d]|\[|\]\(|[*_`]/.test(text)) throw new Error(`PRIVACY.md: unsupported Markdown in "${text.slice(0, 40)}"`);
    return `<p>${escapeHtml(text)}</p>`;
  });
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Privacy policy · diagrammatic-calculator</title>
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<style>
:root { color-scheme: light dark; }
body { max-width: 640px; margin: 0 auto; padding: 32px 18px; font: 16px/1.55 system-ui, sans-serif; background: Canvas; color: CanvasText; }
h1 { font-size: 24px; }
</style>
</head>
<body>
${body.join('\n')}
<p><a href="/">diagrammatic-calculator</a></p>
</body>
</html>
`;
}

function privacy(): Plugin {
  return {
    name: 'privacy-page',
    generateBundle() {
      this.emitFile({ type: 'asset', fileName: 'privacy.html', source: privacyPage() });
    },
  };
}

export default defineConfig({
  plugins: [solid(), privacy(), buildInfo()],
  define: { __COMMIT__: JSON.stringify(shortCommit) },
  server: { headers: isolation, fs: { allow: ['..'] } },
  preview: { headers },
  worker: { format: 'es' },
  build: { target: 'es2022' },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
});
