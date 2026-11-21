import { expect, test } from '@playwright/test';
import { cell, newDoc, setContext } from './helpers';

// A complex whose second differential is zero; writing a nonzero entry there breaks d² = 0.
const COMPLEX =
  'complex C := {"terms":[{"at":-2,"summands":[{"object":"i@1","shift":2}]},{"at":-1,"summands":[{"object":"i@1"}]},{"at":0,"summands":[{"object":"i@1"}]}],"differentials":[{"from":-2,"matrix":[["x_1 e(i@1)"]]}]}';

async function setup(page: import('@playwright/test').Page) {
  await newDoc(page);
  await setContext(page, 'klrw type=A1 ring=QQ red=2i@0 quotient=cyclotomic');
  await page.keyboard.insertText(COMPLEX);
  await page.keyboard.press('Enter');
  const c = cell(page, 0);
  await expect(c.getByTestId('complex-status')).toBeVisible();
  return c;
}

test('an edit that breaks a check keeps the entry and marks it in red', async ({ page }) => {
  const c = await setup(page);
  await c.getByTestId('cx-entry-edit').nth(1).click();
  await c.getByTestId('cx-entry-input').fill('e(i@1)');
  await page.keyboard.press('Enter');
  // the verdict, not the status line: the status line of a valid complex also mentions d², and it
  // is replaced by the verdict once the check of the edit comes back
  await expect(c.getByTestId('verdict')).toHaveText('not a complex');
  // Both differentials take part in d ∘ d, so both arrows are marked; both entries are kept.
  await expect(c.locator('.cx-arrow[data-bad="true"]')).toHaveCount(2);
  await expect(c.getByTestId('cx-entry-edit')).toHaveCount(2);
  await c.getByTestId('cx-entry-edit').nth(1).click();
  await expect(c.getByTestId('cx-entry-input')).toHaveValue('e(i@1)');
});

test('an unreadable entry is shown with the error and nothing is thrown away', async ({ page }) => {
  const c = await setup(page);
  await c.getByTestId('cx-entry-edit').nth(0).click();
  await c.getByTestId('cx-entry-input').fill('x_1 e(i@1) +');
  await page.keyboard.press('Enter');
  await expect(c.getByTestId('cx-error')).toBeVisible();
  await expect(c.getByTestId('cx-entry-edit')).not.toHaveCount(0);
  // The entry can be repaired in place.
  await c.getByTestId('cx-entry-edit').nth(0).click();
  await c.getByTestId('cx-entry-input').fill('x_1 e(i@1)');
  await page.keyboard.press('Enter');
  await expect(c.getByTestId('cx-error')).toHaveCount(0);
});

test('double-clicking a map opens its pane beside the cell, edits an entry, and closes', async ({ page }) => {
  const c = await setup(page);
  await c.getByTestId('cx-map').first().locator('.cx-shaft').dblclick();
  const pane = page.getByTestId('map-pane');
  await expect(pane).toBeVisible();
  await expect(page.locator('.cell-row.cell-split')).toHaveCount(1);
  await expect(page.locator('.cell-row.cell-split').getByTestId('map-pane')).toBeVisible();
  await expect(pane.getByTestId('map-pane-canvas')).toBeVisible();
  await pane.getByTestId('map-pane-input').fill('2 x_1 e(i@1)');
  await pane.getByTestId('map-pane-write').click();
  await expect(pane.getByTestId('map-pane-write')).toBeDisabled();
  await expect(pane.getByTestId('map-pane-input')).toHaveValue(/^2/);
  await pane.getByTestId('map-pane-close').click();
  await expect(pane).toHaveCount(0);
  await expect(page.locator('.cell-row.cell-split')).toHaveCount(0);
  // a double-click on an entry opens the pane too
  await c.getByTestId('cx-map').first().locator('.cx-on-arrow, .cx-entry-btn, .cx-entry-edit').first().dblclick();
  await expect(page.getByTestId('map-pane')).toBeVisible();
  await page.keyboard.press('Escape');
});
