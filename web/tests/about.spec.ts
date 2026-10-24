import { expect, test } from '@playwright/test';

test('the About page shows the build and links the tip page and the privacy policy, which the start page does not', async ({ page, context }) => {
  await page.goto('/');
  await expect(page.locator('a[href="/privacy"]')).toHaveCount(0);
  await page.getByTestId('open-about').click();
  await expect(page).toHaveURL(/#\/about$/);
  await expect(page.getByTestId('about-tip')).toHaveAttribute('href', 'https://buy.stripe.com/bJe28reyVa2081W86gaEE01');
  await expect(page.getByTestId('about-commit')).toHaveText(/^[0-9a-f]{7}$|^unknown$/);
  // The deploy workflow reads the full hash back from /build-info.json; it extends the shown one.
  const info = await (await page.request.get('/build-info.json')).json();
  expect(info.commit).toMatch(/^[0-9a-f]{40}$|^unknown$/);
  expect(info.commit.startsWith(await page.getByTestId('about-commit').innerText())).toBe(true);

  const opened = context.waitForEvent('page');
  await page.getByTestId('about-privacy').click();
  const policy = await opened;
  await expect(policy.getByRole('heading', { name: 'Privacy policy' })).toBeVisible();
  await expect(policy.getByText('Signing in is optional.')).toBeVisible();

  await page.getByTestId('about-back').click();
  await expect(page.getByTestId('open-about')).toBeVisible();
});

test('Help sits beside About and Settings, holds Getting started, and Back returns where it was opened', async ({ page }) => {
  await page.goto('/');
  const help = page.getByTestId('open-help');
  await expect(help).toHaveAttribute('title', 'Help');
  await expect(page.getByRole('link', { name: 'Help', exact: true })).toBeVisible();
  // the three icons sit together in the title bar, Help first
  const order = await page.locator('.titlebar a.button.icon').evaluateAll((as) => as.map((a) => a.getAttribute('aria-label')));
  expect(order).toEqual(['Help', 'About', 'Settings']);
  await help.click();
  await expect(page).toHaveURL(/#\/help$/);
  await expect(page.getByRole('heading', { name: 'Getting started' })).toBeVisible();
  await expect(page.locator('.getting-started')).toContainText('Check cell');
  // Back already goes to the document list; the page has no separate link to it
  await expect(page.getByRole('link', { name: 'Open the document list' })).toHaveCount(0);
  await page.getByTestId('help-back').click();
  await expect(page).toHaveURL(/#\/$|\/$/);
  await expect(page.getByTestId('open-help')).toBeVisible();

  // About no longer carries Getting started
  await page.getByTestId('open-about').click();
  await expect(page.getByTestId('about-commit')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Getting started' })).toHaveCount(0);
});
