import { expect, test } from '@playwright/test';
import { addCell, cell, newDoc, timeUntil, typeCell } from './helpers';

test('a definition and a watch: values, diffs, and undo from the cache', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iji)');
  await addCell(page, 'watch a a + x_2 e(iji)');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('2 terms, degrees 2 to 4');
  await expect(cell(page, 1).locator('.summand')).toHaveCount(2);

  // editing the definition recomputes the watch, and the new summands are outlined
  await cell(page, 0).locator('.head').click();
  await page.locator(`#cell-${await cell(page, 0).getAttribute('data-cell')} .cm-content`).click();
  await page.keyboard.press('ControlOrMeta+a');
  await typeCell(page, 'def a := x_3 e(iji)');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('2 terms, degrees 2 to 4');
  await cell(page, 1).locator('.head').click();
  await expect(cell(page, 1).locator('.summand.new')).toHaveCount(1);
  await expect(cell(page, 1).locator('.changed summary')).toHaveText('Changed: 1 new, 1 gone, 0 with a new coefficient');
  await expect(cell(page, 1).getByTestId('textform')).toHaveAttribute('data-text', 'x_2 e(iji) + x_3^2 e(iji)');

  // undo with the button: the earlier values come back from the cache
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(cell(page, 1).getByTestId('textform')).toHaveAttribute('data-text', 'x_2 e(iji) + x_1^2 e(iji)');
  await page.getByRole('button', { name: 'Redo' }).click();
  await expect(cell(page, 1).getByTestId('textform')).toHaveAttribute('data-text', 'x_2 e(iji) + x_3^2 e(iji)');
});

test('solve and check cells, and parse errors', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def e := c1 e(iji) + c2 psi_1 psi_2 psi_1 e(iji)');
  await addCell(page, 'solve e e - e = 0 for c1, c2');
  await addCell(page, 'check idempotent(psi_1 psi_2 psi_1 e(iji))');
  await addCell(page, 'check idempotent(psi_1 e(iji))');
  // text that does not parse is not written: the editor stays open and says why
  await page.getByTestId('add-cell').click();
  await page.keyboard.type('watch psi_1 +');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('cell-problem')).toContainText('Esc');
  await expect(page.locator('.editor.invalid')).toHaveCount(1);
  await page.keyboard.press('Escape');
  await expect(cell(page, 4)).toHaveCount(0);
  // one that parses but names nothing known is written, and its error shown
  await addCell(page, 'watch foo_1');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('4 solutions');
  await expect(cell(page, 2).getByTestId('outcome')).toHaveText('holds');
  await expect(cell(page, 3).getByTestId('outcome')).toHaveText('fails');
  await expect(cell(page, 4).getByTestId('summary')).toHaveText('error');
  await cell(page, 1).locator('.head').click();
  await expect(cell(page, 1).getByTestId('outcome')).toHaveText('4 solutions');
  await expect(cell(page, 1).locator('.kv .value[data-text="(c1 = 1, c2 = -1)"]')).toHaveCount(1);
  await cell(page, 4).locator('.head').click();
  await expect(cell(page, 4).locator('.error')).toBeVisible();
});

test('a bare term becomes a definition, and / offers the cell kinds', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'psi_1 e(iji)');
  await expect(cell(page, 0)).toHaveAttribute('data-kind', 'def');
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 't1 := psi_1 e(iji)');
  await page.getByTestId('add-cell').click();
  await page.keyboard.type('/wa');
  await page.locator('.kindmenu').getByRole('button', { name: 'watch', exact: true }).click();
  await typeCell(page, 't1');
  await expect(cell(page, 1)).toHaveAttribute('data-kind', 'watch');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term, degree 1');
});

test('changing the context recomputes every cell, and a bad context is refused', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'watch x_1 psi_1 - psi_1 x_2');
  await expect(cell(page, 0).getByTestId('textform')).toHaveAttribute('data-text', 'e(iij)');
  await page.getByTestId('context').click();
  const input = page.getByLabel('Context');
  await input.fill('nilhecke nu=3 ring=GF(5)');
  await input.press('Enter');
  await expect(page.getByTestId('context').locator('.ctx')).toHaveAttribute('data-text', 'nilhecke nu=3 ring=GF(5)');
  await expect(cell(page, 0).getByTestId('textform')).toHaveAttribute('data-text', '1');
  await page.getByTestId('context').click();
  await page.getByLabel('Context').fill('klr type=Z9');
  await page.getByLabel('Context').press('Enter');
  await expect(page.locator('.pop .error')).toBeVisible();
});

test('history: versions, read-only view, and restore', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'watch x_1 e(ij)');
  await addCell(page, 'watch x_2 e(ij)');
  await page.getByRole('tab', { name: 'History' }).click();
  const items = page.locator('.outline .oitem');
  await expect(items.first()).toHaveAttribute('data-note', 'add watch x_2 e(ij)');
  // the version before the second cell was added
  await items.nth(1).click();
  await expect(page.getByRole('dialog')).toContainText('watch x_1 e(ij)');
  // compared with the document now: the second cell is the one line added since
  const diff = page.getByTestId('version-diff');
  await expect(diff.locator('.add')).toHaveCount(1);
  await expect(diff.locator('.add')).toContainText('watch x_2 e(ij)');
  await expect(diff.locator('.del')).toHaveCount(0);
  await page.getByRole('button', { name: 'Restore this version' }).click();
  await expect(page.locator('.cell[data-cell]')).toHaveCount(1);
  await expect(items.first()).toHaveAttribute('data-note', /^restore version/);
});

test('documents persist across reloads, and the list can rename, duplicate, and delete', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'watch x_1 x_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term, degree 4');
  await page.reload();
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term, degree 4');
  await page.goto('/');
  await page.getByRole('button', { name: 'Rename' }).first().click();
  await page.getByLabel('Document name').fill('Dots');
  await page.getByLabel('Document name').press('Enter');
  await expect(page.locator('td.open').first()).toHaveText('Dots');
  await page.getByRole('button', { name: 'Duplicate' }).first().click();
  await expect(page.locator('td.open')).toHaveText(['Dots (copy)', 'Dots']);
  await page.getByRole('button', { name: 'Delete' }).first().click();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(page.locator('td.open')).toHaveText(['Dots']);
});

test('export to .dcw and import it back', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a:=psi_1   e(iji)');
  await addCell(page, 'check idempotent(a a)');
  await expect(cell(page, 1).getByTestId('verdict')).toHaveText('holds');
  await page.keyboard.press('Escape');
  const download = page.waitForEvent('download');
  await page.keyboard.press('ControlOrMeta+Shift+S');
  const file = await download;
  const text = await (await file.createReadStream()).toArray().then((b) => Buffer.concat(b).toString());
  expect(text).toBe('diagcalc-workspace/1\ncontext klr type=A2 nu=2i+j ring=QQ\ndef a := psi_1 e(iji)  # id=df1\ncheck idempotent(a a)  # id=ck2\n');
  await page.goto('/');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Import .dcw or .dch…' }).click();
  await (await chooser).setFiles({ name: 'mine.dcw', mimeType: 'text/plain', buffer: Buffer.from(text) });
  await expect(page.locator('.topbar .name')).toHaveText('mine');
  await expect(cell(page, 1).getByTestId('outcome')).toHaveText('holds');
});

test('a history file (.dch) imports as a document with its versions, snapshots, and AI timeline', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'watch x_1 psi_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('2 terms, degree 0');
  await cell(page, 0).getByRole('button', { name: 'Pin result' }).click();
  await page.getByLabel('Snapshot name').fill('first');
  await page.getByLabel('Snapshot name').press('Enter');
  await addCell(page, 'def a := x_2 e(iij)');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term');
  await page.keyboard.press('Escape');
  await page.keyboard.press('ControlOrMeta+k');
  await page.keyboard.type('Export history');
  const download = page.waitForEvent('download');
  await page.keyboard.press('Enter');
  const text = await (await (await download).createReadStream()).toArray().then((b) => Buffer.concat(b).toString());
  const exported = JSON.parse(text) as { versions: { note: string; author: string }[] };
  expect(exported.versions.length).toBeGreaterThanOrEqual(2);

  await page.goto('/');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Import .dcw or .dch…' }).click();
  await (await chooser).setFiles({ name: 'history.dch', mimeType: 'application/json', buffer: Buffer.from(text) });
  await expect(page.locator('.topbar .name')).toHaveText('Untitled');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term');
  // every version is there again with its note
  await page.getByRole('tab', { name: 'History' }).click();
  await expect(page.locator('.outline .oitem')).toHaveCount(exported.versions.length);
  await expect(page.locator(`.outline .oitem[data-note=${JSON.stringify(exported.versions[0].note)}]`)).toHaveCount(1);
  await page.getByRole('tab', { name: 'Snapshots' }).click();
  await expect(page.locator('.outline .oitem', { hasText: 'first' })).toHaveCount(1);
  // new cells take numbers the history never used
  await addCell(page, 'watch a');
  await expect(cell(page, 2)).toHaveAttribute('data-cell', 'wt3');
});

test('a document linked to a file goes on saving to it after a reload, once the browser allows it again', async ({ page }) => {
  // the picker hands out a file of the page's private file system; a flag makes the browser ask
  // for permission again, as it does on a new visit
  await page.addInitScript(() => {
    const w = window as unknown as { showSaveFilePicker: () => Promise<FileSystemFileHandle> };
    w.showSaveFilePicker = async () => (await navigator.storage.getDirectory()).getFileHandle('linked.dcw', { create: true });
    if (sessionStorage.getItem('ask-again')) {
      const p = FileSystemHandle.prototype as unknown as Record<string, unknown>;
      let granted = false;
      p.queryPermission = async () => (granted ? 'granted' : 'prompt');
      p.requestPermission = async () => ((granted = true), 'granted');
    }
  });
  const read = () => page.evaluate(async () => (await (await (await navigator.storage.getDirectory()).getFileHandle('linked.dcw')).getFile()).text());
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await page.keyboard.press('ControlOrMeta+k');
  await page.keyboard.type('Save to a file on disk');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('linked')).toHaveText('↔ linked.dcw');
  await expect.poll(read).toContain('def a := x_1 e(iij)  # id=df1');

  // a reload keeps the link, and saving goes on
  await page.reload();
  await expect(page.getByTestId('linked')).toHaveText('↔ linked.dcw');
  await addCell(page, 'watch a');
  await expect.poll(read).toContain('watch a  # id=wt2');

  // when the browser asks again, saving pauses until the person allows it
  await page.evaluate(() => sessionStorage.setItem('ask-again', '1'));
  await page.reload();
  await expect(page.getByTestId('relink')).toContainText('linked.dcw');
  await addCell(page, 'check zero(a)');
  await expect(cell(page, 2).getByTestId('summary')).not.toHaveText('');
  expect(await read()).not.toContain('check zero(a)');
  await page.getByTestId('relink').click();
  await expect(page.getByTestId('linked')).toHaveText('↔ linked.dcw');
  await expect.poll(read).toContain('check zero(a)  # id=ck3');

  // unlinked, it stays unlinked after a reload
  await page.keyboard.press('ControlOrMeta+k');
  await page.keyboard.type('Stop saving to the file');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('linked')).toHaveCount(0);
  await page.reload();
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await expect(page.getByTestId('linked')).toHaveCount(0);
  await expect(page.getByTestId('relink')).toHaveCount(0);
});

test('pinning a result keeps a snapshot', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'watch x_1 psi_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('2 terms, degree 0');
  await cell(page, 0).getByRole('button', { name: 'Pin result' }).click();
  await page.getByLabel('Snapshot name').fill('first');
  await page.getByLabel('Snapshot name').press('Enter');
  await page.getByRole('tab', { name: 'Snapshots' }).click();
  await page.locator('.outline .oitem', { hasText: 'first' }).click();
  await expect(page.getByRole('dialog').or(page.locator('.modal'))).toContainText('"kind": "watch"');
  await page.getByTestId('pinned-recompute').getByRole('button', { name: 'Recompute', exact: true }).click();
  await expect(page.getByTestId('pinned-recompute')).toContainText('Matches the saved value and provenance.');
});

test('the gate example: idempotents of a cyclotomic quotient', async ({ page }) => {
  await page.goto('/');
  await page.locator('[data-example="klr-a2-idempotents"]').click();
  // collapsed checks show their verdict once, in the pill beside the kind, not in the summary
  const summaries = page.locator('.cell[data-cell] [data-testid=summary]');
  await expect(summaries).toHaveText(['', '2 terms', '2 terms, degree 0', '4 solutions', '1 term', '2 terms', '1 term, degree 0'], { timeout: 30_000 });
  await expect(page.locator('.cell[data-cell] [data-testid=outcome]')).toHaveText(['holds', 'holds', 'holds', 'holds', 'holds', 'holds']);
  // the quotient's notes under the badge are typeset
  const watch = page.locator('.cell[data-cell]').nth(2);
  await watch.locator('.head').click();
  await expect(watch.getByTestId('prov-note').first().locator('.katex').first()).toBeVisible();
  await expect(watch.getByTestId('prov-note').first()).not.toContainText('<h_(i_1)');
  // a definition's outline row shows its term after the name
  await expect(page.locator('.outline .oitem .t[data-text*=":= "]').first()).toBeVisible();
});

test('an idempotent outside the weight ν is named in a warning, not a silent 0', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'watch e(ii) + x_1 e(iji)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term, degree 2');
  await expect(cell(page, 0).getByTestId('warn-mark')).toBeVisible();
  const warning = cell(page, 0).getByTestId('warning');
  await expect(warning).toContainText('here: its object is not one of weight');
  await expect(warning.locator('.katex')).toHaveCount(3);
  // a term within the weight has no warning
  await addCell(page, 'watch x_1 e(iji)');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term, degree 2');
  await expect(cell(page, 1).getByTestId('warn-mark')).toHaveCount(0);
});

// Timed in a project of its own that runs after all the others (playwright.config.ts), so other
// tests do not compete for the processor while it measures.
test('latency: edits and cached watches meet the interactive target', { tag: '@timing' }, async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  await addCell(page, 'watch a a a');
  const w = cell(page, 1).getByTestId('summary');
  const first = '1 term, degree 6';
  await expect(w).toHaveText(first);
  const id = await cell(page, 0).getAttribute('data-cell');
  await page.locator(`#cell-${id} .head`).click();
  const editor = page.locator(`#cell-${id} .cm-content`);
  await editor.click();
  // Preview recomputes on input, before Enter. Start the clock at the actual edit.
  const second = '3 terms, degrees 4 to 6';
  const computed = await timeUntil(page, 1, second, 'input', () => editor.fill('def a := (x_1 + x_2) psi_1 e(iij) + x_3 e(iij)'));
  await editor.press('Enter');
  await expect(page.getByTestId('draft-preview')).toHaveCount(0);
  // undo and redo: the values come from the cache
  await page.locator('.main').click({ position: { x: 5, y: 5 } });
  // one round first, not timed: the first undo and redo also load the cached results into memory
  await timeUntil(page, 1, first, 'click', () => page.getByRole('button', { name: 'Undo' }).click());
  await timeUntil(page, 1, second, 'click', () => page.getByRole('button', { name: 'Redo' }).click());
  const undo: number[] = [];
  const redo: number[] = [];
  for (let k = 0; k < 5; k++) {
    undo.push(await timeUntil(page, 1, first, 'click', () => page.getByRole('button', { name: 'Undo' }).click()));
    redo.push(await timeUntil(page, 1, second, 'click', () => page.getByRole('button', { name: 'Redo' }).click()));
  }
  const worst = Math.max(...undo, ...redo);
  console.log(`latency: computed edit ${computed.toFixed(0)} ms; cached undo/redo worst ${worst.toFixed(0)} ms (${[...undo, ...redo].map((x) => x.toFixed(0)).join(', ')})`);
  expect(computed).toBeLessThan(1000);
  expect(worst).toBeLessThan(150);
});

test('latency: a 50-cell dependency chain computes locally and undoes from cache', { tag: '@timing' }, async ({ page }) => {
  const lines = ['diagcalc-workspace/1', 'context klr type=A2 nu=2i+j ring=QQ', 'def a0 := x_1 e(iji)'];
  for (let i = 1; i < 49; i++) lines.push(`def a${i} := a${i - 1}`);
  lines.push('watch a48');
  await page.goto('/');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Import .dcw or .dch…' }).click();
  await (await chooser).setFiles({ name: 'chain.dcw', mimeType: 'text/plain', buffer: Buffer.from(lines.join('\n') + '\n') });
  await expect(page.locator('.cell[data-cell]')).toHaveCount(50);
  await expect(cell(page, 49).getByTestId('summary')).toHaveText('1 term, degree 2');
  await cell(page, 0).locator('.head').click();
  const editor = cell(page, 0).locator('.cm-content');
  const computed = await timeUntil(page, 49, '1 term, degree 4', 'input', () => editor.fill('def a0 := x_1^2 e(iji)'));
  await editor.press('Enter');
  await expect(page.getByTestId('draft-preview')).toHaveCount(0);
  const undo = await timeUntil(page, 49, '1 term, degree 2', 'click', () => page.getByRole('button', { name: 'Undo', exact: true }).click());
  console.log(`50-cell real WASM/browser chain: edit ${computed.toFixed(1)} ms; cached undo ${undo.toFixed(1)} ms`);
  expect(Number.isFinite(computed) && Number.isFinite(undo)).toBe(true);
});

test('cells and results are typeset; clicking a cell edits its text, clicking a result shows its text', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1^2 psi_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  // after committing, the cell shows its text typeset, not in an editor
  await expect(cell(page, 0).locator('.cm-content')).toHaveCount(0);
  const shown = cell(page, 0).getByTestId('cell-display');
  await expect(shown.locator('.katex')).toHaveCount(1);
  await expect(shown.locator('.katex .msupsub').first()).toBeVisible();
  // expanded, the text is shown once (not again in the head)
  await expect(cell(page, 0).locator('.head .text .katex')).toHaveCount(0);
  // clicking it edits the plain text
  await shown.click();
  await expect(cell(page, 0).locator('.cm-content')).toBeFocused();
  await expect(cell(page, 0).locator('.cm-content')).toHaveText('def a := x_1^2 psi_1 e(iij)');
  await page.keyboard.press('End');
  await page.keyboard.type(' + e(iij) + e(iij)');
  await page.keyboard.press('Enter');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('2 terms');
  await expect(cell(page, 0).getByTestId('cell-display')).toHaveAttribute('data-text', 'def a := x_1^2 psi_1 e(iij) + e(iij) + e(iij)');
  // a result is typeset; a click shows its text in the fixed-width font, selected
  const form = cell(page, 0).getByTestId('textform');
  await expect(form.locator('.katex')).toHaveCount(1);
  await form.click();
  await expect(form).toHaveClass(/raw/);
  await expect(form).toHaveText(/^a := /);
  expect(await page.evaluate(() => window.getSelection()?.toString())).toMatch(/^a := /);
});

test('an expanded cell shows its text once; the normal form only when it differs', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await expect(cell(page, 0).getByTestId('cell-display')).toBeVisible();
  await expect(cell(page, 0).getByTestId('textform')).toHaveCount(0);
  await cell(page, 0).getByTestId('cell-display').click();
  await page.keyboard.press('End');
  await page.keyboard.type(' + psi_1 e(iij)');
  await page.keyboard.press('Enter');
  await expect(cell(page, 0).getByTestId('textform')).toHaveAttribute('data-text', 'a := 2 psi_1 e(iij)');
});

test('clicking a summand of a watch shows which terms of the definitions produced it', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij) + psi_1 e(iij)');
  await addCell(page, 'watch a a');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText(/terms/);
  const summand = cell(page, 1).locator('.summand[data-word="psi_1 x_2 e(iij)"]');
  await summand.click();
  await expect(summand).toHaveAttribute('aria-pressed', 'true');
  const trace = cell(page, 1).getByTestId('trace');
  // x_1 psi_1 = psi_1 x_2 + 1 on e(ii): the product of a's two terms
  await expect(trace.getByTestId('trace-source')).toHaveCount(1);
  await expect(trace.getByTestId('trace-source')).toContainText('a 1');
  await expect(trace.getByTestId('trace-source')).toContainText('a 2');
  // a square uses one term twice
  await cell(page, 1).locator('.summand[data-word="x_1^2 e(iij)"]').click();
  await expect(trace.getByTestId('trace-source')).toHaveCount(1);
  await expect(trace.locator('sup')).toHaveText('2');
  await trace.getByTestId('trace-close').click();
  await expect(cell(page, 1).getByTestId('trace')).toHaveCount(0);
});

test("a watch's value can be read with its commuting dots gathered into monomials", async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'watch psi_1 psi_2 x_1^2 x_3 e(iij)');
  const text = cell(page, 0).getByTestId('textform');
  await expect(text).toHaveAttribute('data-text', 'psi_1 x_1 psi_2 x_1 x_3 e(iij)');
  await cell(page, 0).getByTestId('monomials').click();
  await expect(text).toHaveAttribute('data-text', 'psi_1 psi_2 x_1^2 x_3 e(iij)');
  // one choice for every watch, kept across reloads
  await page.reload();
  await cell(page, 0).locator('.head').click();
  await expect(cell(page, 0).getByTestId('textform')).toHaveAttribute('data-text', 'psi_1 psi_2 x_1^2 x_3 e(iij)');
  await cell(page, 0).getByTestId('monomials').click();
  await expect(cell(page, 0).getByTestId('textform')).toHaveAttribute('data-text', 'psi_1 x_1 psi_2 x_1 x_3 e(iij)');
});
