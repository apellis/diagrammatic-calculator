import { expect, test } from '@playwright/test';
import { addCell, cell, newDoc, typeCell } from './helpers';

async function existing(page: import('@playwright/test').Page) {
  await newDoc(page);
  await typeCell(page, 'def a := e(iij)');
  await expect(cell(page, 0).getByTestId('cell-display')).toBeVisible();
}

test('existing text supports Space, accessible errors, cancellation and disclosure', async ({ page }) => {
  await existing(page);
  const c = cell(page, 0);
  const display = c.getByTestId('cell-display');
  await display.focus();
  await page.keyboard.press('Space');
  const editor = c.getByRole('textbox', { name: 'Cell text' });
  await expect(editor).toBeFocused();
  await editor.fill('watch ???');
  const problem = c.getByTestId('cell-problem');
  await expect(problem).toHaveAttribute('role', 'status');
  await expect(editor).toHaveAttribute('aria-invalid', 'true');
  await expect(editor).toHaveAttribute('aria-describedby', await problem.getAttribute('id') as string);
  await page.keyboard.press('Escape');
  await expect(display).toBeFocused();
  await expect(display).toHaveAttribute('data-text', 'def a := e(iij)');
  await expect(c.getByTestId('cell-disclosure')).toHaveText('▾');
  // After cancellation the first activation selects the inactive, open cell.
  await c.locator('.head').press('Space');
  await c.locator('.head').press('Space');
  await expect(c.getByTestId('cell-disclosure')).toHaveText('▸');
});

test('existing explicit edits return focus and blur saves without stealing external focus', async ({ page }) => {
  await existing(page);
  const c = cell(page, 0);
  await c.getByTestId('cell-display').click();
  await c.getByRole('textbox', { name: 'Cell text' }).fill('def a := x_1 e(iij)');
  await page.keyboard.press('Enter');
  await expect(c.getByTestId('cell-display')).toBeFocused();
  await c.getByTestId('cell-display').click();
  await c.getByRole('textbox', { name: 'Cell text' }).fill('def a := x_2 e(iij)');
  const search = page.getByRole('textbox', { name: 'Search cells' });
  await search.click();
  await expect(c.getByTestId('cell-display')).toHaveAttribute('data-text', 'def a := x_2 e(iij)');
  await expect(search).toBeFocused();
});

async function delayParsing(page: import('@playwright/test').Page) {
  await page.evaluate(() => {
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (m) {
      if (m.type === 'call' && m.op === 'workspace.parse') setTimeout(() => post.call(this, m), 600);
      else post.call(this, m);
    };
  });
}

test('pending new-cell submission does not take focus back from search', async ({ page }) => {
  await newDoc(page);
  await delayParsing(page);
  await page.keyboard.type('x_1 e(iij)');
  await page.keyboard.press('Enter');
  const search = page.getByRole('textbox', { name: 'Search cells' });
  await search.click();
  await expect(cell(page, 0).getByTestId('cell-display')).toBeVisible();
  await expect(search).toBeFocused();
});

test('pending invalid existing edits announce errors without taking external focus', async ({ page }) => {
  await existing(page);
  await delayParsing(page);
  await cell(page, 0).getByTestId('cell-display').click();
  await cell(page, 0).getByRole('textbox', { name: 'Cell text' }).fill('watch ???');
  await page.keyboard.press('Enter');
  const search = page.getByRole('textbox', { name: 'Search cells' });
  await search.click();
  await expect(cell(page, 0).getByTestId('cell-problem')).toBeVisible();
  await expect(search).toBeFocused();
});

test('intentional check evaluation errors are not globally forbidden', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'check idempotent()');
  await expect(cell(page, 0)).toHaveAttribute('data-kind', 'check');
  await expect(composer(page)).toHaveCount(0);
});

const composer = (page: import('@playwright/test').Page) => page.locator('[data-cell="new"]');

test('moving the insertion position preserves the new text, name and per-kind drafts', async ({ page }) => {
  await existing(page);
  await addCell(page, 'def b := x_1 e(iij)');
  await page.getByTestId('add-cell').click();
  await composer(page).getByLabel('Element name').fill('c');
  await composer(page).locator('.cm-content').fill('x_2 e(iij)');
  await composer(page).getByRole('button', { name: 'Note', exact: true }).click();
  await composer(page).locator('.cm-content').fill('note """Keep this note draft"""');
  await composer(page).getByRole('button', { name: 'Element', exact: true }).click();
  await cell(page, 0).locator('.head').press('Alt+Enter');
  await expect(composer(page).locator('.cm-content')).toHaveText('x_2 e(iij)');
  await expect(composer(page).getByLabel('Element name')).toHaveValue('c');
  await cell(page, 1).getByRole('button', { name: 'Move the cell up' }).click();
  await expect(composer(page).locator('.cm-content')).toHaveText('x_2 e(iij)');
  await cell(page, 0).getByRole('button', { name: 'Move the cell down' }).click();
  await composer(page).getByRole('button', { name: 'Note', exact: true }).click();
  await expect(composer(page).locator('.cm-content')).toHaveText('note """Keep this note draft"""');
  await composer(page).getByRole('button', { name: 'Element', exact: true }).click();
  await composer(page).getByRole('button', { name: 'Add cell', exact: true }).click();
  await expect(cell(page, 1).locator('.head .text')).toHaveAttribute('data-text', 'c := x_2 e(iij)');
  await expect(cell(page, 2).locator('.head .text')).toHaveAttribute('data-text', 'b := x_1 e(iij)');
});

test('moving a draft cancels validation started at its previous position', async ({ page }) => {
  await existing(page);
  await addCell(page, 'def b := x_1 e(iij)');
  await page.getByTestId('add-cell').click();
  await composer(page).locator('.cm-content').fill('def c := a');
  await page.evaluate(() => {
    const w = window as unknown as { movedValidationFinished: boolean };
    w.movedValidationFinished = false;
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (m) {
      if (m.type !== 'call' || m.op !== 'workspace.parse') return post.call(this, m);
      const received = (e: MessageEvent) => {
        if ((e.data.type === 'result' || e.data.type === 'failed') && e.data.id === m.id) {
          w.movedValidationFinished = true;
          this.removeEventListener('message', received);
        }
      };
      this.addEventListener('message', received);
      setTimeout(() => post.call(this, m), 700);
    };
  });
  await composer(page).getByRole('button', { name: 'Add cell', exact: true }).click();
  await cell(page, 0).locator('.head').press('Alt+Enter');
  await expect.poll(() => page.evaluate(() => (window as unknown as { movedValidationFinished: boolean }).movedValidationFinished)).toBe(true);
  await expect(page.locator('.cell[data-cell]:not([data-cell="new"])')).toHaveCount(2);
  await expect(composer(page).locator('.cm-content')).toHaveText('def c := a');
  await composer(page).getByRole('button', { name: 'Add cell', exact: true }).click();
  await expect(cell(page, 1).locator('.head .text')).toHaveAttribute('data-text', 'c := a');
});

test('moving a drawing draft preserves its selected boundary', async ({ page }) => {
  await existing(page);
  await addCell(page, 'def b := x_1 e(iij)');
  await page.getByTestId('add-cell').click();
  await composer(page).getByLabel('Element name').fill('c');
  await composer(page).getByRole('button', { name: 'Draw', exact: true }).click();
  await composer(page).getByRole('button', { name: 'jii', exact: true }).click();
  await cell(page, 0).locator('.head').press('Alt+Enter');
  await expect(composer(page).getByRole('button', { name: 'Draw', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(composer(page).getByRole('button', { name: 'jii', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await composer(page).getByRole('button', { name: 'Start drawing', exact: true }).click();
  await expect(cell(page, 1).locator('.head .text')).toHaveAttribute('data-text', 'c := e(jii)');
});

test('new drafts survive external navigation without committing, then explicit add returns focus', async ({ page }) => {
  await newDoc(page);
  await page.keyboard.type('x_1 e(iij)');
  const search = page.getByRole('textbox', { name: 'Search cells' });
  await search.click();
  await expect(search).toBeFocused();
  await expect(composer(page).locator('.cm-content')).toHaveText('x_1 e(iij)');
  await expect(cell(page, 0)).toHaveCount(0);
  await composer(page).getByRole('button', { name: 'Add cell', exact: true }).click();
  await expect(cell(page, 0).locator('.head')).toBeFocused();
  await page.getByTestId('add-cell').click();
  await page.keyboard.type('unfinished');
  await page.keyboard.press('Escape');
  await expect(composer(page)).toHaveCount(0);
  await expect(page.getByTestId('add-cell')).toBeFocused();
});

test('the unfinished Check template explains its missing argument instead of adding an error cell', async ({ page }) => {
  await newDoc(page);
  await composer(page).getByRole('button', { name: 'Check', exact: true }).click();
  await page.keyboard.press('Enter');
  await expect(composer(page).getByRole('status')).toContainText('inside idempotent');
  await composer(page).getByRole('button', { name: 'Note', exact: true }).click();
  await composer(page).getByRole('button', { name: 'Check', exact: true }).click();
  await page.keyboard.press('Enter');
  await expect(composer(page).getByRole('status')).toContainText('inside idempotent');
  await expect(cell(page, 0)).toHaveCount(0);
  await page.keyboard.press('End');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.type('e(iij)');
  await page.keyboard.press('Enter');
  await expect(cell(page, 0)).toHaveAttribute('data-kind', 'check');
});
