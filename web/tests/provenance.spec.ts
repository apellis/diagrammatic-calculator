import { expect, test } from '@playwright/test';
import { cell, newDoc, typeCell } from './helpers';

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
