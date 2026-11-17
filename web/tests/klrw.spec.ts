import { expect, test } from '@playwright/test';
import { addCell, cell, newDoc, setContext, typeCell } from './helpers';

test('weighted KLR: definitions are normal forms, checks, quotient, and errors', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'klrw type=A2 theta_ij=-1/2 red="i@-1" quotient=cyclotomic');
  await typeCell(page, 'def e := e(i@0 j@1)');
  await addCell(page, 'check primitive(e)');
  await addCell(page, 'watch x_1 e');
  await addCell(page, 'watch to(j@0 i@1) e');
  await expect(cell(page, 1).locator('.head > .kind + .outcome')).toHaveText('holds');
  // the dot right of a red strand of weight i is the red bigon, zero in the quotient
  await expect(cell(page, 2).getByTestId('summary')).toHaveText('0');
  // a straight line to other colours is a typo, not zero
  await expect(cell(page, 3)).toContainText('no part of it has the same colours');
  // a definition is kept in the normal form, so its badge is the certificate, not "as written"
  await cell(page, 0).locator('.head').click();
  await expect(cell(page, 0).getByTestId('badge')).not.toContainText('as written');
});

test('weighted KLR canvas offers no rewriting', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'klrw type=A2 theta_ij=1/2');
  await typeCell(page, 'def b := to(j@0 i@1/4) to(j@0 i@1) e(j@0 i@1/4)');
  await cell(page, 0).getByTestId('open-canvas').click();
  await expect(page.getByText('Each change is put back into the basis normal form at once.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Relations' })).toHaveCount(0);
  // ghosts are drawn in the canvas
  await expect(page.locator('.canvas path.ghost, .canvas line.ghost, .canvas .ghost').first()).toBeVisible();
});
