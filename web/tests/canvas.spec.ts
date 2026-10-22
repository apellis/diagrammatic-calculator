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
  await expect(cell(page, 0).locator('.head .text')).not.toHaveAttribute('data-text', 'a := psi_1 x_1 e(iij)');

  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := psi_1 x_1 e(iij)');
});


test('a dragged dot follows its strand, a crossing glows as it nears and rings once passed', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 x_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  const dot = frames(page).first().locator('[data-draw="dot"]');
  const box = (await dot.boundingBox())!;
  const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
  const live = canvas(page).getByTestId('live');
  await page.mouse.move(x, y);
  await page.mouse.down();
  // a little way up: the dot moves with the pointer and the crossing ahead starts to glow
  await page.mouse.move(x, y - 18, { steps: 3 });
  await expect(live.locator('.live-dot')).toHaveCount(1);
  const liveBox = (await live.locator('.live-dot').boundingBox())!;
  expect(liveBox.y + liveBox.height / 2).toBeLessThan(y - 10);
  await expect(live.locator('.xing.near')).toHaveCount(1);
  await expect(live.locator('.xing.passed')).toHaveCount(0);
  // past the crossing's middle: it rings, and the dot has moved across with the strand
  await page.mouse.move(x, y - 70, { steps: 4 });
  await expect(live.locator('.xing.passed')).toHaveCount(1);
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Let go to slide the dot up through 1 crossing');
  const across = (await live.locator('.live-dot').boundingBox())!;
  expect(across.x + across.width / 2).toBeGreaterThan(x + 10);
  await page.mouse.up();
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Slid the dot up through 1 crossing');
  await expect(frames(page)).toHaveCount(2);
  await expect(live).toHaveCount(0);
});

test('a dot on a bare strand slides with the pointer and settles back when nothing changes', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_3 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  const dot = frames(page).first().locator('[data-draw="dot"]');
  const box = (await dot.boundingBox())!;
  const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
  const live = canvas(page).getByTestId('live');
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y - 12, { steps: 3 });
  const moved = (await live.locator('.live-dot').boundingBox())!;
  expect(moved.y + moved.height / 2).toBeLessThan(y - 6);
  await expect(live.locator('.xing')).toHaveCount(0);
  await page.mouse.up();
  await expect(live).toHaveCount(0);
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := x_3 e(iij)');
});

/** The crossings of the first frame, bottom to top, as page boxes. */
async function crossings(page: Page) {
  const boxes = [];
  for (const el of await frames(page).first().locator('.hit-piece[data-draw="crossing"]').all()) {
    const b = await el.boundingBox();
    if (b) boxes.push(b);
  }
  return boxes.sort((a, b) => b.y - a.y);
}

test('pulling the middle crossing of a triangle across applies the braid relation, with its result shown first', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 psi_2 psi_1 e(iji)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  const [, middle] = await crossings(page);
  const [x, y] = [middle.x + middle.width / 2, middle.y + middle.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  // the wrong way (right) does not arm it
  await page.mouse.move(x + 30, y, { steps: 4 });
  await expect(canvas(page).getByTestId('pull').locator('.xing')).toHaveCount(3);
  await expect(canvas(page).getByTestId('pull').locator('.xing.passed')).toHaveCount(0);
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Pull left to apply braid');
  // towards the other two: armed, and the result is previewed
  await page.mouse.move(x - 30, y, { steps: 6 });
  await expect(canvas(page).getByTestId('pull').locator('.xing.passed')).toHaveCount(3);
  await expect(canvas(page).getByTestId('pull-result')).toBeVisible();
  await page.mouse.up();
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Applied braid-corrected');
  await expect(frames(page)).toHaveCount(2);
});

test('pulling a stacked pair of crossings apart applies the square relation', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 psi_1 e(iji)');
  await expect(cell(page, 0).getByTestId('summary')).not.toHaveText('');
  await openCanvas(page, false);
  const [bottom] = await crossings(page);
  const [x, y] = [bottom.x + bottom.width / 2, bottom.y + bottom.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 35, y, { steps: 6 });
  await expect(canvas(page).getByTestId('pull').locator('.xing.passed')).toHaveCount(2);
  await page.mouse.up();
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Applied square');
  await expect(frames(page).first().locator('.hit-piece[data-draw="crossing"]')).toHaveCount(0);
});

test('a crossing dragged up past a piece on other strands is redrawn there as it moves, and stays on release', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_3 psi_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  const dot = frames(page).first().locator('.hit-piece[data-draw="dot"]');
  const [c] = await crossings(page);
  const before = (await dot.boundingBox())!;
  const [x, y] = [c.x + c.width / 2, c.y + c.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y - 90, { steps: 8 });
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Let go to interchange');
  // the dot is redrawn below the crossing while dragging
  await expect.poll(async () => (await dot.boundingBox())?.y ?? 0).toBeGreaterThan(before.y + 10);
  // the crossing stays centred between its strands
  await expect.poll(async () => Math.abs(((await crossings(page))[0]?.x ?? 1e9) - c.x)).toBeLessThan(2);
  await page.mouse.up();
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Interchanged');
  await expect.poll(async () => (await dot.boundingBox())?.y ?? 0).toBeGreaterThan(before.y + 10);
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
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'd := x_1 x_2');

  // a watch that uses a definition opens written out
  await addCell(page, 'watch d psi_1');
  await cell(page, 1).getByTestId('open-canvas').click();
  await expect(canvas(page).locator('.banner')).toContainText('uses definitions');
  await expect(frames(page)).toHaveCount(1);
  await page.keyboard.press('Control+a');
  await page.keyboard.press('r');
  await expect(canvas(page).locator('.relmenu')).toContainText('in the selection');
});
