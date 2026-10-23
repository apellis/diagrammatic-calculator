import { expect, test } from '@playwright/test';

test('the About page shows the build and links the privacy policy, which the start page does not', async ({ page, context }) => {
  await page.goto('/');
  await expect(page.locator('a[href="/privacy"]')).toHaveCount(0);
  await page.getByTestId('open-about').click();
  await expect(page).toHaveURL(/#\/about$/);
  await expect(page.getByTestId('about-commit')).toHaveText(/^[0-9a-f]{7}$|^unknown$/);

  const opened = context.waitForEvent('page');
  await page.getByTestId('about-privacy').click();
  const policy = await opened;
  await expect(policy.getByRole('heading', { name: 'Privacy policy' })).toBeVisible();
  await expect(policy.getByText('Signing in is optional.')).toBeVisible();

  await page.getByTestId('about-back').click();
  await expect(page.getByTestId('open-about')).toBeVisible();
});
