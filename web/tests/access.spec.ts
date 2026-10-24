import { expect, test } from '@playwright/test';
import { addCell, cell, newDoc, setContext, typeCell } from './helpers';

/** The page is no wider than the window: nothing scrolls sideways. */
async function fitsWidth(page: import('@playwright/test').Page) {
  const [scroll, inner] = await page.evaluate(() => [document.documentElement.scrollWidth, window.innerWidth]);
  expect(scroll).toBeLessThanOrEqual(inner);
}

test('at phone width the pages fit the window and the cells show', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'New document' })).toBeVisible();
  await fitsWidth(page);
  await page.getByRole('button', { name: 'New document' }).click();
  await fitsWidth(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page.locator('.cm-content')).toBeFocused();
  await typeCell(page, 'watch x_1 e(iji)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term, degree 2');
  await expect(cell(page, 0)).toBeInViewport();
  await fitsWidth(page);
  // the outline is a drawer: closed at first, opened by ☰, closed again by choosing a cell
  await expect(page.locator('.outline')).toHaveCount(0);
  await page.getByRole('button', { name: 'Show the outline' }).click();
  await page.locator('.outline .oitem').first().click();
  await expect(page.locator('.outline')).toHaveCount(0);
});

test('cells, the outline, and the document list are reachable and operable by keyboard', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iji)');
  await addCell(page, 'watch a a');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term, degree 4');
  await page.keyboard.press('Escape');
  await page.locator('.main').click({ position: { x: 5, y: 5 } });

  // Tab reaches the first cell's head without stopping on invisible inserters
  const head0 = cell(page, 0).locator('.head');
  await head0.focus();
  await expect(head0).toHaveAttribute('aria-expanded', 'false');
  await page.keyboard.press('Enter');
  await expect(head0).toHaveAttribute('aria-expanded', 'true');
  await expect(cell(page, 0).getByTestId('cell-display')).toBeVisible();
  // ↓ moves to the next cell's head, Space opens it
  await page.keyboard.press('ArrowDown');
  await expect(cell(page, 1).locator('.head')).toBeFocused();
  await page.keyboard.press(' ');
  await expect(cell(page, 1).locator('.head')).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('Tab');
  await expect(cell(page, 1).getByTestId('cell-display')).toBeFocused();
  expect(await page.locator('.insert button[tabindex="-1"]').count()).toBeGreaterThan(0);

  // Alt+Enter on a head inserts a cell below it
  await cell(page, 0).locator('.head').focus();
  await page.keyboard.press('Alt+Enter');
  await expect(page.locator('.cell[data-cell="new"] .cm-content')).toBeFocused();
  await page.keyboard.type('watch x_2 e(iji)');
  await page.keyboard.press('Enter');
  await expect(cell(page, 1).locator('.head .text')).toHaveAttribute('data-text', 'x_2 e(iji)');

  // outline rows are buttons with names; the tabs are a tab list
  await expect(page.getByRole('tab', { name: 'Cells' })).toHaveAttribute('aria-selected', 'true');
  await page.locator('.outline .oitem').nth(2).focus();
  await page.keyboard.press('Enter');
  await expect(cell(page, 2)).toHaveClass(/active/);

  // the document name renames in place; the list links each document
  await page.getByRole('button', { name: /Rename the document/ }).click();
  await page.getByLabel('Document name').fill('Keys');
  await page.getByLabel('Document name').press('Enter');
  await expect(page.locator('.topbar .name')).toHaveText('Keys');
  await page.getByRole('button', { name: 'All documents' }).click();
  await expect(page.getByRole('link', { name: 'Keys' })).toHaveAttribute('href', /#\/doc\//);
});

test('a conditional certificate is named, and says which verdict rests on it', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'be2 type=A1 odd=i mu=-1');
  await typeCell(page, 'check zero(psi_1 psi_1 e(ii))');
  await addCell(page, 'check zero(x_1 e(i))');
  await addCell(page, 'watch x_1 e(i)');
  // holds: proven by the computation, so no mark; fails: conditional, in the summary and the outline
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('holds');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('fails');
  await expect(cell(page, 0).locator('.cond-mark')).toHaveCount(0);
  await expect(cell(page, 1).locator('.cond-mark')).toHaveText('Assuming nondegeneracy');
  await expect(cell(page, 2).locator('.cond-mark')).toHaveText('Assuming nondegeneracy');
  await expect(page.locator('.outline .cond-mark')).toHaveCount(2);
  await cell(page, 0).locator('.head').click();
  await expect(cell(page, 0).getByTestId('verdict-note')).toHaveText('unconditional');
  await cell(page, 1).locator('.head').click();
  await expect(cell(page, 1).getByTestId('verdict-note')).toHaveText('Assuming nondegeneracy');
  await expect(cell(page, 2).getByTestId('cond-text')).toHaveText('Assuming nondegeneracy');
  await expect(cell(page, 2).getByTestId('badge')).not.toHaveClass(/warn/);
  // the badge opens on the reference and hypotheses, not a falsely upgraded certificate
  await cell(page, 1).getByTestId('badge').click();
  await expect(cell(page, 1).getByTestId('provenance')).toContainText('arXiv:1701.04133');
  await expect(cell(page, 1).getByTestId('provenance').locator('tr').filter({ has: page.locator('td', { hasText: /^certificate$/ }) })).toHaveText('certificateconditional');
  await expect(cell(page, 1).getByTestId('provenance')).toContainText('Nondegeneracy Conjecture');
  // the presentation file's prose is typeset: its formulas through KaTeX, Lean names as code
  await expect(cell(page, 1).getByTestId('cert-hypotheses').locator('.katex').first()).toBeVisible();
  await expect(cell(page, 1).getByTestId('cert-reference').locator('code').first()).toBeVisible();
});

test('a long computation shows how long it has run and stops from its cell', async ({ page }) => {
  await newDoc(page);
  await setContext(page, 'nilhecke nu=6');
  const sum = '(x_1 + x_2 + x_3 + x_4 + x_5 + x_6 + psi_1 + psi_3 + psi_5)';
  await typeCell(page, `watch ${Array(24).fill(sum).join(' ')}`);
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('computing…');
  await expect(cell(page, 0).locator('.head .elapsed')).toHaveText(/^\d+ s$/);
  // the new cell is open: its running row has the Stop button
  await cell(page, 0).getByTestId('stop').click();
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('cancelled');
  await expect(page.getByTestId('light')).toBeDisabled();
});

test('a new document starts from a chosen presentation, with its options as fields', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'New document' }).click();
  const d = page.getByRole('dialog', { name: 'New document' });
  await expect(d.getByLabel('Name')).toBeFocused();
  await d.getByLabel('Name').fill('Odd bubbles');
  await d.getByLabel('Presentation', { exact: true }).selectOption('be2');
  // a 2-category: the weight is μ, and the Cartan type and odd colours are fields
  await expect(d.getByLabel('Context line')).toHaveValue('be2 type=A2 mu=i ring=QQ');
  await d.getByLabel('Cartan type').fill('A1');
  await d.getByText('Quotient and parity', { exact: true }).click();
  await d.getByLabel('Odd colours').fill('i');
  await d.getByLabel(/Weight/).fill('-1');
  await expect(d.getByLabel('Context line')).toHaveValue('be2 type=A1 mu=-1 ring=QQ odd=i');
  // a bad value is refused with the engine's reason, and nothing is created
  await d.getByLabel('Cartan type').fill('Z9');
  await d.getByRole('button', { name: 'Create' }).click();
  await expect(d.locator('.error')).toContainText('the types are');
  await d.getByLabel('Cartan type').fill('A1');
  await d.getByRole('button', { name: 'Create' }).click();
  await expect(page.locator('.topbar .name')).toHaveText('Odd bubbles');
  await expect(page.getByTestId('context').locator('.ctx')).toHaveAttribute('data-text', /^be2 type=A1 odd=i ring=QQ mu=-1$/);
});

test('strands are coloured by colour in cells, and the patterns setting dashes them everywhere', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'watch psi_2 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term, degree 1');
  // the cell just added is open
  const stroke = (k: string) => cell(page, 0).locator(`.summand path.${k}`).first().evaluate((e) => getComputedStyle(e).stroke);
  expect(await stroke('k0')).toBe('rgb(47, 111, 223)');
  expect(await stroke('k1')).toBe('rgb(184, 92, 0)');
  const dash = () => cell(page, 0).locator('.summand path.k1').first().evaluate((e) => getComputedStyle(e).strokeDasharray);
  expect(await dash()).toBe('none');
  await page.goto('/#/settings');
  await page.getByTestId('patterns').check();
  await page.goBack();
  const head = cell(page, 0).locator('.head');
  if ((await head.getAttribute('aria-expanded')) !== 'true') await head.click();
  await expect(cell(page, 0).locator('.summand path.k1').first()).toBeVisible();
  expect(await dash()).not.toBe('none');
});
