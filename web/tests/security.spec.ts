import { readdirSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { addCell, cell, newDoc, typeCell } from './helpers';

// The app runs under the Content-Security-Policy of public/_headers (served by `vite preview` as
// by Cloudflare Pages): the engine's WebAssembly and worker, KaTeX's fonts, and the AI panel all
// work, and the browser reports no violation on the way.

/** Collects every Content-Security-Policy violation the page reports, from the event and the console. */
async function watchViolations(page: Page): Promise<() => Promise<string[]>> {
  const console: string[] = [];
  page.on('console', (m) => {
    if (/Content Security Policy|Refused to/i.test(m.text())) console.push(m.text());
  });
  await page.addInitScript(() => {
    const w = window as unknown as { __csp: string[] };
    w.__csp = [];
    document.addEventListener('securitypolicyviolation', (e) => w.__csp.push(`${e.violatedDirective} ${e.blockedURI}`));
  });
  return async () => [...console, ...(await page.evaluate(() => (window as unknown as { __csp: string[] }).__csp))];
}

test('every page and asset carries the security headers', async ({ page, request }) => {
  await page.goto('/');
  // the page, the theme script, and one built asset of each kind (script, style, WebAssembly, font)
  const assets = readdirSync('dist/assets');
  const one = (ext: string) => assets.filter((f) => f.endsWith(ext)).map((f) => `/assets/${f}`).slice(0, 1);
  const paths = ['/', '/theme.js', ...one('.js'), ...one('.css'), ...one('.wasm'), ...one('.woff2')];
  expect(paths.length).toBe(6);
  for (const path of paths) {
    const r = await request.get(path);
    const h = r.headers();
    expect(h['content-security-policy'], path).toContain("frame-ancestors 'none'");
    expect(h['content-security-policy'], path).toContain("object-src 'none'");
    expect(h['content-security-policy'], path).toContain("script-src 'self' 'wasm-unsafe-eval'");
    expect(h['referrer-policy'], path).toBe('no-referrer');
    expect(h['x-content-type-options'], path).toBe('nosniff');
  }
  // the page has no inline script
  expect(await page.locator('script:not([src])').count()).toBe(0);
});

test('the web-app manifest and its icons load under the policy, and Chromium can install the app', async ({ page, request }) => {
  const violations = await watchViolations(page);
  await page.goto('/');
  const href = await page.locator('link[rel="manifest"]').getAttribute('href');
  expect(href).toBe('/manifest.webmanifest');
  const r = await request.get(href!);
  expect(r.ok()).toBe(true);
  expect(r.headers()['content-security-policy']).toContain("default-src 'self'");
  const m = (await r.json()) as { start_url: string; display: string; icons: { src: string; sizes: string; purpose?: string }[] };
  expect(m.start_url).toBe('/');
  expect(m.display).toBe('standalone');
  expect(m.icons.map((i) => i.sizes)).toEqual(expect.arrayContaining(['192x192', '512x512']));
  for (const i of m.icons) {
    const ir = await request.get(i.src);
    expect(ir.ok(), i.src).toBe(true);
    expect(ir.headers()['content-type'], i.src).toMatch(/^image\//);
  }
  // the browser itself reads the manifest without errors and finds nothing in the way of installing
  const cdp = await page.context().newCDPSession(page);
  await expect.poll(async () => ((await cdp.send('Page.getAppManifest')) as { url: string }).url).toContain('/manifest.webmanifest');
  const read = (await cdp.send('Page.getAppManifest')) as { errors: { message: string }[] };
  expect(read.errors).toEqual([]);
  const installability = (await cdp.send('Page.getInstallabilityErrors')) as { installabilityErrors: { errorId: string }[] };
  // (the test browser's private profile is the one thing in the way; a person's browser has none)
  expect(installability.installabilityErrors.map((e) => e.errorId).filter((e) => e !== 'in-incognito')).toEqual([]);
  expect(await violations()).toEqual([]);
});

test('the engine, the typeset results, the theme, and the AI panel work under the policy', async ({ page }) => {
  const violations = await watchViolations(page);
  await page.addInitScript(() => localStorage.setItem('diagcalc.theme', 'dark'));
  await newDoc(page);
  expect(await page.evaluate(() => document.documentElement.dataset.theme)).toBe('dark');

  // the WebAssembly engine, in its worker
  await typeCell(page, 'def a := x_1^2 psi_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  // KaTeX, with its fonts
  await expect(cell(page, 0).getByTestId('cell-display').locator('.katex')).toHaveCount(1);
  await addCell(page, 'watch a');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('3 terms, degree 2');
  await page.evaluate(() => document.fonts.ready);
  await expect
    .poll(() => page.evaluate(() => [...document.fonts].some((f) => f.family.includes('KaTeX') && f.status === 'loaded')))
    .toBe(true);

  // the AI panel reaches the provider
  const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': '*', 'access-control-allow-methods': 'POST, GET, OPTIONS' };
  await page.route('https://api.anthropic.com/**', async (route) => {
    const r = route.request();
    if (r.method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
    if (r.method() === 'GET') return route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify({ data: [{ type: 'model', id: 'claude-opus-5-5', display_name: 'Claude Opus 5.5', created_at: '2026-09-01T00:00:00Z' }], has_more: false }) });
    const ev = (type: string, data: object) => `event: ${type}\ndata: ${JSON.stringify({ type, ...data })}\n\n`;
    const body =
      ev('message_start', { message: { id: 'm', type: 'message', role: 'assistant', model: 'claude-opus-5-5', content: [], stop_reason: null, stop_sequence: null, usage: { input_tokens: 10, output_tokens: 1 } } }) +
      ev('content_block_start', { index: 0, content_block: { type: 'text', text: '' } }) +
      ev('content_block_delta', { index: 0, delta: { type: 'text_delta', text: 'It is \\(a^2\\).' } }) +
      ev('content_block_stop', { index: 0 }) +
      ev('message_delta', { delta: { stop_reason: 'end_turn', stop_sequence: null }, usage: { output_tokens: 5 } }) +
      ev('message_stop', {});
    return route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'text/event-stream' }, body });
  });
  await page.getByTestId('ai-toggle').click();
  await page.getByRole('button', { name: 'I understand' }).click();
  await page.getByTestId('ai-setup-link').click();
  await page.getByLabel('API key').fill('sk-ant-test');
  await page.getByTestId('ai-save').click();
  await expect(page.getByTestId('ai-saved')).toBeVisible();
  await page.getByTestId('settings-back').click();
  await page.getByLabel('Message to the AI').fill('what is a?');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('chat').locator('.ai-text .katex')).toHaveCount(1);

  expect(await violations()).toEqual([]);
  // (and the watch above does see a violation: an inline script is refused)
  await page.evaluate(() => {
    const s = document.createElement('script');
    s.textContent = 'window.__ran = true';
    document.body.append(s);
  });
  await expect.poll(violations).toContainEqual(expect.stringMatching(/^script-src/));
  expect(await page.evaluate(() => (window as unknown as { __ran?: boolean }).__ran)).toBeUndefined();
});
