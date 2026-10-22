import { expect, test, type Page } from '@playwright/test';
import { addCell, cell, newDoc, typeCell } from './helpers';

const canvas = (page: Page) => page.locator('.canvas');
const frames = (page: Page) => canvas(page).locator('.frame');

/** Opens the canvas on the first cell, which is active: with F after clicking its diagrams, or with its button. */
async function openCanvas(page: Page, key = true) {
  if (key) {
    await cell(page, 0).locator('.sum').click();
    await page.keyboard.press('f');
  } else await cell(page, 0).getByTestId('open-canvas').click();
  await expect(canvas(page)).toBeVisible();
  await expect(frames(page).first().locator('svg.diagram')).toBeVisible();
}

test('dragging a dot through a crossing slides it, and undo restores the term', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 x_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  await expect(frames(page)).toHaveCount(1);

  const dot = frames(page).first().locator('[data-draw="dot"]');
  const box = (await dot.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2, box.y - 60, { steps: 6 });
  await page.mouse.up();
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Slid the dot up through 1 crossing');
  // nilHecke-type dot slide in a KLR algebra with equal colours: x_1 psi_1 = psi_1 x_2 + correction
  await expect(frames(page)).toHaveCount(2);
  await expect(canvas(page).locator('.frame.new')).toHaveCount(2);
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  await expect(canvas(page)).toBeHidden();
  await expect(cell(page, 0).locator('.head .text')).not.toHaveText('a := psi_1 x_1 e(iij)');

  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(cell(page, 0).locator('.head .text')).toHaveText('a := psi_1 x_1 e(iij)');
});


test('keyboard path: R lists relations and 1 applies one; undo; Tab to the dot and the arrow slides it', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def b := psi_1 x_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page);
  await page.keyboard.press('r');
  await expect(canvas(page).locator('.relmenu .relitem').first()).toBeVisible();
  await page.keyboard.press('1');
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Applied');
  await expect(frames(page)).toHaveCount(2);
  // undo works from inside the canvas
  await page.keyboard.press('ControlOrMeta+z');
  await expect(frames(page)).toHaveCount(1);
  // Tab moves through the pieces in drawn order: the dot is drawn first
  await page.keyboard.press('Tab');
  await expect(frames(page).first().locator('.hit-piece[data-draw="dot"]')).toHaveAttribute('stroke', 'var(--accent)');
  await page.keyboard.press('ArrowUp');
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Slid the dot up');
  await expect(frames(page)).toHaveCount(2);
});

test('inserting from the palette and removing with Delete change the element', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def c := psi_2 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page);
  await page.keyboard.press('Tab');
  await page.keyboard.press('i');
  // the palette numbers its generators while inserting
  const first = canvas(page).locator('.palette-bar .gen').first();
  await expect(first).toContainText('1 ');
  await page.keyboard.press('1');
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Insert');
  await expect(frames(page).first().locator('.hit-piece')).toHaveCount(2);
  await page.keyboard.press('Tab');
  await page.keyboard.press('Delete');
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Remove');
  await expect(frames(page).first().locator('.hit-piece')).toHaveCount(1);
});

test('interchanging odd dots shows the sign, box selection narrows relations, and a watch of definitions is written out', async ({ page }) => {
  await newDoc(page);
  await page.getByTestId('context').click();
  await page.getByLabel('Context').fill('odd-nilhecke nu=2');
  await page.getByLabel('Context').press('Enter');
  await page.locator('.cm-content').click();
  await typeCell(page, 'def d := x_1 x_2');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page);
  // the canonical drawing puts both dots at one height; moving the first up costs a Koszul sign
  await page.keyboard.press('Tab');
  await page.keyboard.press('Alt+ArrowUp');
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('changes sign');
  // the drawing is a layout hint carrying a −1 badge; the term is unchanged, and undo puts the dots back
  await expect(frames(page).first().locator('.badge')).toHaveText('−1 from interchange');
  await page.keyboard.press('ControlOrMeta+z');
  await expect(frames(page).first().locator('.badge')).toHaveCount(0);

  // a look-only move with Alt-drag stores an offset and leaves the term alone
  const dot = frames(page).first().locator('[data-draw="dot"]').first();
  const b = (await dot.boundingBox())!;
  await page.keyboard.down('Alt');
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2 - 10, { steps: 4 });
  await page.mouse.up();
  await page.keyboard.up('Alt');
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  await expect(cell(page, 0).locator('.head .text')).toHaveText('d := x_1 x_2');

  // a watch that uses a definition opens written out
  await addCell(page, 'watch d psi_1');
  await cell(page, 1).getByTestId('open-canvas').click();
  await expect(canvas(page).locator('.banner')).toContainText('uses definitions');
  await expect(frames(page)).toHaveCount(1);
  await page.keyboard.press('Control+a');
  await page.keyboard.press('r');
  await expect(canvas(page).locator('.relmenu')).toContainText('in the selection');
});
