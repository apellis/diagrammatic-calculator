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

test('a click on empty canvas clears the selection; Delete removes every selected piece', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 x_3 psi_2 e(iji)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  const dots = frames(page).first().locator('.hit-piece[data-draw="dot"]');
  const selected = frames(page).first().locator('.hit-piece[stroke="var(--accent)"]');
  await dots.first().click();
  await expect(selected).toHaveCount(1);
  // the empty board around the frames
  const view = (await canvas(page).locator('.canvas-view').boundingBox())!;
  await page.mouse.click(view.x + view.width - 20, view.y + view.height - 20);
  await expect(selected).toHaveCount(0);
  // two pieces selected, then Delete removes both in one step
  await dots.first().click();
  await dots.nth(1).click({ modifiers: ['ControlOrMeta'] });
  await expect(selected).toHaveCount(2);
  await page.keyboard.press('Delete');
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Remove 2 pieces');
  await expect(frames(page).first().locator('.hit-piece')).toHaveCount(1);
  await expect(frames(page).first().locator('.hit-piece[data-draw="crossing"]')).toHaveCount(1);
});

test('a crossing with no move to make still follows a sideways drag a little and says why', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_2 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  const [c] = await crossings(page);
  const [x, y] = [c.x + c.width / 2, c.y + c.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 40, y, { steps: 6 });
  await expect(canvas(page).getByTestId('pull').locator('.pulled path')).toHaveCount(2);
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('No braid or square move here');
  await page.mouse.up();
  await expect(canvas(page).getByTestId('pull')).toHaveCount(0);
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('No braid or square move here');
  // up or down, with nothing on other strands to pass, it gives a little and says so
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y - 40, { steps: 6 });
  await expect(canvas(page).getByTestId('pull')).toHaveCount(1);
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('cannot change height');
  await page.mouse.up();
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := psi_2 e(iij)');
});

test('the middle crossing of the idempotent of NH3 makes the braid move', async ({ page }) => {
  await newDoc(page);
  await page.getByTestId('context').click();
  await page.getByLabel('Context').fill('nilhecke nu=3');
  await page.getByLabel('Context').press('Enter');
  await page.locator('.cm-content').click();
  await typeCell(page, 'def e := x_1^2 x_2 psi_1 psi_2 psi_1');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  const [, middle] = await crossings(page);
  const [x, y] = [middle.x + middle.width / 2, middle.y + middle.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 8, y, { steps: 3 });
  const msg = canvas(page).getByTestId('canvas-message');
  await expect(msg).toContainText(/Pull (left|right) to apply braid/);
  const way = (await msg.textContent())!.includes('left') ? -1 : 1;
  await page.mouse.move(x + 40 * way, y, { steps: 6 });
  await expect(msg).toContainText('Let go to apply braid');
  await page.mouse.up();
  await expect(msg).toContainText('Applied braid');
});

test('a click on a strand sets the insertion point, and a palette click inserts there', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  // the third strand, about halfway up
  const strands = frames(page).first().locator('.hit-strand');
  await expect(strands).toHaveCount(3);
  const boxes = [];
  for (const el of await strands.all()) boxes.push((await el.boundingBox())!);
  boxes.sort((a, b) => a.x - b.x);
  const s3 = boxes[2];
  await page.mouse.click(s3.x + s3.width / 2, s3.y + s3.height / 2);
  await expect(canvas(page).getByTestId('mark')).toBeVisible();
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Insertion point set');
  await canvas(page).locator('.palette-bar .gen', { hasText: 'x' }).first().click();
  await expect(canvas(page).getByTestId('canvas-message')).toContainText('Insert x');
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := x_3 e(iij)');
  await expect(canvas(page).getByTestId('mark')).toHaveCount(0);
});

test('from the canvas, the back button and the breadcrumb return to the document, not the list', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  await expect(page.getByTestId('crumbs')).toContainText('def a');
  await page.getByTestId('back-to-doc').click();
  await expect(canvas(page)).toHaveCount(0);
  await expect(page).toHaveURL(/#\/doc\//);
  await expect(cell(page, 0)).toBeVisible();
  await openCanvas(page, false);
  await page.getByTestId('canvas-back').click();
  await expect(canvas(page)).toHaveCount(0);
  await openCanvas(page, false);
  await page.getByTestId('crumbs').getByRole('button').click();
  await expect(canvas(page)).toHaveCount(0);
  await expect(page).toHaveURL(/#\/doc\//);
});

test('summands fill rows as wide as the canvas, so new ones go below', async ({ page }) => {
  await newDoc(page);
  const sum = Array.from({ length: 14 }, (_, k) => `x_1^${k} psi_2 e(iij)`).join(' + ');
  await typeCell(page, `def s := ${sum}`);
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('14 terms');
  await openCanvas(page, false);
  await expect(frames(page)).toHaveCount(14);
  const view = (await canvas(page).locator('.canvas-view').boundingBox())!;
  const first = (await frames(page).first().boundingBox())!;
  const last = (await frames(page).last().boundingBox())!;
  expect(last.y).toBeGreaterThan(first.y + first.height / 2);
  for (const f of await frames(page).all()) {
    const b = (await f.boundingBox())!;
    expect(b.x + b.width).toBeLessThanOrEqual(view.x + view.width);
  }
});

test('keyboard paths for strands, the insertion point, panning, zooming and leaving for the palette', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  const msg = canvas(page).getByTestId('canvas-message');
  // S selects a whole strand, and moves on to the next
  await page.keyboard.press('s');
  await expect(msg).toContainText('Strand 1 of 3');
  await page.keyboard.press('s');
  await expect(msg).toContainText('Strand 2 of 3');
  await page.keyboard.press('Escape');
  // I, then the arrows place the insertion point; a digit inserts there
  await page.keyboard.press('i');
  await page.keyboard.press('ArrowRight');
  await expect(canvas(page).getByTestId('mark')).toBeVisible();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  await expect(msg).toContainText('strand 3 of 3');
  const x = await canvas(page).locator('.palette-bar .gen', { hasText: 'x' }).first().evaluate((b) => [...b.parentElement!.children].indexOf(b) + 1);
  await page.keyboard.press(String(x));
  await expect(msg).toContainText('Insert x');
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', /x_3/);
  // + zooms and Shift+arrows pan
  const board = canvas(page).locator('.canvas-board');
  const before = await board.getAttribute('style');
  await page.keyboard.press('+');
  await expect(board).toHaveAttribute('style', /scale\(1\.25\)/);
  await page.keyboard.press('Shift+ArrowLeft');
  expect(await board.getAttribute('style')).not.toBe(before);
  // Shift+Tab before the first piece leaves for the palette; Tab from its last generator comes back
  await page.keyboard.press('Shift+Tab');
  await expect(canvas(page).locator('.palette-bar .gen').last()).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(canvas(page)).toBeFocused();
  // the sheet lists each action with its pointer gesture and its keys
  await page.keyboard.press('?');
  await expect(canvas(page).getByRole('dialog', { name: 'Gestures and keys' }).locator('th')).toHaveText(['Action', 'Pointer', 'Keys']);
});

test('occurrence IDs show on hover and with the IDs toggle; Tab and X select several pieces from the keyboard', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 x_3 psi_2 e(iji)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await openCanvas(page, false);
  const frame = frames(page).first();
  const msg = canvas(page).getByTestId('canvas-message');
  // each piece's tooltip starts with its ID: two dots and a crossing, numbered from the bottom
  const titles = await frame.locator('.hit-piece title').allTextContents();
  expect(titles.map((t) => t.split(':')[0]).sort()).toEqual(['c1', 'd1', 'd2']);
  expect(titles.every((t) => !t.includes('layer'))).toBe(true);
  // L (or the IDs button) writes them beside the pieces and over the strands
  await expect(frame.getByTestId('occ-labels')).toHaveCount(0);
  await page.keyboard.press('l');
  await expect(canvas(page).getByTestId('labels-toggle')).toHaveAttribute('aria-pressed', 'true');
  await expect(frame.getByTestId('occ-labels')).toHaveCount(1);
  expect((await frame.locator('[data-occ]').allTextContents()).sort()).toEqual(['c1', 'd1', 'd2', 's1', 's2', 's3']);
  await canvas(page).getByTestId('labels-toggle').click();
  await expect(frame.getByTestId('occ-labels')).toHaveCount(0);
  await canvas(page).focus();

  // Tab selects one piece; X keeps it, then Tab moves the dashed ring and X adds the piece there
  const selected = frame.locator('.hit-piece[stroke="var(--accent)"]');
  await page.keyboard.press('Tab');
  await expect(selected).toHaveCount(1);
  await page.keyboard.press('x');
  await expect(msg).toContainText('1 piece selected');
  await page.keyboard.press('Tab');
  await expect(selected).toHaveCount(1);
  await expect(frame.getByTestId('ring')).toHaveCount(1);
  await page.keyboard.press('x');
  await expect(selected).toHaveCount(2);
  await page.keyboard.press('Tab');
  await page.keyboard.press('x');
  await expect(selected).toHaveCount(3);
  // X again takes the piece at the ring out
  await page.keyboard.press('x');
  await expect(selected).toHaveCount(2);
  await expect(msg).toContainText('2 pieces selected');
  // the selection is what R and Delete act on; Esc clears it and the ring
  await page.keyboard.press('Escape');
  await expect(selected).toHaveCount(0);
  await expect(frame.getByTestId('ring')).toHaveCount(0);
  // the sheet lists the keys
  await page.keyboard.press('?');
  await expect(canvas(page).getByRole('dialog', { name: 'Gestures and keys' })).toContainText('X adds');
});
