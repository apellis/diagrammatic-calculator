import { expect, test } from '@playwright/test';
import { cell, newDoc, typeCell } from './helpers';
import type { FromWorker, ToWorker } from '../src/engine/worker';

async function delayContext(page: import('@playwright/test').Page) {
  await page.evaluate(() => {
    const w = window as unknown as { contextStarted: number; contextFinished: number };
    w.contextStarted = w.contextFinished = 0;
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (m: ToWorker) {
      if (m.type !== 'call' || m.op !== 'context.describe') return post.call(this, m);
      w.contextStarted++;
      const receive = (e: MessageEvent<FromWorker>) => {
        if ((e.data.type === 'result' || e.data.type === 'failed') && e.data.id === m.id) {
          w.contextFinished++;
          this.removeEventListener('message', receive);
        }
      };
      this.addEventListener('message', receive);
      setTimeout(() => post.call(this, m), 700);
    };
  });
}

async function contextSettled(page: import('@playwright/test').Page) {
  await expect.poll(() => page.evaluate(() => {
    const w = window as unknown as { contextStarted: number; contextFinished: number };
    return w.contextStarted > 0 && w.contextFinished === w.contextStarted;
  })).toBe(true);
}

test('closing context options cancels an already submitted change', async ({ page }) => {
  await newDoc(page);
  const original = await page.getByTestId('context').locator('.ctx').getAttribute('data-text');
  await page.getByTestId('context').click();
  await delayContext(page);
  await page.getByLabel('Context', { exact: true }).fill('nilhecke nu=2 ring=QQ');
  await page.getByLabel('Context', { exact: true }).press('Enter');
  await page.keyboard.press('Escape');
  await contextSettled(page);
  await expect(page.getByTestId('context').locator('.ctx')).toHaveAttribute('data-text', original!);
});

test('editing a pending context change leaves the new draft open and the document unchanged', async ({ page }) => {
  await newDoc(page);
  const original = await page.getByTestId('context').locator('.ctx').getAttribute('data-text');
  await page.getByTestId('context').click();
  await delayContext(page);
  const input = page.getByLabel('Context', { exact: true });
  await input.fill('nilhecke nu=2 ring=QQ');
  await input.press('Enter');
  await input.fill('nilhecke nu=3 ring=QQ');
  await contextSettled(page);
  await expect(input).toBeVisible();
  await expect(page.getByTestId('context').locator('.ctx')).toHaveAttribute('data-text', original!);
  await input.press('Enter');
  await expect(page.getByTestId('context').locator('.ctx')).toHaveAttribute('data-text', 'nilhecke nu=3 ring=QQ');
});

async function phone(page: import('@playwright/test').Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await newDoc(page);
}

test('phone cell headers give expressions room rather than squeezing them behind summaries', async ({ page }) => {
  await phone(page);
  await typeCell(page, 'watch x_1^2 e(iji) + x_2^3 e(iji)');
  const head = cell(page, 0).locator('.head');
  await expect(head).toHaveAttribute('aria-expanded', 'true');
  await head.click();
  await expect(head).toHaveAttribute('aria-expanded', 'false');
  const expression = await head.locator('.text').boundingBox();
  expect(expression!.width).toBeGreaterThan(190);
  await expect(head.getByTestId('summary')).toHaveText('2 terms, degrees 4 to 6');
  await expect(head.getByTestId('summary')).toBeInViewport({ ratio: 1 });
});

test('phone popovers keep branch names and context text editable', async ({ page }) => {
  await phone(page);
  await page.getByTestId('context').click();
  await expect(page.getByLabel('Context', { exact: true })).toBeVisible();
  await page.getByLabel('Context', { exact: true }).fill('nilhecke nu=2 ring=QQ');
  await page.getByLabel('Context', { exact: true }).press('Enter');
  await expect(page.getByTestId('context')).toContainText('nilhecke');
  await page.getByTestId('branch-start').click();
  await expect(page.getByLabel('Branch name')).toBeVisible();
  await page.getByLabel('Branch name').fill('Try another diagram');
  await page.getByRole('button', { name: 'Start', exact: true }).click();
  await expect(page.getByTestId('branch')).toContainText('Try another diagram');
});

test('canvas editing and framing actions work without keyboard shortcuts', async ({ page }) => {
  await phone(page);
  await typeCell(page, 'def a := psi_1 psi_1 e(iij)');
  await cell(page, 0).getByTestId('open-canvas').click();
  const canvas = page.locator('.canvas');
  const frame = canvas.locator('.frame');
  await expect(frame).toHaveAttribute('data-word', 'psi_1^2 e(iij)');
  await canvas.getByRole('button', { name: 'Fit diagram', exact: true }).click();
  const transform = await canvas.locator('.canvas-board').getAttribute('style');
  await canvas.getByRole('button', { name: 'Zoom in', exact: true }).click();
  expect(await canvas.locator('.canvas-board').getAttribute('style')).not.toBe(transform);
  await canvas.getByRole('button', { name: 'Relations', exact: true }).click();
  await expect(canvas.getByRole('dialog', { name: 'Relations that apply' })).toBeVisible();
  await canvas.getByRole('button', { name: 'Close the relations' }).click();
  await expect(canvas.getByRole('button', { name: 'Remove selected pieces' })).toBeDisabled();
  await canvas.locator('.hit-piece').first().click();
  await canvas.getByRole('button', { name: 'Remove selected pieces' }).click();
  await expect(frame).toHaveAttribute('data-word', 'psi_1 e(iij)');
  await expect(canvas.getByTestId('canvas-message')).toContainText('changes the element');
});

for (const width of [390, 1280]) test(`gesture feedback never shifts the drawing at ${width}px`, async ({ page }) => {
  await page.setViewportSize({ width, height: 900 });
  await newDoc(page);
  await typeCell(page, 'def a := x_3 e(iij)');
  await cell(page, 0).getByTestId('open-canvas').click();
  const canvas = page.locator('.canvas');
  const dot = (await canvas.locator('.hit-piece[data-draw="dot"]').boundingBox())!;
  const before = (await canvas.locator('.canvas-view').boundingBox())!;
  const x = dot.x + dot.width / 2, y = dot.y + dot.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y - 12, { steps: 3 });
  await expect(canvas.getByTestId('canvas-message')).toContainText('Sliding the dot');
  expect((await canvas.locator('.canvas-view').boundingBox())!.y).toBe(before.y);
  const moved = (await canvas.locator('.live-dot').boundingBox())!;
  expect(moved.y + moved.height / 2).toBeCloseTo(y - 12, 1);
  await page.mouse.up();
});

for (const width of [768, 1024]) test(`canvas controls stay visible beside other panels at ${width}px`, async ({ page }) => {
  await page.setViewportSize({ width, height: 900 });
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iji)');
  if (width === 1024) await page.getByTestId('ai-toggle').click();
  await cell(page, 0).getByTestId('open-canvas').click();
  const canvas = page.locator('.canvas');
  for (const name of ['Gestures and keys', 'IDs', 'Relations', 'Fit diagram']) {
    await expect(canvas.getByRole('button', { name, exact: true })).toBeInViewport({ ratio: 1 });
  }
  await expect(canvas.locator('.canvas-badge')).toBeInViewport({ ratio: 1 });
});

test('phone canvas help uses the available width and never clips instructions', async ({ page }) => {
  await phone(page);
  await typeCell(page, 'def a := x_1 e(iji)');
  await cell(page, 0).getByTestId('open-canvas').click();
  await page.getByRole('button', { name: 'Gestures and keys', exact: true }).click();
  const help = page.getByRole('dialog', { name: 'Gestures and keys', exact: true });
  await expect(help).toBeVisible();
  const widths = await help.evaluate((e) => ({ client: e.clientWidth, scroll: e.scrollWidth, width: e.getBoundingClientRect().width }));
  expect(widths.scroll).toBeLessThanOrEqual(widths.client);
  expect(widths.width).toBeGreaterThan(340);
  await page.keyboard.press('Tab');
  await expect(help.getByRole('button', { name: 'Close the gestures' })).toBeFocused();
  await page.keyboard.press('Delete');
  await expect(page.locator('.canvas .frame')).toHaveAttribute('data-word', 'x_1 e(iji)');
  await help.getByRole('button', { name: 'Close the gestures' }).click();
  await expect(help).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Gestures and keys', exact: true })).toBeFocused();
});
