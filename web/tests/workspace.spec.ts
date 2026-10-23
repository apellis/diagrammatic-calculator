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
  await expect(cell(page, 2).getByTestId('summary')).toHaveText('holds');
  await expect(cell(page, 3).getByTestId('summary')).toHaveText('fails');
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
  await page.getByRole('button', { name: 'watch' }).click();
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

test('history: versions, read-only view and restore', async ({ page }) => {
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

test('documents persist across reloads, and the list can rename, duplicate and delete', async ({ page }) => {
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
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('holds');
  await page.keyboard.press('Escape');
  const download = page.waitForEvent('download');
  await page.keyboard.press('ControlOrMeta+Shift+S');
  const file = await download;
  const text = await (await file.createReadStream()).toArray().then((b) => Buffer.concat(b).toString());
  expect(text).toBe('diagcalc-workspace/1\ncontext klr type=A2 nu=2i+j ring=QQ\ndef a := psi_1 e(iji)  # id=d1\ncheck idempotent(a a)  # id=k2\n');
  await page.goto('/');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Import .dcw…' }).click();
  await (await chooser).setFiles({ name: 'mine.dcw', mimeType: 'text/plain', buffer: Buffer.from(text) });
  await expect(page.locator('.topbar .name')).toHaveText('mine');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('holds');
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
});

test('the gate example: idempotents of a cyclotomic quotient', async ({ page }) => {
  await page.goto('/');
  await page.locator('[data-example="klr-a2-idempotents"]').click();
  const summaries = page.locator('.cell[data-cell] [data-testid=summary]');
  await expect(summaries).toHaveText(['', '2 terms', '2 terms, degree 0', '4 solutions', '1 term', '2 terms', 'holds', 'holds', 'holds', 'holds', 'holds', 'holds', '1 term, degree 0'], { timeout: 30_000 });
  // the quotient's notes under the badge are typeset
  const watch = page.locator('.cell[data-cell]').nth(2);
  await watch.locator('.head').click();
  await expect(watch.getByTestId('prov-note').first().locator('.katex').first()).toBeVisible();
  await expect(watch.getByTestId('prov-note').first()).not.toContainText('<h_(i_1)');
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
  const edit = async (text: string) => {
    await page.locator(`#cell-${id} .head`).click();
    await page.locator(`#cell-${id} .cm-content`).click();
    await page.keyboard.press('ControlOrMeta+a');
    await page.keyboard.type(text);
  };
  // an edit that needs the engine
  const second = '3 terms, degrees 4 to 6';
  await edit('def a := (x_1 + x_2) psi_1 e(iij) + x_3 e(iij)');
  const computed = await timeUntil(page, 1, second, 'keydown', () => page.keyboard.press('Enter'));
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
