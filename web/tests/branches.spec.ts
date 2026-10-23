import { expect, test } from '@playwright/test';
import { addCell, cell, newDoc, typeCell } from './helpers';

const text = (page: import('@playwright/test').Page, n: number) => cell(page, n).locator('.head .text');

test('a scratch branch: try a change, leave it, come back, accept it, undo the accept', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iji)');
  await addCell(page, 'watch a a');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term, degree 4');

  await page.locator('.main').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('ControlOrMeta+b');
  // the branch gets a name, suggested and selected: Enter takes it
  await expect(page.getByLabel('Branch name')).toHaveValue('what if 1');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('branch')).toContainText('what if 1');
  await expect(page.locator('.branch-banner')).toBeVisible();
  // change the definition on the branch: the watch follows there
  await cell(page, 0).locator('.head').click();
  await page.locator('.cell.active .cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await typeCell(page, 'def a := x_1 e(iji) + x_3 e(iji)');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('3 terms, degree 4');

  // the document itself is unchanged
  await page.getByRole('button', { name: 'Leave' }).click();
  await expect(page.getByTestId('branch')).toHaveCount(0);
  await expect(text(page, 0)).toHaveAttribute('data-text', 'a := x_1 e(iji)');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term, degree 4');

  // back on the branch, compare and accept
  await page.getByLabel('Branches').selectOption({ label: 'what if 1' });
  await expect(text(page, 0)).toHaveAttribute('data-text', 'a := x_1 e(iji) + x_3 e(iji)');
  await page.getByRole('button', { name: 'Accept…' }).click();
  await expect(page.getByTestId('branch-diff')).toContainText('→ def a := x_1 e(iji) + x_3 e(iji)');
  await page.getByRole('button', { name: 'Accept', exact: true }).click();
  await expect(page.getByTestId('branch')).toHaveCount(0);
  await expect(page.getByLabel('Branches')).toHaveCount(0);
  await expect(text(page, 0)).toHaveAttribute('data-text', 'a := x_1 e(iji) + x_3 e(iji)');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('3 terms, degree 4');

  // accepting is one step
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(text(page, 0)).toHaveAttribute('data-text', 'a := x_1 e(iji)');
});

test('discarding a branch leaves the document as it was', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iji)');
  await page.locator('.main').click({ position: { x: 5, y: 5 } });
  await page.getByTestId('branch-start').click();
  await page.getByLabel('Branch name').fill('three');
  await page.getByRole('button', { name: 'Start' }).click();
  await expect(page.getByTestId('branch')).toContainText('three');
  await addCell(page, 'watch a a a');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term, degree 6');
  await page.getByRole('button', { name: 'Discard' }).click();
  await page.getByRole('dialog', { name: 'Discard the branch' }).getByRole('button', { name: 'Discard', exact: true }).click();
  await expect(page.getByTestId('branch')).toHaveCount(0);
  await expect(page.locator('.cell[data-cell]:not([data-cell="new"])')).toHaveCount(1);
});
