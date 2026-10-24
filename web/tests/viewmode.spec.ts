import { expect, test, type Browser, type Page } from '@playwright/test';
import * as Y from 'yjs';
import { Doc } from '../src/doc/model';
import { MockAccounts, type User } from './mockapi';
import { addCell, cell, expectReadOnlyCanvas, newDoc, typeCell } from './helpers';

const message = (page: Page) => page.locator('.topbar-msg');

/** Nothing on the page offers to change the document: every editing control is absent. */
async function expectNoEditingControls(page: Page) {
  await expect(page.getByTestId('add-cell')).toHaveCount(0);
  await expect(page.locator('.insert button')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Undo', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Redo', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Rename the document/ })).toHaveCount(0);
  await expect(page.getByTestId('doc-name')).toBeVisible();
  // the context is shown to read, not as a control that does nothing
  await expect(page.getByTestId('context')).toHaveJSProperty('tagName', 'SPAN');
  // (the light only shows whether something is computing)
  await expect(page.locator('.topbar button:disabled:not(.light)')).toHaveCount(0);

  await cell(page, 0).locator('.head').click();
  const display = cell(page, 0).getByTestId('cell-display');
  await expect(display).toBeVisible();
  await expect(display).not.toHaveAttribute('role', 'button');
  await expect(display).not.toHaveAttribute('title', /edit/i);
  await display.click();
  await expect(page.locator('.cm-content')).toHaveCount(0);
  const tools = cell(page, 0).locator('.tools');
  for (const name of ['Move the cell up', 'Move the cell down', 'Pin result', 'Delete', 'Tell the AI: comment on this cell'])
    await expect(tools.getByRole('button', { name, exact: true })).toHaveCount(0);
  // reading and exporting stay
  await expect(tools.getByRole('button', { name: 'Copy TikZ' })).toBeVisible();
  await expect(tools.getByTestId('open-canvas')).toBeVisible();

  // keys that edit do nothing, and say why
  await cell(page, 0).locator('.head').focus();
  await page.keyboard.press('Alt+Enter');
  await expect(page.locator('.cell[data-cell="new"]')).toHaveCount(0);
  await expect(message(page)).not.toHaveText('');
  await page.keyboard.press('ControlOrMeta+z');
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := psi_1 x_1 e(iij)');

  // the palette offers no command that edits
  await page.keyboard.press('ControlOrMeta+k');
  const palette = page.getByRole('dialog', { name: 'Command palette' });
  await expect(palette.getByRole('option', { name: /Export workspace/ })).toHaveCount(1);
  const commands = await palette.locator('[role=option] .grow').allTextContents();
  for (const name of ['New def cell', 'Insert a cell below the active cell', 'Rename document', 'Undo', 'Redo'])
    expect(commands).not.toContain(name);
  await page.keyboard.press('Escape');

  // the AI may read and answer, not change anything
  await page.getByTestId('ai-toggle').click();
  await expect(page.getByTestId('ai-read-only')).toBeVisible();
  await page.getByTestId('ai-toggle').click();
}

test('an editor switches view mode on and off; it hides every editing control and is kept per viewer', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 x_1 e(iij)');
  await addCell(page, 'watch a');
  await expect(cell(page, 1).getByTestId('summary')).not.toHaveText(/computing|^$/);
  await page.keyboard.press('Escape');
  await page.getByRole('tab', { name: 'History' }).click();
  const versions = await page.locator('.outline .oitem').count();
  await page.getByRole('tab', { name: 'Cells' }).click();

  const toggle = page.getByTestId('view-mode');
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByTestId('view-mode-banner')).toBeVisible();
  await expect(page.getByTestId('branch-start')).toHaveCount(0);
  await expectNoEditingControls(page);
  await page.keyboard.press('ControlOrMeta+b');
  await expect(page.getByRole('dialog', { name: 'Start a scratch branch' })).toHaveCount(0);
  await expect(message(page)).toContainText('View mode is on');

  // the canvas opens read-only; export stays
  await cell(page, 0).getByTestId('open-canvas').click();
  await expectReadOnlyCanvas(page);
  await page.getByTestId('canvas-back').click();
  const download = page.waitForEvent('download');
  await page.keyboard.press('ControlOrMeta+Shift+S');
  expect((await download).suggestedFilename()).toMatch(/\.dcw$/);

  // the shortcut is listed with the others
  await page.locator('.main').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('?');
  await expect(page.getByRole('dialog', { name: 'Keyboard shortcuts' })).toContainText('Switch view mode on or off');
  await page.keyboard.press('Escape');

  // switching it wrote nothing into the document, and it is still on after a reload
  await page.getByRole('tab', { name: 'History' }).click();
  await expect(page.locator('.outline .oitem')).toHaveCount(versions);
  await page.getByRole('tab', { name: 'Cells' }).click();
  await page.reload();
  await expect(page.getByTestId('view-mode')).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByTestId('add-cell')).toHaveCount(0);

  // the keyboard shortcut switches it off, and editing is back
  await page.keyboard.press('ControlOrMeta+Shift+E');
  await expect(page.getByTestId('view-mode')).toHaveAttribute('aria-pressed', 'false');
  await expect(page.getByTestId('add-cell')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Undo', exact: true })).toBeVisible();

  // an unfinished edit is not thrown away by switching view mode on
  await cell(page, 0).locator('.head').click();
  await page.locator('.cm-content').press('End');
  await page.keyboard.type(' + x_1 e(iij)');
  await page.keyboard.press('ControlOrMeta+Shift+E');
  await expect(page.getByTestId('view-mode')).toHaveAttribute('aria-pressed', 'false');
  await expect(message(page)).toContainText('Finish or discard the edit first');
  await expect(page.locator('.cm-content')).toContainText('+ x_1 e(iij)');
});

const ID = 'view000000000001';
function fixture() {
  const mock = new MockAccounts(true);
  const ada = mock.user('ada@example.org', 'Ada');
  const bob = mock.user('bob@example.org', 'Bob');
  const viewer = mock.user('viewer@example.org', 'Visitor');
  const doc = new Doc();
  doc.ydoc.clientID = 100;
  doc.setContext('klr type=A2 nu=2i+j ring=QQ');
  doc.addCell('def a := psi_1 x_1 e(iij)');
  doc.addCell('watch a');
  mock.docs.set(ID, { id: ID, owner: ada.id, name: 'Shared algebra', created: 1, edited: 1, version: 1, generation: 1, data: Buffer.from(Y.encodeStateAsUpdate(doc.ydoc)), link: null, files: new Map() });
  doc.ydoc.destroy();
  doc.privateDoc.destroy();
  mock.shares.set(ID, new Map([[bob.email, 'editor'], [viewer.email, 'viewer']]));
  return { mock, ada, bob, viewer };
}
async function device(browser: Browser, mock: MockAccounts, user: User) {
  const context = await browser.newContext();
  await mock.attach(context, { user, next: null });
  const page = await context.newPage();
  await page.goto(`/#/s/${ID}`);
  await expect(cell(page, 1)).toBeVisible();
  return { context, page };
}
const roomText = (mock: MockAccounts) => new Doc(mock.room(ID).doc).list().map((c) => c.text);

test('a view-only recipient is always in view mode, with no editing control, until a personal branch', async ({ browser }) => {
  const { mock, viewer } = fixture();
  const v = await device(browser, mock, viewer);
  try {
    const page = v.page;
    await expect(page.getByTestId('sync')).toContainText(/view only/i);
    await expect(page.getByTestId('read-only')).toBeVisible();
    // view mode cannot be switched off: no toggle, and its key says why
    await expect(page.getByTestId('view-mode')).toHaveCount(0);
    await expectNoEditingControls(page);
    await page.keyboard.press('ControlOrMeta+Shift+E');
    await expect(message(page)).toContainText('view only for you');
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.getByRole('dialog', { name: 'Command palette' }).getByRole('option', { name: /Export workspace/ })).toHaveCount(1);
    await expect(page.getByRole('dialog', { name: 'Command palette' }).getByRole('option', { name: /view mode/i })).toHaveCount(0);
    await page.keyboard.press('Escape');
    await cell(page, 0).getByTestId('open-canvas').click();
    await expectReadOnlyCanvas(page);
    await page.getByTestId('canvas-back').click();

    // a personal branch is theirs to edit (and to read in view mode if they like)
    await page.getByTestId('branch-start').click();
    await page.getByRole('dialog', { name: 'Start a scratch branch' }).getByRole('button', { name: 'Start' }).click();
    await expect(page.getByTestId('branch')).toBeVisible();
    await expect(page.getByTestId('add-cell')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Accept…' })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Accept…' })).toHaveAttribute('title', /Only people who can edit/);
    await page.getByTestId('view-mode').click();
    await expect(page.getByTestId('add-cell')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Accept…' })).toHaveCount(0);
    // the branch is kept privately with their account; the shared document was never written
    expect(mock.log.some((l) => l.who === viewer.email && l.method === 'PUT' && !l.path.includes('/branches'))).toBe(false);
    expect(roomText(mock)).toEqual(['def a := psi_1 x_1 e(iij)', 'watch a']);
  } finally { await v.context.close(); mock.dispose(); }
});

test('an editor\'s view mode is theirs alone: co-editors keep editing and the shared document is unchanged', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada), b = await device(browser, mock, bob);
  try {
    await expect.poll(() => mock.rooms.get(ID)?.peers.size ?? 0, { timeout: 15_000 }).toBe(2);
    const before = roomText(mock);
    await b.page.getByTestId('view-mode').click();
    await expect(b.page.getByTestId('view-mode')).toHaveAttribute('aria-pressed', 'true');
    await expectNoEditingControls(b.page);
    // Ada is not in view mode and can still edit; Bob reads her change live
    await expect(a.page.getByTestId('view-mode')).toHaveAttribute('aria-pressed', 'false');
    await expect(a.page.getByTestId('add-cell')).toBeVisible();
    expect(roomText(mock)).toEqual(before);
    await cell(a.page, 1).locator('.head').click();
    await a.page.locator('.cm-content').fill('watch a a');
    await a.page.locator('.cm-content').press('Enter');
    await expect(cell(b.page, 1).locator('.head .text')).toHaveAttribute('data-text', 'a a');
    await expect(b.page.locator('.cm-content')).toHaveCount(0);
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});
