import { expect, test, type Page } from '@playwright/test';
import { cell, newDoc, typeCell } from './helpers';

const background = (page: Page) => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
const DARK = 'rgb(22, 22, 24)';
const LIGHT = 'rgb(251, 251, 250)';

test('the theme follows the system unless Settings chooses light or dark, and the choice is kept', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('/');
  await expect.poll(() => background(page)).toBe(DARK);
  await page.getByTestId('open-settings').click();
  const sw = page.getByTestId('theme-switch');
  await expect(sw.getByRole('radio', { name: 'System' })).toHaveAttribute('aria-checked', 'true');

  await sw.getByRole('radio', { name: 'Light' }).click();
  await expect.poll(() => background(page)).toBe(LIGHT);
  await page.reload();
  await expect.poll(() => background(page)).toBe(LIGHT);
  await expect(page.getByTestId('theme-switch').getByRole('radio', { name: 'Light' })).toHaveAttribute('aria-checked', 'true');

  // the arrow keys move the slider: System follows the system again, Dark overrides it
  await page.getByTestId('theme-switch').getByRole('radio', { name: 'Light' }).focus();
  await page.keyboard.press('ArrowRight');
  await expect.poll(() => background(page)).toBe(DARK);
  expect(await page.evaluate(() => document.documentElement.dataset.theme ?? null)).toBeNull();
  await page.keyboard.press('ArrowRight');
  await page.emulateMedia({ colorScheme: 'light' });
  await expect.poll(() => background(page)).toBe(DARK);
  await expect(page.getByTestId('theme-switch').getByRole('radio', { name: 'Dark' })).toBeFocused();

  await page.getByTestId('settings-back').click();
  await expect(page.getByRole('button', { name: 'New document' })).toBeVisible();
});

test('with no AI set up, the panel links to Settings, whose Back returns to the document', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await page.getByTestId('ai-toggle').click();
  await expect(page.getByTestId('ai-setup')).toBeVisible();
  await expect(page.getByTestId('ai-model')).toHaveText('not set up');
  // the panel has no settings form of its own
  await expect(page.getByLabel('API key')).toHaveCount(0);
  await page.getByTestId('ai-setup-link').click();
  await expect(page.getByLabel('API key')).toBeVisible();
  await expect(page.getByTestId('settings-back')).toHaveText('← Back to the document');
  await page.getByTestId('settings-back').click();
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
});
