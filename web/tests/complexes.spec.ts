import { expect, test } from '@playwright/test';
import { cell, newDoc, setContext } from './helpers';

// A projective resolution built with the tools of the cells, one step at a time: the simple
// module L = P/rad P at i@1, where e(i@1) A e(i@1) = k[x]/(x^2), so every kernel is again a copy of L.
test('resolving a simple module step by step with the cell tools', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'klrw type=A1 ring=QQ red=2i@0 quotient=cyclotomic');
  await page.keyboard.insertText('module L := {"head": "i@1"}');
  await page.keyboard.press('Enter');
  const m = cell(page, 0);
  // collapsed or not, the cell reads as a module, not as JSON
  await expect(m.locator('.head .cx-head')).toHaveCount(0);
  await expect(m.getByTestId('module-view')).toBeVisible();
  // projectives read as their strands, the red ones in red, each with its position
  await expect(m.getByTestId('module-view')).toContainText('P(2i0 i1)⟨2⟩');
  await expect(m.locator('.cx-red').first()).toHaveText(/2i/);
  await expect(m.getByTestId('module-view')).toContainText('graded dimension: 1 at i@1');
  await m.getByTestId('md-resolve').click();

  // a complex cell below it: the generators of L at 0, the augmentation onto L
  const r = cell(page, 1);
  await expect(r.locator('.head .kind')).toHaveText('complex');
  await expect(r.getByTestId('resolution-status')).toContainText('resolution in progress');
  await expect(r.getByTestId('cx-module')).toHaveText(/L/);
  await expect(r.getByTestId('cx-term')).toHaveCount(1);

  await r.getByTestId('cx-extend').click();
  await expect(r.getByTestId('cx-term')).toHaveCount(2);
  await expect(r.getByTestId('resolution-status')).toContainText('length 1');
  // a 1×1 differential is drawn on its arrow
  await expect(r.getByTestId('cx-entry').first().locator('svg')).toBeVisible();
  await r.getByTestId('cx-extend').click();
  await expect(r.getByTestId('cx-term')).toHaveCount(3);
  await expect(r.getByTestId('cx-extend')).toContainText('−3');
  // the cell still names the module cell, so changing L changes what it resolves
  await expect(r).toHaveAttribute('data-kind', 'complex');
  expect(await page.evaluate(() => document.querySelector('.cell[data-kind="complex"] [data-testid="cell-display"]')?.getAttribute('data-text'))).toContain('"module":"L"');
});
