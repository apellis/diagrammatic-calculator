import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';

test('small-phone settings keep shortcut controls inside the viewport', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 740 });
  await page.goto('/#/settings');
  await expect(page.getByTestId('shortcuts')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  const change = page.getByTestId('shortcuts').getByRole('button', { name: 'Change', exact: true }).first();
  const bounds = (await change.boundingBox())!;
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(320);
});

test('phone document actions fit beside a readable name and export a reusable workspace', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.locator('[data-example="nilhecke-idempotents"]').click();
  await expect(page).toHaveURL(/#\/doc\//);
  await page.goto('/#/');
  const row = page.locator('tr[data-doc]').first();
  await expect(row).toContainText('nilhecke-idempotents');
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  expect((await row.locator('.open').boundingBox())!.width).toBeGreaterThan(200);
  for (const name of ['Rename', 'Duplicate', 'Export', 'Delete']) {
    const action = row.getByRole('button', { name, exact: true });
    await expect(action).toBeVisible();
    const box = (await action.boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(390);
  }
  await page.screenshot({ path: testInfo.outputPath('phone-documents.png'), fullPage: true });
  const downloading = page.waitForEvent('download');
  await row.getByRole('button', { name: 'Export', exact: true }).click();
  const file = await downloading;
  expect(file.suggestedFilename()).toBe('nilhecke-idempotents.dcw');
  const text = await readFile((await file.path())!, 'utf8');
  expect(text).toContain('context nilhecke');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Import .dcw or .dch' }).click();
  await (await chooser).setFiles({ name: file.suggestedFilename(), mimeType: 'text/plain', buffer: Buffer.from(text) });
  await expect(page).toHaveURL(/#\/doc\//);
  await page.goto('/#/');
  await expect(page.locator('tr[data-doc]')).toHaveCount(2);
});

test('AI settings explain saving and guard Back without autosaving', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/#/settings');
  await expect(page.getByText('Appearance, shortcuts, and the context confirmation save automatically.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Save AI settings', exact: true })).toBeVisible();
  const budget = page.getByTestId('ai-budget');
  await budget.selectOption('250000');
  await expect(page.getByTestId('ai-dirty')).toHaveText('Unsaved AI settings');
  await page.getByTestId('patterns').check();
  await page.getByTestId('settings-back').click();
  const guard = page.getByRole('dialog', { name: 'Unsaved AI settings' });
  await expect(guard).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('phone-unsaved-settings.png'), fullPage: true });
  await guard.getByRole('button', { name: 'Keep editing' }).click();
  await expect(budget).toHaveValue('250000');
  await page.getByTestId('settings-back').click();
  await guard.getByRole('button', { name: 'Discard changes' }).click();
  await page.getByTestId('open-settings').click();
  await expect(budget).toHaveValue('1000000');
  await expect(page.getByTestId('patterns')).toBeChecked();
  await budget.selectOption('250000');
  await page.getByTestId('ai-save').click();
  await expect(page.getByTestId('ai-saved')).toBeVisible();
  await expect(page.getByTestId('ai-dirty')).toHaveCount(0);
  await page.getByTestId('settings-back').click();
  await page.getByTestId('open-settings').click();
  await expect(budget).toHaveValue('250000');
  await budget.selectOption('1000000');
  await budget.selectOption('250000');
  await expect(page.getByTestId('ai-dirty')).toHaveCount(0);
});

test('landing explains copies and Help gives practical first steps', async ({ page }, testInfo) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Start from an example' })).toBeVisible();
  await expect(page.getByText('Each example opens as your own editable copy. The original stays unchanged.')).toBeVisible();
  await page.getByRole('link', { name: 'Getting started' }).click();
  await expect(page).toHaveURL(/#\/help$/);
  await expect(page.getByRole('heading', { name: 'Getting started' })).toBeVisible();
  await expect(page.locator('.getting-started')).toContainText('Watch');
  await expect(page.locator('.getting-started')).toContainText('.dcw');
  await page.screenshot({ path: testInfo.outputPath('help.png'), fullPage: true });
});

test('dirty AI draft survives browser Back and hash navigation until discarded', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('open-settings').click();
  await page.getByTestId('ai-budget').selectOption('250000');
  await page.goBack();
  const guard = page.getByRole('dialog', { name: 'Unsaved AI settings' });
  await expect(guard).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('ai-budget')).toHaveValue('250000');
  await expect(page).toHaveURL(/#\/settings$/);
  await page.evaluate(() => { location.hash = '#/about'; });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Discard changes' }).click();
  await expect(page).toHaveURL(/#\/about$/);
  // The hash changes before the router handles it; wait for Settings to actually unmount.
  await expect(page.getByTestId('about-commit')).toBeVisible();
  await page.goto('/#/settings');
  await expect(page.getByTestId('ai-budget')).toHaveValue('1000000');
});

test('AI draft warns on reload, and narrow document rows retain rename controls', async ({ page }) => {
  await page.goto('/#/settings');
  await page.getByTestId('ai-budget').selectOption('250000');
  const warning = page.waitForEvent('dialog');
  await page.evaluate(() => { setTimeout(() => location.reload(), 0); });
  const dialog = await warning;
  expect(dialog.type()).toBe('beforeunload');
  await dialog.dismiss();
  await expect(page.getByTestId('ai-budget')).toHaveValue('250000');
  await page.getByTestId('settings-back').click();
  await page.getByRole('button', { name: 'Discard changes' }).click();
  await page.setViewportSize({ width: 320, height: 740 });
  await page.locator('[data-example="nilhecke-idempotents"]').click();
  await expect(page).toHaveURL(/#\/doc\//);
  await page.goto('/#/');
  const row = page.locator('tr[data-doc]').first();
  await row.getByRole('button', { name: 'Rename', exact: true }).click();
  await page.getByLabel('Document name').fill('A long document name that should wrap and remain readable');
  await page.getByLabel('Document name').press('Enter');
  await expect(row).toContainText('A long document name');
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  await row.getByRole('button', { name: 'Delete', exact: true }).click();
  await page.getByRole('button', { name: 'Keep it' }).click();
  await expect(row).toContainText('A long document name');
});
