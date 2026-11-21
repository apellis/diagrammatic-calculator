import { expect, test } from '@playwright/test';
import { cell, newDoc, setContext } from './helpers';

// The deformation solver on a complex cell: pick entries, solve d² = 0 order by order in h, read
// the corrections or the obstruction, and add the corrected complex as a new cell.
const complexText = 'complex C := {"terms":[{"at":-2,"summands":[{"object":"i@1","shift":4}]},{"at":-1,"summands":[{"object":"i@1","shift":2}]},{"at":0,"summands":[{"object":"i@1","shift":0}]}],"differentials":[{"from":-2,"matrix":[["h e(i@1)"]]},{"from":-1,"matrix":[["x_1 e(i@1)"]]}]}';

test('without truncate the solver says how to turn it on', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'klrw type=A1 ring=QQ red=2i@0');
  // h is not a scalar of this context, so the map from −2 is written as zero
  await page.keyboard.insertText(complexText.replace('"h e(i@1)"', 'null'));
  await page.keyboard.press('Enter');
  const c = cell(page, 0);
  await c.getByTestId('cx-deform').click();
  await expect(c.getByTestId('deform-needs-truncate')).toContainText('truncate=h^N');
  await expect(c.getByTestId('deform-solve')).toHaveCount(0);
});

test('solving, an obstruction, and the corrected complex as a new cell', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'klrw type=A1 ring=QQ red=2i@0 truncate=h^2 hdeg=2');
  await page.keyboard.insertText(complexText);
  await page.keyboard.press('Enter');
  const c = cell(page, 0);
  // d² = h x_1 ≠ 0 modulo h³: the cell is not a complex, yet the solver is offered
  await expect(c.getByTestId('verdict')).toContainText('not a complex');
  await c.getByTestId('cx-deform').click();
  await expect(c.getByTestId('deform-diff')).toHaveCount(2);
  await expect(c.getByTestId('deform-solve')).toBeDisabled();

  // only the map from −1 may change: order 1 has no solution, and its composite is marked
  await c.getByTestId('deform-whole').nth(1).check();
  await c.getByTestId('deform-solve').click();
  await expect(c.getByTestId('deform-verdict')).toHaveText('obstructed at order 1');
  await expect(c.getByTestId('deform-failure')).toBeVisible();
  await expect(c.locator('.cx-arrow[data-bad="true"]').first()).toBeVisible();
  await expect(c.getByTestId('deform-add')).toHaveCount(0);

  // the map from −2 instead: corrected by −h e(i@1), with no freedom
  await c.getByTestId('deform-whole').nth(1).uncheck();
  await c.getByTestId('deform-entry').first().check();
  await c.getByTestId('deform-solve').click();
  await expect(c.getByTestId('deform-verdict')).toHaveText('solved');
  await expect(c.getByTestId('deform-correction').first()).toContainText('-h e(i@1)');
  await expect(c.getByTestId('deform-order').first()).toContainText('No freedom');
  await c.getByTestId('deform-add').click();
  const n = cell(page, 1);
  await expect(n.locator('.head .kind')).toHaveText('complex');
  await expect(n.getByTestId('complex-status')).toContainText('d² = 0');
  // the original cell is untouched
  await expect(c).toContainText('not a complex');
});
