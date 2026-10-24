import { expect, test, type Page } from '@playwright/test';
import { addCell, cell, newDoc, setContext, typeCell } from './helpers';
import type { FromWorker, ToWorker } from '../src/engine/worker';

async function example(page: Page) {
  await newDoc(page);
  await setContext(page, 'nilhecke nu=2 ring=QQ');
  await typeCell(page, 'def e := x_1 psi_1');
  await addCell(page, 'def alias := e');
  await addCell(page, 'watch alias alias - alias');
  await addCell(page, 'check idempotent(alias)');
  await expect(cell(page, 2).getByTestId('summary')).toHaveText('0');
  await expect(cell(page, 3).getByTestId('verdict')).toHaveText('holds');
}

async function editDefinition(page: Page, text: string, commit: boolean) {
  await cell(page, 0).locator('.head').click();
  await cell(page, 0).locator('.cm-content').fill(text);
  if (commit) await cell(page, 0).locator('.cm-content').press('Enter');
}

test('committed edits update transitive watches and checks, undo and redo automatically', async ({ page }) => {
  await example(page);
  await editDefinition(page, 'def e := x_1', true);
  await expect(cell(page, 3).getByTestId('verdict')).toHaveText('fails');
  await expect(cell(page, 2).getByTestId('summary')).toHaveText('2 terms, degrees 2 to 4');
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(cell(page, 3).getByTestId('verdict')).toHaveText('holds');
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  await expect(cell(page, 3).getByTestId('verdict')).toHaveText('fails');
});

test('editing a definition previews downstream results without Enter or Recompute', async ({ page }) => {
  await example(page);
  await editDefinition(page, 'def e := x_1', false);
  await expect(cell(page, 3).getByTestId('verdict')).toHaveText('fails');
  await expect(cell(page, 2).getByTestId('summary')).toHaveText('2 terms, degrees 2 to 4');
  await expect(page.getByRole('button', { name: 'Recompute', exact: true })).toHaveCount(0);
  // A broken draft cannot leave a successful verdict visible either.
  await cell(page, 0).locator('.cm-content').fill('def e := x_1 +');
  await expect(cell(page, 3).getByTestId('summary')).not.toHaveText(/holds|fails/);
  await cell(page, 0).locator('.cm-content').press('Escape');
  await expect(cell(page, 3).getByTestId('verdict')).toHaveText('holds');
});

test('removing an idempotent crossing on the canvas updates its dependents automatically', async ({ page }) => {
  await example(page);
  await cell(page, 0).locator('.head').click();
  await cell(page, 0).getByTestId('open-canvas').click();
  const canvas = page.locator('.canvas');
  await canvas.locator('.hit-piece[data-draw="crossing"]').click();
  await page.keyboard.press('Delete');
  await expect(canvas.locator('.hit-piece[data-draw="crossing"]')).toHaveCount(0);
  await canvas.getByTestId('canvas-back').click();
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'e := x_1');
  await expect(cell(page, 3).getByTestId('outcome')).toHaveText('fails');
  await expect(cell(page, 2).getByTestId('summary')).toHaveText('2 terms, degrees 2 to 4');
});

test('drafts update solves and direct check edits, and committing persists the result', async ({ page }) => {
  await example(page);
  await addCell(page, 'solve c alias alias - alias = 0 for c');
  await expect(cell(page, 4).getByTestId('summary')).toHaveText('unique solution');
  await editDefinition(page, 'def e := x_1', false);
  await expect(cell(page, 4).getByTestId('summary')).toHaveText('no solution');
  await expect(cell(page, 1)).toHaveAttribute('data-status', 'fresh');
  await expect(page.getByTestId('draft-preview')).toBeVisible();
  await cell(page, 0).locator('.cm-content').press('Enter');
  await expect(page.getByTestId('draft-preview')).toHaveCount(0);
  await page.reload();
  await expect(cell(page, 3).getByTestId('outcome')).toHaveText('fails');
  await expect(cell(page, 4).getByTestId('summary')).toHaveText('no solution');
  await cell(page, 3).locator('.head').click();
  await cell(page, 3).locator('.cm-content').fill('check idempotent(1)');
  await expect(cell(page, 3).getByTestId('verdict')).toHaveText('holds');
  await cell(page, 3).locator('.cm-content').press('Escape');
  await expect(cell(page, 3).getByTestId('verdict')).toHaveText('fails');
});

for (const cancel of [true, false]) {
  test(`pending commit cannot override ${cancel ? 'Escape' : 'a newer draft'}`, async ({ page }) => {
    await newDoc(page);
    await typeCell(page, 'def a := x_1');
    const row = cell(page, 0);
    await expect(row).toHaveAttribute('data-status', 'fresh');
    // Delay real parsing, not its result, so Enter remains pending when the next edit arrives.
    await page.evaluate(() => {
      const w = window as unknown as { parsed: number };
      w.parsed = 0;
      const post = Worker.prototype.postMessage;
      Worker.prototype.postMessage = function (m: ToWorker) {
        if (m.type !== 'call' || m.op !== 'workspace.parse') return post.call(this, m);
        const receive = (event: MessageEvent<FromWorker>) => {
          if ((event.data.type === 'result' || event.data.type === 'failed') && event.data.id === m.id) {
            if (m.request.includes('x_1^9')) w.parsed++;
            this.removeEventListener('message', receive);
          }
        };
        this.addEventListener('message', receive);
        setTimeout(() => post.call(this, m), 500);
      };
    });
    await row.getByTestId('cell-display').click();
    const editor = row.locator('.cm-content');
    await editor.fill('def a := x_1^9');
    await editor.press('Enter');
    if (cancel) await editor.press('Escape');
    else await editor.fill('def a := x_1^7');
    // Both the preview parse and commit validation must have returned before asserting.
    await expect.poll(() => page.evaluate(() => (window as unknown as { parsed: number }).parsed)).toBeGreaterThanOrEqual(2);
    await expect(row.locator('.head .text')).toHaveAttribute('data-text', 'a := x_1');
    if (!cancel) {
      await expect(editor).toHaveText('def a := x_1^7');
      await editor.press('Escape');
    }
    await expect(page.getByTestId('draft-preview')).toHaveCount(0);
    await page.reload();
    await expect(row.locator('.head .text')).toHaveAttribute('data-text', 'a := x_1');
  });
}

/** Holds every `workspace.run` call until `release`, so a recomputation can be seen while it runs. */
async function holdRuns(page: Page) {
  await page.evaluate(() => {
    const w = window as unknown as { held: (() => void)[]; hold: boolean };
    w.held = [];
    w.hold = true;
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (m: ToWorker) {
      if (!w.hold || m.type !== 'call' || m.op !== 'workspace.run') return post.call(this, m);
      w.held.push(() => post.call(this, m));
    };
  });
  return () => page.evaluate(() => {
    const w = window as unknown as { held: (() => void)[]; hold: boolean };
    w.hold = false;
    for (const f of w.held.splice(0)) f();
  });
}

async function collapse(page: Page, index: number) {
  const head = cell(page, index).locator('.head');
  if (await head.getAttribute('aria-expanded') === 'true') await head.click();
  await expect(head).toHaveAttribute('aria-expanded', 'false');
}

test('a collapsed check shows its verdict beside its kind, and its dot only says whether the computation is current', async ({ page }) => {
  await example(page);
  await collapse(page, 3);
  const check = cell(page, 3);
  const pill = check.locator('.head > .kind + .outcome');
  const dot = check.getByTestId('cell-dot');
  await expect(pill).toHaveText('holds');
  await expect(dot).toHaveAttribute('title', 'Computed');
  await expect(check.locator('.head > .kind')).toHaveText('check');

  // an edit to a definition it uses: while it recomputes the dot says so and no verdict is shown
  const release = await holdRuns(page);
  await editDefinition(page, 'def e := x_1', true);
  await expect(dot).toHaveAttribute('title', 'Recomputing');
  await expect(dot).toHaveClass(/stale|running/);
  await expect(check.getByTestId('outcome')).toHaveCount(0);
  await release();
  // computed fine, and the check fails: the dot is green, the pill says fails
  await expect(pill).toHaveText('fails');
  await expect(pill).toHaveClass(/fails/);
  await expect(dot).toHaveAttribute('title', 'Computed');
  await expect(dot).toHaveClass(/fresh/);

  // undo restores the old verdict from the cache
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(pill).toHaveText('holds');
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  await expect(pill).toHaveText('fails');

  // an edit to the check itself
  await check.locator('.head').click();
  await expect(check.getByTestId('outcome')).toHaveCount(0);
  await check.locator('.cm-content').fill('check idempotent(1)');
  await check.locator('.cm-content').press('Enter');
  await expect(check.getByTestId('verdict')).toHaveText('holds');
  await collapse(page, 3);
  await expect(pill).toHaveText('holds');

  // view mode shows the same pill
  await page.getByTestId('view-mode').click();
  await expect(page.getByTestId('view-mode')).toHaveAttribute('aria-pressed', 'true');
  await collapse(page, 3);
  await expect(pill).toHaveText('holds');
  await expect(dot).toHaveAttribute('title', 'Computed');
  // the other cells have no yes/no outcome, so no pill
  await expect(cell(page, 0).getByTestId('outcome')).toHaveCount(0);
  await expect(cell(page, 2).getByTestId('outcome')).toHaveCount(0);
});
