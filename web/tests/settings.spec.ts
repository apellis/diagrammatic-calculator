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

test('AI settings abbreviate the token limits, and the key can be shown and hidden from inside its box', async ({ page }) => {
  await page.goto('/#/settings');
  const budget = page.getByTestId('ai-budget');
  await expect(budget.locator('option')).toHaveText(['250K tokens', '1M tokens', '4M tokens', 'no limit']);
  // display only: the stored values are the numbers
  expect(await budget.locator('option').evaluateAll((os) => os.map((o) => (o as HTMLOptionElement).value))).toEqual(['250000', '1000000', '4000000', '0']);

  const key = page.getByLabel('API key');
  await key.fill('sk-ant-not-a-real-key');
  await expect(key).toHaveAttribute('type', 'password');
  const show = page.getByRole('button', { name: 'Show key' });
  await expect(show).toHaveAttribute('aria-pressed', 'false');
  // inside the box, at its right end
  const box = (await key.boundingBox())!, b = (await show.boundingBox())!;
  expect(b.x + b.width).toBeLessThanOrEqual(box.x + box.width);
  expect(b.x).toBeGreaterThan(box.x + box.width / 2);
  expect(b.y).toBeGreaterThanOrEqual(box.y);
  expect(b.y + b.height).toBeLessThanOrEqual(box.y + box.height);

  await show.click();
  await expect(key).toHaveAttribute('type', 'text');
  await expect(key).toHaveValue('sk-ant-not-a-real-key');
  const hide = page.getByRole('button', { name: 'Hide key' });
  await expect(hide).toHaveAttribute('aria-pressed', 'true');
  await hide.click();
  await expect(key).toHaveAttribute('type', 'password');

  // from the keyboard: Tab from the key reaches the toggle, Space and Enter work it
  await key.focus();
  await page.keyboard.press('Tab');
  await expect(show).toBeFocused();
  await page.keyboard.press('Space');
  await expect(key).toHaveAttribute('type', 'text');
  await page.keyboard.press('Enter');
  await expect(key).toHaveAttribute('type', 'password');
  // showing the key stores it nowhere: nothing is kept until Save AI settings
  const stored = await page.evaluate(() => JSON.stringify({ ...localStorage }) + JSON.stringify({ ...sessionStorage }));
  expect(stored).not.toContain('sk-ant-not-a-real-key');
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

test('Settings lists the shortcuts and rebinds the main ones, refusing clashes; the new keys work in a document', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('open-settings').click();
  const keys = page.getByTestId('shortcuts');
  await expect(keys.getByTestId('keys-palette')).toHaveText(/K$/);
  await expect(keys.locator('tr.fixed')).toContainText(['Undo']);

  // a clash with another action is refused and the old keys stay
  await keys.getByTestId('rebind-palette').click();
  await page.keyboard.press('ControlOrMeta+j');
  await expect(keys.getByTestId('rebind-error')).toContainText('AI panel');
  await expect(keys.getByTestId('keys-palette')).toHaveText(/K$/);
  // a plain key for an action that works while typing is refused too
  await page.keyboard.press('p');
  await expect(keys.getByTestId('rebind-error')).toContainText('while you type');
  await page.keyboard.press('ControlOrMeta+Shift+p');
  await expect(keys.getByTestId('rebind-palette')).toHaveText('Change');
  await expect(keys.getByTestId('keys-palette')).toHaveText(/P$/);
  await expect(keys.getByTestId('keys-palette')).toHaveText(/Shift|⇧/);
  // a key for an action outside editors may be bare
  await keys.getByTestId('rebind-canvas').click();
  await page.keyboard.press('g');
  await expect(keys.getByTestId('keys-canvas')).toHaveText('G');

  // kept across a reload, and in force in a document
  await page.reload();
  await expect(page.getByTestId('keys-palette')).toHaveText(/P$/);
  await page.getByTestId('settings-back').click();
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await page.keyboard.press('ControlOrMeta+k');
  await expect(page.getByRole('dialog', { name: 'Command palette' })).toHaveCount(0);
  await page.keyboard.press('ControlOrMeta+Shift+p');
  const palette = page.getByRole('dialog', { name: 'Command palette' });
  await expect(palette).toBeVisible();
  await expect(palette).toContainText('Open the active cell in the canvas');
  await page.keyboard.press('Escape');
  await cell(page, 0).locator('.sum').click();
  await page.keyboard.press('g');
  await expect(page.locator('.canvas')).toBeVisible();
  await page.keyboard.press('Escape');

  // reset brings the defaults back
  await page.goto('/#/settings');
  await page.getByTestId('shortcuts-reset').click();
  await expect(page.getByTestId('keys-palette')).toHaveText(/K$/);
  await expect(page.getByTestId('keys-canvas')).toHaveText('F');
});
