import { expect, test } from '@playwright/test';
import { cell, newDoc, setContext, typeCell } from './helpers';

test('ordinary provenance is a quiet disclosure with the exact certificate inside', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'watch x_1 e(iji)');
  const v = cell(page, 0);
  const badge = v.getByTestId('badge');
  await expect(badge).toHaveText('How computed ▾');
  await expect(badge).not.toHaveClass(/good|warn/);
  await expect(badge).toHaveAttribute('aria-expanded', 'false');
  await expect(v.getByTestId('provenance')).toHaveCount(0);
  await badge.focus();
  await page.keyboard.press('Enter');
  await expect(badge).toHaveAttribute('aria-expanded', 'true');
  await expect(v.getByTestId('provenance')).toContainText('faithful representation');
  await expect(v.getByTestId('provenance')).toContainText('proven in the literature');
  await expect(v.getByTestId('cert-reference')).not.toBeEmpty();
  await expect(v.getByTestId('cert-hypotheses')).not.toBeEmpty();
  await badge.click();
  await expect(v.getByTestId('provenance')).toHaveCount(0);
});

test('KL results show a literature certificate; odd BE2 conclusions say they are conditional in comparisons and copies too', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await newDoc(page);
  await setContext(page, 'kl type=A1 mu=2');
  await typeCell(page, 'watch x_1 e(i)');
  const kl = cell(page, 0);
  await expect(kl.getByTestId('summary')).toHaveText(/term/);
  await expect(kl.locator('.cond-mark')).toHaveCount(0);
  await kl.getByTestId('badge').click();
  await expect(kl.getByTestId('provenance').locator('tr').filter({ has: page.locator('td', { hasText: /^certificate$/ }) })).toHaveText('certificateproven in the literature');
  await expect(kl.getByTestId('cert-reference')).toContainText('arXiv:0807.3250');

  await page.getByTestId('context').click();
  await page.getByLabel('Context').fill('be2 type=A1 odd=i mu=-1');
  await page.getByLabel('Context').press('Enter');
  await expect(page.getByTestId('context').locator('.ctx')).toHaveAttribute('data-text', /^be2\b/);
  await expect(kl.locator('.cond-mark')).toHaveText('Assuming nondegeneracy');
  // a copied picture of the normal form carries its caveat
  await expect(kl.locator('.head')).toHaveAttribute('aria-expanded', 'true');
  await kl.getByRole('button', { name: 'Copy TikZ' }).click();
  await expect(page.locator('.topbar-msg')).toHaveText('TikZ copied.');
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied.split('\n')[0]).toMatch(/^% Normal form by basis normal form \(skm-basis\); conditional on the Nondegeneracy Conjecture/);

  // comparing a branch shows the values with the same mark
  await page.locator('.main').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('ControlOrMeta+b');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('branch')).toBeVisible();
  await kl.locator('.head').click();
  await page.locator('.cell.active .cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await typeCell(page, 'watch x_1 x_1 e(i)');
  await page.getByRole('button', { name: 'Accept…' }).click();
  const values = page.getByTestId('value-changes');
  await expect(values.getByTestId('brief-mark')).toHaveText(['Assuming nondegeneracy', 'Assuming nondegeneracy']);
});
