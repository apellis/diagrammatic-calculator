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
  // an entry of a map is edited in place: the hint names its source, target, and degree
  await r.getByTestId('cx-entry-edit').first().click();
  const input = r.getByTestId('cx-entry-input');
  await expect(input).toBeFocused();
  await expect(r.locator('.cx-edit-hint')).toContainText('of degree');
  await page.keyboard.press('Escape');
  await expect(input).toHaveCount(0);
  await r.getByTestId('cx-entry-edit').first().click();
  await input.fill('0');
  await page.keyboard.press('Enter');
  await expect.poll(async () => page.evaluate(() => document.querySelector('.cell[data-kind="complex"] [data-testid="cell-display"]')?.getAttribute('data-text'))).toContain('[[null]]');
  // the cell still names the module cell, so changing L changes what it resolves
  await expect(r).toHaveAttribute('data-kind', 'complex');
  expect(await page.evaluate(() => document.querySelector('.cell[data-kind="complex"] [data-testid="cell-display"]')?.getAttribute('data-text'))).toContain('"module":"L"');
});

// A plain complex says in one line whether d² = 0 and where it is not exact, and after an edit
// what it said before.
test('the status line of a complex, before and after editing an entry', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'klrw type=A1 ring=QQ red=2i@0 quotient=cyclotomic');
  await page.keyboard.insertText('complex C := {"terms":[{"at":-1,"summands":[{"object":"i@1","shift":2}]},{"at":0,"summands":[{"object":"i@1"}]}],"differentials":[{"from":-1,"matrix":[["x_1 e(i@1)"]]}]}');
  await page.keyboard.press('Enter');
  const c = cell(page, 0);
  const status = c.getByTestId('complex-status');
  await expect(status).toContainText('d² = 0, so this is a complex');
  await expect(status).toContainText('not zero at −1 (q^4 at i@1), 0 (1 at i@1)');
  await expect(c.getByTestId('status-before')).toHaveCount(0);
  await c.getByTestId('cx-entry-edit').first().click();
  await c.getByTestId('cx-entry-input').fill('0');
  await page.keyboard.press('Enter');
  await expect(status).toContainText('not zero at −1 (q^2 + q^4 at i@1)');
  await expect(c.getByTestId('status-before')).toContainText('Before the last change: not exact: the homology is not zero at −1 (q^4 at i@1)');
});

// A chain map is a cell naming its source and target: drawn as a ladder, its components edited in
// place, its status checked after every edit, and its cone one click away.
test('a chain map cell: edit a component, see the check, take the cone', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'klrw type=A1 ring=QQ red=2i@0 quotient=cyclotomic');
  await page.keyboard.insertText('complex C := {"terms":[{"at":-1,"summands":[{"object":"i@1","shift":2}]},{"at":0,"summands":[{"object":"i@1"}]}],"differentials":[{"from":-1,"matrix":[["x_1 e(i@1)"]]}]}');
  await page.keyboard.press('Enter');
  await expect(cell(page, 0).getByTestId('complex-status')).toBeVisible();
  await page.getByTestId('add-cell').click();
  await page.keyboard.insertText('chainmap F := {"source":"C","target":"C","map":[]}');
  await page.keyboard.press('Enter');
  const f = cell(page, 1);
  await expect(f).toHaveAttribute('data-kind', 'chainmap');
  await expect(f.getByTestId('chainmap-view')).toBeVisible();
  // the zero map is a chain map
  await expect(f.getByTestId('chainmap-status').getByTestId('verdict')).toHaveText('a chain map');
  await expect(f.getByTestId('cm-component')).toHaveCount(2);
  // a component at 0 alone is not: dF ≠ Fd from −1
  await f.getByTestId('cm-component').nth(1).getByTestId('cx-entry-edit').click();
  await f.getByTestId('cx-entry-input').fill('e(i@1)');
  await page.keyboard.press('Enter');
  await expect(f.getByTestId('chainmap-status').getByTestId('verdict')).toHaveText('not a chain map');
  await expect(f.getByTestId('cm-cone')).toHaveCount(0);
  // the identity is
  await f.getByTestId('cm-component').nth(0).getByTestId('cx-entry-edit').click();
  await f.getByTestId('cx-entry-input').fill('e(i@1)');
  await page.keyboard.press('Enter');
  await expect(f.getByTestId('chainmap-status').getByTestId('verdict')).toHaveText('a chain map');
  expect(await page.evaluate(() => document.querySelector('.cell[data-kind="chainmap"] [data-testid="cell-display"]')?.getAttribute('data-text'))).toContain('"source":"C"');
  await f.getByTestId('cm-cone').click();
  const cone = cell(page, 2);
  await expect(cone.locator('.head .kind')).toHaveText('complex');
  await expect(cone.getByTestId('cx-term')).toHaveCount(3);
});

// The arrows of a complex lie on one line, whatever is drawn above them (a diagram, a coefficient,
// a matrix chip) and however tall the terms are.
test('the arrows of a complex are at one height', async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await newDoc(page);
  await setContext(page, 'klrw type=A1 ring=QQ red=2i@0 quotient=cyclotomic');
  await page.keyboard.insertText('complex C := {"terms":[{"at":-3,"summands":[{"object":"i@1","shift":6}]},{"at":-2,"summands":[{"object":"i@1","shift":4},{"object":"i@1","shift":4}]},{"at":-1,"summands":[{"object":"i@1","shift":2}]},{"at":0,"summands":[{"object":"i@1"}]}],"differentials":[{"from":-3,"matrix":[["x_1 e(i@1)"],["0"]]},{"from":-2,"matrix":[["0","0"]]},{"from":-1,"matrix":[["(2/3) x_1 e(i@1)"]]}]}');
  await page.keyboard.press('Enter');
  const c = cell(page, 0);
  await expect(c.getByTestId('complex-status')).toBeVisible();
  // the last arrow carries a drawn diagram with its coefficient, the others a matrix chip
  await expect(c.getByTestId('cx-entry').first().locator('svg')).toBeVisible();
  const shafts = c.locator('.cx-diagram .cx-arrow:not(.cx-down) > .cx-shaft');
  await expect(shafts).toHaveCount(3);
  const mids = await shafts.evaluateAll((es) => es.map((e) => { const r = e.getBoundingClientRect(); return r.top + r.height / 2; }));
  for (const y of mids) expect(Math.abs(y - mids[0])).toBeLessThan(1);
});
