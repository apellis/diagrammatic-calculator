import { expect, test } from '@playwright/test';
import { cell, newDoc, typeCell } from './helpers';
import type { FromWorker, ToWorker } from '../src/engine/worker';

const composer = (page: import('@playwright/test').Page) => page.locator('[data-cell="new"]');

async function delayOperation(page: import('@playwright/test').Page, op: string) {
  await page.evaluate((op) => {
    const w = window as unknown as { finishedCreationCalls: number };
    w.finishedCreationCalls = 0;
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (m: ToWorker) {
      if (m.type !== 'call' || m.op !== op) return post.call(this, m);
      const receive = (e: MessageEvent<FromWorker>) => {
        if ((e.data.type === 'result' || e.data.type === 'failed') && e.data.id === m.id) {
          w.finishedCreationCalls++;
          this.removeEventListener('message', receive);
        }
      };
      this.addEventListener('message', receive);
      setTimeout(() => post.call(this, m), 500);
    };
  }, op);
}

test('cancelling document creation invalidates pending validation', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await delayOperation(page, 'context.describe');
  const dialog = page.getByRole('dialog', { name: 'New document' });
  await dialog.getByRole('button', { name: 'Create', exact: true }).click();
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { finishedCreationCalls: number }).finishedCreationCalls)).toBeGreaterThan(0);
  await expect(page.locator('tr[data-doc]')).toHaveCount(0);
  await expect(composer(page)).toHaveCount(0);
});

test('cancelling a new cell invalidates pending validation', async ({ page }) => {
  await newDoc(page);
  await delayOperation(page, 'workspace.parse');
  await page.keyboard.type('x_1 e(iij)');
  await page.keyboard.press('Enter');
  await page.keyboard.press('Escape');
  await expect.poll(() => page.evaluate(() => (window as unknown as { finishedCreationCalls: number }).finishedCreationCalls)).toBeGreaterThan(0);
  await expect(composer(page)).toHaveCount(0);
  await expect(cell(page, 0)).toHaveCount(0);
});

test('changing the context replaces the starting strands before creation', async ({ page }) => {
  await newDoc(page);
  const c = composer(page);
  await c.getByRole('button', { name: 'Draw', exact: true }).click();
  await c.getByRole('button', { name: 'jii', exact: true }).click();
  await expect(c.getByRole('img', { name: 'Identity diagram: jii', exact: true })).toBeVisible();
  await delayOperation(page, 'term.layout');
  await page.getByTestId('context').click();
  await page.getByLabel('Context', { exact: true }).fill('nilhecke nu=2 ring=QQ');
  await page.getByLabel('Context', { exact: true }).press('Enter');
  await expect(c.getByRole('img', { name: 'Identity diagram: ii', exact: true })).toBeVisible();
  await c.getByRole('button', { name: 'Start drawing', exact: true }).click();
  await expect(page.locator('.canvas .frame')).toHaveAttribute('data-word', /^(1|e\(ii\))$/);
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 't1 := e(ii)');
});

test('changing the boundary cancels a pending Start drawing submission', async ({ page }) => {
  await newDoc(page);
  const c = composer(page);
  await c.getByRole('button', { name: 'Draw', exact: true }).click();
  await expect(c.getByRole('button', { name: 'Start drawing', exact: true })).toBeEnabled();
  await delayOperation(page, 'workspace.parse');
  await c.getByRole('button', { name: 'Start drawing', exact: true }).click();
  await c.getByRole('button', { name: 'jii', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { finishedCreationCalls: number }).finishedCreationCalls)).toBeGreaterThan(0);
  await expect(cell(page, 0)).toHaveCount(0);
  await expect(c.getByRole('img', { name: 'Identity diagram: jii', exact: true })).toBeVisible();
  await c.getByRole('button', { name: 'Start drawing', exact: true }).click();
  await expect(page.locator('.canvas .frame')).toHaveAttribute('data-word', 'e(jii)');
});

test('new-document defaults and its Create action fit on a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'New document' });
  await expect(dialog.getByRole('button', { name: 'Create', exact: true })).toBeInViewport();
  await dialog.getByLabel('Presentation', { exact: true }).selectOption('nilhecke');
  await expect(dialog.getByLabel(/Weight/)).toHaveValue('3');
  await dialog.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page.getByTestId('context')).toContainText('nilhecke');
  await expect(composer(page).getByRole('button', { name: 'Draw', exact: true })).toBeVisible();
});

test('a new document shows every cell type without a slash command', async ({ page }) => {
  await newDoc(page);
  const c = composer(page);
  for (const name of ['Element', 'Watch', 'Check', 'Solve', 'Note']) {
    await expect(c.getByRole('button', { name, exact: true })).toBeVisible();
  }
  await c.getByRole('button', { name: 'Watch', exact: true }).click();
  await expect(c.locator('.cm-content')).toHaveText('watch ');
  await page.keyboard.type('x_1 e(iij)');
  await c.getByRole('button', { name: 'Add cell', exact: true }).click();
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term, degree 2');
  await page.getByTestId('add-cell').click();
  await expect(composer(page).getByRole('button', { name: 'Note', exact: true })).toBeVisible();
});

test('switching new-cell types keeps unfinished text, and Escape discards it', async ({ page }) => {
  await newDoc(page);
  const c = composer(page);
  await page.keyboard.type('x_1 e(iij)');
  await c.getByRole('button', { name: 'Note', exact: true }).click();
  await page.keyboard.type('An unfinished note');
  await c.getByRole('button', { name: 'Element', exact: true }).click();
  await expect(c.locator('.cm-content')).toHaveText('x_1 e(iij)');
  await c.getByRole('button', { name: 'Note', exact: true }).click();
  await expect(c.locator('.cm-content')).toContainText('An unfinished note');
  await page.keyboard.press('Escape');
  await expect(composer(page)).toHaveCount(0);
  await expect(cell(page, 0)).toHaveCount(0);
});

test('draw starts from a valid identity without typing and edits on the canvas', async ({ page }) => {
  await newDoc(page);
  await composer(page).getByRole('button', { name: 'Draw', exact: true }).click();
  await composer(page).getByRole('button', { name: 'Start drawing', exact: true }).click();
  const canvas = page.locator('.canvas');
  await expect(canvas.locator('.frame[data-word="e(iij)"]')).toBeVisible();
  await expect(canvas.getByText('Read only', { exact: true })).toHaveCount(0);
  // Straight SVG paths have zero geometric width; click the real stroked hit area.
  const strand = (await canvas.locator('.hit-strand').first().boundingBox())!;
  await page.mouse.click(strand.x + strand.width / 2, strand.y + strand.height / 2);
  await canvas.locator('[data-generator="x"]').click();
  await expect(canvas.locator('[data-draw="dot"]')).toBeVisible();
  await canvas.getByTestId('canvas-back').click();
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', /:= x_1 e\(iij\)/);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', /:= e\(iij\)/);
});

test('an expression can continue directly on the drawing canvas', async ({ page }) => {
  await newDoc(page);
  await page.keyboard.type('psi_1 e(iij)');
  await composer(page).getByRole('button', { name: 'Draw', exact: true }).click();
  await composer(page).getByRole('button', { name: 'Open on canvas', exact: true }).click();
  await expect(page.locator('.canvas [data-draw="crossing"]').first()).toBeVisible();
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', /:= psi_1 e\(iij\)/);
});

test('starting sequences can be chosen and rearranged without changing their weight', async ({ page }) => {
  await newDoc(page);
  const c = composer(page);
  await c.getByLabel('Element name', { exact: true }).fill('a');
  await c.getByRole('button', { name: 'Draw', exact: true }).click();
  await c.getByRole('button', { name: 'jii', exact: true }).click();
  await expect(c.getByRole('img', { name: 'Identity diagram: jii', exact: true })).toBeVisible();
  await c.getByRole('button', { name: 'Swap strands 1 and 2', exact: true }).click();
  await expect(c.getByRole('img', { name: 'Identity diagram: iji', exact: true })).toBeVisible();
  await c.getByRole('button', { name: 'Type', exact: true }).click();
  await c.getByRole('button', { name: 'Draw', exact: true }).click();
  await expect(c.getByRole('img', { name: 'Identity diagram: iji', exact: true })).toBeVisible();
  await c.getByRole('button', { name: 'Start drawing', exact: true }).click();
  await expect(page.locator('.canvas .frame[data-word="e(iji)"]')).toBeVisible();
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := e(iji)');
});

test('oriented diagrams can start with upward and downward strands', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'New document' });
  await dialog.getByLabel('Presentation', { exact: true }).selectOption('kl');
  await dialog.getByRole('button', { name: 'Create', exact: true }).click();
  const c = composer(page);
  await c.getByRole('button', { name: 'Draw', exact: true }).click();
  await c.getByRole('button', { name: 'Add upward i strand', exact: true }).click();
  await c.getByRole('button', { name: 'Add downward i strand', exact: true }).click();
  await expect(c.getByRole('img', { name: 'Identity diagram: i i*', exact: true })).toBeVisible();
  await c.getByRole('button', { name: 'Start drawing', exact: true }).click();
  await expect(page.locator('.canvas .frame[data-word="e(i i*)"]')).toBeVisible();
});

test('new-cell choices fit on a phone and are operable by keyboard', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await newDoc(page);
  await composer(page).getByRole('button', { name: 'Note', exact: true }).focus();
  await page.keyboard.press('Enter');
  await page.keyboard.type('A note');
  await page.keyboard.press('Control+Enter');
  await expect(cell(page, 0)).toHaveAttribute('data-kind', 'note');
  await page.getByTestId('add-cell').click();
  await composer(page).getByRole('button', { name: 'Draw', exact: true }).click();
  await expect(composer(page).getByRole('button', { name: 'Start drawing', exact: true })).toBeVisible();
  await composer(page).getByRole('button', { name: 'Start drawing', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Gestures and keys', exact: true })).toBeInViewport({ ratio: 1 });
  await expect(page.getByTestId('labels-toggle')).toBeInViewport({ ratio: 1 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});

test('the direct text and slash shortcuts still work', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := e(iij)');
  await page.getByTestId('add-cell').click();
  await page.keyboard.type('/watch');
  await composer(page).locator('.kindmenu').getByRole('button', { name: 'watch', exact: true }).click();
  await page.keyboard.type('a');
  await page.keyboard.press('Enter');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term, degree 0');
});
