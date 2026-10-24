import { test, expect, type Browser, type Page } from '@playwright/test';
import * as Y from 'yjs';
import * as encoding from 'lib0/encoding';
import * as decoding from 'lib0/decoding';
import * as sync from 'y-protocols/sync';
import { Doc } from '../src/doc/model';
import { stageUpdate } from '../live/validation';
import { MockAccounts, type User } from './mockapi';
import { cell, expectReadOnlyCanvas } from './helpers';

const ID = 'live000000000001';
function fixture() {
  const mock = new MockAccounts(true);
  const ada = mock.user('ada@example.org', 'Ada');
  const bob = mock.user('bob@example.org', 'Bob');
  const cy = mock.user('cy@example.org', 'Cy');
  const viewer = mock.user('viewer@example.org', 'Visitor');
  const doc = new Doc();
  doc.ydoc.clientID = 100;
  doc.setContext('klr type=A2 nu=2i+j ring=QQ');
  doc.addCell('watch x_1 e(iji)');
  doc.addCell('watch x_2 e(iji)');
  mock.docs.set(ID, { id: ID, owner: ada.id, name: 'Live algebra', created: 1, edited: 1, version: 1, generation: 1, data: Buffer.from(Y.encodeStateAsUpdate(doc.ydoc)), link: null, files: new Map() });
  doc.ydoc.destroy();
  doc.privateDoc.destroy();
  mock.shares.set(ID, new Map([[bob.email, 'editor'], [cy.email, 'editor'], [viewer.email, 'viewer']]));
  return { mock, ada, bob, cy, viewer };
}
async function device(browser: Browser, mock: MockAccounts, user: User) {
  const context = await browser.newContext();
  await mock.attach(context, { user, next: null });
  const page = await context.newPage();
  await page.goto(`/#/s/${ID}`);
  await expect(cell(page, 1)).toBeVisible();
  return { context, page };
}
async function connected(mock: MockAccounts, count: number) {
  await expect.poll(() => mock.rooms.get(ID)?.peers.size ?? 0, { timeout: 15_000 }).toBe(count);
}
async function edit(page: Page, index: number, text: string, commit = true) {
  await cell(page, index).locator('.head').click();
  const editor = cell(page, index).locator('.cm-content');
  await editor.fill(text);
  if (commit) await editor.press('Enter');
  return editor;
}
const text = (page: Page, index: number, value: string) => expect(cell(page, index).locator('.head .text')).toHaveAttribute('data-text', value);

test('private previews update dependents locally, and committed edits update a viewer', async ({ browser }) => {
  const { mock, ada, viewer } = fixture();
  const a = await device(browser, mock, ada), b = await device(browser, mock, viewer);
  try {
    await connected(mock, 2);
    await edit(a.page, 0, 'def e := psi_1 psi_2 psi_1 e(iji)');
    await edit(a.page, 1, 'check idempotent(e)');
    await expect(cell(a.page, 1).getByTestId('summary')).toHaveText('holds');
    await expect(cell(b.page, 1).getByTestId('summary')).toHaveText('holds');
    const before = Buffer.from(Y.encodeStateAsUpdate(mock.room(ID).doc));
    await cell(a.page, 0).getByTestId('cell-display').click();
    const editor = cell(a.page, 0).locator('.cm-content');
    await editor.fill('def e := x_1 e(iji)');
    await expect(cell(a.page, 1).getByTestId('summary')).toHaveText('fails');
    await expect(cell(b.page, 1).getByTestId('summary')).toHaveText('holds');
    expect(Buffer.from(Y.encodeStateAsUpdate(mock.room(ID).doc))).toEqual(before);
    await cell(a.page, 0).getByTestId('open-canvas').click();
    await expect(a.page.locator('.canvas .hit-piece[data-draw="dot"]')).toBeVisible();
    // Presence and AI selection are outbound channels too, not just the Yjs document.
    await expect.poll(() => [...mock.room(ID).presence.getStates().values()].filter(s => s.user?.id === ada.id)).toEqual([
      expect.objectContaining({ selection: null }),
    ]);
    await a.page.getByTestId('back-to-doc').click();
    expect(Buffer.from(Y.encodeStateAsUpdate(mock.room(ID).doc))).toEqual(before);
    await expect(cell(b.page, 1).locator('.head > .kind + .outcome')).toHaveText('holds');
    await editor.press('Enter');
    await expect(cell(b.page, 1).getByTestId('summary')).toHaveText('fails');
    // the viewer's collapsed check: computed (the dot), and fails (the pill)
    await expect(cell(b.page, 1).locator('.head > .kind + .outcome')).toHaveText('fails');
    await expect(cell(b.page, 1).getByTestId('cell-dot')).toHaveAttribute('title', 'Computed');
    await a.page.getByRole('button', { name: 'Undo', exact: true }).click();
    await expect(cell(b.page, 1).getByTestId('summary')).toHaveText('holds');
    await expect(cell(b.page, 1).locator('.head > .kind + .outcome')).toHaveText('holds');
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});

test('public-ID repair keeps the private draft and editor on the original UUID', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada), b = await device(browser, mock, bob);
  const peer = new Doc(); peer.ydoc.clientID = 1;
  peer.addCell('watch x_3 e(iji)');
  try {
    await connected(mock, 2);
    const room = mock.room(ID);
    const original = room.doc.getArray<Y.Map<unknown>>('cells').toArray().find(c => c.get('id') === 'wt1')!;
    const key = original.get('key');
    const editor = await edit(a.page, 0, 'watch x_1^9 e(iji)', false);
    const node = await editor.elementHandle();
    const staged = stageUpdate(room.doc, Y.encodeStateAsUpdate(peer.ydoc));
    Y.applyUpdate(room.doc, staged.update); staged.doc.destroy();
    expect(original.get('key')).toBe(key);
    expect(original.get('id')).not.toBe('wt1');
    const repaired = a.page.locator(`[data-cell="${original.get('id')}"]`);
    await expect(repaired.locator('.cm-content')).toHaveText('watch x_1^9 e(iji)');
    expect(await repaired.locator('.cm-content').evaluate((el, previous) => el === previous, node)).toBe(true);
    await expect(repaired).toHaveClass(/active/);
    await expect(a.page.getByTestId('draft-conflict')).toHaveCount(0);
    await repaired.locator('.cm-content').press('Enter');
    await expect.poll(() => (original.get('text') as Y.Text).toString()).toBe('watch x_1^9 e(iji)');
    const other = room.doc.getArray<Y.Map<unknown>>('cells').toArray().find(c => c.get('key') === peer.cells.get(0).get('key'))!;
    expect(other.get('id')).toBe('wt1');
    expect((other.get('text') as Y.Text).toString()).toBe('watch x_3 e(iji)');
    await expect(b.page.locator(`[data-cell="${original.get('id')}"] .head .text`)).toHaveAttribute('data-text', 'x_1^9 e(iji)');
  } finally { peer.ydoc.destroy(); peer.privateDoc.destroy(); await a.context.close(); await b.context.close(); mock.dispose(); }
});

for (const scenario of ['deleted draft', 'canvas target'] as const) {
  test(`public-ID repair preserves ${scenario} ownership`, async ({ browser }) => {
    const { mock, ada } = fixture();
    const a = await device(browser, mock, ada);
    const peer = new Doc(); peer.ydoc.clientID = 1;
    // Matching text must not make refresh reuse another key's CellData.
    peer.addCell('watch x_1 e(iji)');
    try {
      await connected(mock, 1);
      const room = mock.room(ID), doc = new Doc(room.doc);
      const original = doc.cells.toArray().find(c => c.get('id') === 'wt1')!;
      await cell(a.page, 0).locator('.head').click();
      if (scenario === 'deleted draft') {
        await cell(a.page, 0).locator('.cm-content').fill('watch x_1^9 e(iji)');
        await cell(a.page, 0).locator('.cm-content').press('ArrowLeft');
      }
      else {
        await cell(a.page, 0).getByTestId('open-canvas').click();
        await expect(a.page.locator('.hit-piece').first()).toBeVisible();
      }
      const staged = stageUpdate(room.doc, Y.encodeStateAsUpdate(peer.ydoc));
      Y.applyUpdate(room.doc, staged.update); staged.doc.destroy();
      const id = original.get('id') as string;
      expect(id).not.toBe('wt1');
      if (scenario === 'deleted draft') {
        await expect(a.page.locator(`[data-cell="${id}"] .cm-content`)).toHaveText('watch x_1^9 e(iji)');
        doc.removeCell(id);
        await expect(a.page.getByTestId('deleted-draft')).toContainText('watch x_1^9 e(iji)');
        await a.page.getByRole('button', { name: 'Restore draft as new cell' }).click();
        await expect.poll(() => doc.list().some(c => c.text === 'watch x_1^9 e(iji)')).toBe(true);
        expect(doc.list().find(c => c.id === 'wt1')?.text).toBe('watch x_1 e(iji)');
      } else {
        await expect(a.page.locator('.canvas')).toContainText(`watch (${id})`);
        doc.removeCell(id);
        await expect(a.page.getByTestId('back-to-doc')).toHaveCount(0);
        expect(doc.list().find(c => c.id === 'wt1')?.text).toBe('watch x_1 e(iji)');
      }
      doc.privateDoc.destroy();
    } finally { peer.ydoc.destroy(); peer.privateDoc.destroy(); await a.context.close(); mock.dispose(); }
  });
}

test('two editors commit independently and a late third receives the converged document', async ({ browser }) => {
  const { mock, ada, bob, cy } = fixture();
  const a = await device(browser, mock, ada);
  const b = await device(browser, mock, bob);
  try {
    await connected(mock, 2);
    await Promise.all([edit(a.page, 0, 'watch x_1^2 e(iji)'), edit(b.page, 1, 'watch x_2^2 e(iji)')]);
    for (const page of [a.page, b.page]) {
      await text(page, 0, 'x_1^2 e(iji)');
      await text(page, 1, 'x_2^2 e(iji)');
    }
    const c = await device(browser, mock, cy);
    try {
      await connected(mock, 3);
      await text(c.page, 0, 'x_1^2 e(iji)');
      await text(c.page, 1, 'x_2^2 e(iji)');
      expect(mock.room(ID).acceptedUpdates).toBeGreaterThan(0);
    } finally { await c.context.close(); }
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});

for (const choice of ['Keep mine', 'Take theirs'] as const) {
  test(`private same-cell draft survives a remote commit with stable editor DOM: ${choice}`, async ({ browser }) => {
    const { mock, ada, bob } = fixture();
    const a = await device(browser, mock, ada);
    const b = await device(browser, mock, bob);
    try {
      await connected(mock, 2);
      const editor = await edit(b.page, 0, 'watch x_1^3 e(iji)', false);
      const node = await editor.elementHandle();
      // The draft must not change either the other browser or the room's committed state.
      await text(a.page, 0, 'x_1 e(iji)');
      expect(new Doc(mock.room(ID).doc).list()[0].text).toBe('watch x_1 e(iji)');
      await edit(a.page, 0, 'watch x_1^2 e(iji)');
      const conflict = b.page.getByTestId('draft-conflict');
      await expect(conflict).toBeVisible();
      await expect(editor).toHaveText('watch x_1^3 e(iji)');
      expect(await editor.evaluate((el, previous) => el === previous, node)).toBe(true);
      await conflict.getByRole('button', { name: choice, exact: true }).click();
      await expect(conflict).toHaveCount(0);
      if (choice === 'Take theirs') await expect(editor).toHaveText('watch x_1^2 e(iji)');
      const wanted = choice === 'Keep mine' ? 'x_1^3 e(iji)' : 'x_1^2 e(iji)';
      await text(a.page, 0, wanted);
      await text(b.page, 0, wanted);
    } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
  });
}

test('undo affects only this session, not another editor’s committed edit', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada);
  const b = await device(browser, mock, bob);
  try {
    await connected(mock, 2);
    await edit(a.page, 0, 'watch x_1^2 e(iji)');
    await text(b.page, 0, 'x_1^2 e(iji)');
    await edit(b.page, 1, 'watch x_2^2 e(iji)');
    await text(a.page, 1, 'x_2^2 e(iji)');
    await a.page.getByRole('button', { name: 'Undo', exact: true }).click();
    for (const page of [a.page, b.page]) {
      await text(page, 0, 'x_1 e(iji)');
      await text(page, 1, 'x_2^2 e(iji)');
    }
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});

test('viewer receives live commits but has no editable cell or enabled add control', async ({ browser }) => {
  const { mock, ada, viewer } = fixture();
  const a = await device(browser, mock, ada);
  const v = await device(browser, mock, viewer);
  try {
    await connected(mock, 2);
    await expect(v.page.getByTestId('sync')).toContainText(/view only/i);
    await expect(v.page.locator('[data-testid="add-cell"]:enabled')).toHaveCount(0);
    await cell(v.page, 0).locator('.head').click();
    await expect(v.page.locator('.cm-content[contenteditable="true"]')).toHaveCount(0);
    await edit(a.page, 0, 'watch x_1^2 e(iji)');
    await text(v.page, 0, 'x_1^2 e(iji)');
    expect(mock.log.some((l) => l.who === viewer.email && l.method === 'PUT')).toBe(false);
  } finally { await a.context.close(); await v.context.close(); mock.dispose(); }
});

test('a viewer definition canvas is read-only, without editing gestures or affordances', async ({ browser }) => {
  const { mock, ada, viewer } = fixture();
  const a = await device(browser, mock, ada);
  const v = await device(browser, mock, viewer);
  try {
    await connected(mock, 2);
    await edit(a.page, 0, 'def a := psi_1 x_1 e(iij)');
    await text(v.page, 0, 'a := psi_1 x_1 e(iij)');
    await cell(v.page, 0).locator('.head').click();
    await cell(v.page, 0).getByTestId('open-canvas').click();
    await expectReadOnlyCanvas(v.page);
    expect(new Doc(mock.room(ID).doc).list()[0].text).toBe('def a := psi_1 x_1 e(iij)');
  } finally { await a.context.close(); await v.context.close(); mock.dispose(); }
});

test('changing a definition to a watch cancels an in-flight canvas gesture', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada), b = await device(browser, mock, bob);
  try {
    await connected(mock, 2);
    await edit(a.page, 0, 'def a := psi_1 x_1 e(iij)');
    await text(b.page, 0, 'a := psi_1 x_1 e(iij)');
    await cell(b.page, 0).locator('.head').click();
    await cell(b.page, 0).getByTestId('open-canvas').click();
    const canvas = b.page.locator('.canvas');
    const dot = canvas.locator('.hit-piece[data-draw="dot"]').first();
    await expect(dot).toBeVisible();
    const box = (await dot.boundingBox())!;
    await b.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await b.page.mouse.down();
    await b.page.mouse.move(box.x + box.width / 2, box.y - 18, { steps: 3 });
    await expect(canvas.getByTestId('live')).toBeVisible();
    await cell(a.page, 0).getByTestId('cell-display').click();
    await cell(a.page, 0).locator('.cm-content').fill('watch psi_1 x_1 e(iij)');
    await cell(a.page, 0).locator('.cm-content').press('Enter');
    await expect(canvas.getByText('Read only', { exact: true })).toBeVisible();
    await expect(canvas.getByTestId('live')).toHaveCount(0);
    await expect(canvas.locator('.palette-bar')).toHaveCount(0);
    await b.page.mouse.up();
    await expectReadOnlyCanvas(b.page);
    expect(new Doc(mock.room(ID).doc).list()[0].text).toBe('watch psi_1 x_1 e(iij)');
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});

test('offline edits remain local and converge after reconnect without a reload', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada);
  const b = await device(browser, mock, bob);
  try {
    await connected(mock, 2);
    await b.context.route('**/live-ticket', (route) => route.abort('internetdisconnected'));
    await b.context.setOffline(true);
    // Routed WebSockets are in-process: explicitly sever this transport as a real outage would.
    mock.room(ID).disconnect(bob);
    await expect(b.page.getByTestId('sync')).toContainText(/offline|reconnect/i);
    await edit(b.page, 1, 'watch x_2^3 e(iji)');
    await edit(a.page, 0, 'watch x_1^2 e(iji)');
    await text(a.page, 1, 'x_2 e(iji)');
    await b.context.unroute('**/live-ticket');
    await b.context.setOffline(false);
    await connected(mock, 2);
    for (const page of [a.page, b.page]) {
      await text(page, 0, 'x_1^2 e(iji)');
      await text(page, 1, 'x_2^3 e(iji)');
    }
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});

test('revoking an editor disconnects them and prevents subsequent document writes', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada);
  const b = await device(browser, mock, bob);
  try {
    await connected(mock, 2);
    mock.revoke(ID, bob);
    await connected(mock, 1);
    await expect(b.page.getByTestId('sync')).toContainText(/access|revoked|permission/i);
    await expect(b.page.locator('[data-testid="add-cell"]:enabled')).toHaveCount(0);
    await cell(b.page, 0).locator('.head').click();
    await expect(b.page.locator('.cm-content[contenteditable="true"]')).toHaveCount(0);
    await edit(a.page, 0, 'watch x_1^2 e(iji)');
    await text(a.page, 0, 'x_1^2 e(iji)');
    expect(new Doc(mock.room(ID).doc).list()[1].text).toBe('watch x_2 e(iji)');
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});

test('personal branch text, canvas, and AI presence never reach a peer', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada), b = await device(browser, mock, bob);
  const received: Record<string, unknown>[] = [];
  await b.page.exposeFunction('capturePresence', (bytes: number[]) => {
    const d = decoding.createDecoder(new Uint8Array(bytes));
    if (decoding.readVarUint(d) !== 1) return;
    const states = decoding.createDecoder(decoding.readVarUint8Array(d));
    const count = decoding.readVarUint(states);
    for (let i = 0; i < count; i++) {
      decoding.readVarUint(states); decoding.readVarUint(states);
      const state = JSON.parse(decoding.readVarString(states));
      if (state?.user?.id === ada.id) received.push(state);
    }
  });
  await b.page.addInitScript(() => {
    const Native = window.WebSocket;
    window.WebSocket = class extends Native {
      constructor(url: string | URL, protocols?: string | string[]) {
        super(url, protocols);
        this.addEventListener('message', async event => {
          const data = event.data instanceof Blob ? await event.data.arrayBuffer() : event.data;
          if (data instanceof ArrayBuffer) void (window as unknown as { capturePresence: (bytes: number[]) => Promise<void> }).capturePresence([...new Uint8Array(data)]);
        });
      }
    };
  });
  let release = () => {};
  try {
    await b.page.reload(); await connected(mock, 2);
    // Set up a fake provider through the ordinary UI; hold its request while inspecting AI presence.
    await a.page.route('https://api.anthropic.com/**', async route => {
      const headers = { 'access-control-allow-origin': '*', 'access-control-allow-headers': '*' };
      if (route.request().method() === 'OPTIONS') return route.fulfill({ status: 204, headers });
      if (route.request().method() === 'GET') return route.fulfill({ json: { data: [] }, headers });
      await new Promise<void>(resolve => { release = resolve; });
      await route.abort();
    });
    await a.page.getByTestId('ai-toggle').click();
    await a.page.getByRole('button', { name: 'I understand' }).click();
    await a.page.getByTestId('ai-setup-link').click();
    await a.page.getByLabel('API key').fill('sk-ant-test');
    await a.page.getByTestId('ai-save').click();
    await expect(a.page.getByTestId('ai-saved')).toBeVisible();
    await a.page.getByTestId('settings-back').click();
    await cell(a.page, 0).locator('.head').click();
    await cell(a.page, 0).getByTestId('open-canvas').click();
    await a.page.locator('.hit-piece').first().click();
    await expect.poll(() => received.some(s => !!s.selection)).toBe(true);
    await a.page.getByTestId('back-to-doc').click();
    await a.page.getByTestId('branch-start').click();
    await a.page.getByLabel('Branch name').fill('secret-branch-name');
    await a.page.getByRole('button', { name: 'Start', exact: true }).click();
    await expect(a.page.locator('.branch-banner')).toBeVisible();
    await edit(a.page, 0, 'watch x_1^7 e(iji)');
    await cell(a.page, 0).locator('.head').click();
    await cell(a.page, 0).locator('.head').click();
    const editor = cell(a.page, 0).locator('.cm-content');
    await editor.press('Home'); await editor.press('Shift+ArrowRight');
    await editor.fill('watch x_1^8 e(iji)');
    await cell(a.page, 0).getByTestId('open-canvas').click();
    await expect(a.page.locator('.hit-piece').last()).toBeVisible();
    await a.page.locator('.hit-piece').last().click();
    await a.page.getByLabel('Message to the AI').fill('private AI prompt');
    await a.page.getByLabel('Message to the AI').press('Enter');
    await expect.poll(() => received.some(s => (s.ai as { active?: boolean })?.active)).toBe(true);
    const privateStates = received.slice(received.findIndex(s => !!s.branch));
    expect(privateStates.length).toBeGreaterThan(0);
    for (const s of privateStates) {
      expect(s.branch).toBe('personal branch');
      expect(s.selection ?? null).toBeNull(); expect(s.cursor ?? null).toBeNull(); expect(s.cell ?? null).toBeNull();
      if (s.ai) expect(s.ai).toEqual({ active: true });
    }
    expect(JSON.stringify(privateStates)).not.toMatch(/secret-branch-name|x_1\^[78]|private AI prompt/);
    expect(new Doc(mock.room(ID).doc).list()[0].text).toBe('watch x_1 e(iji)');
  } finally { release(); await a.context.close(); await b.context.close(); mock.dispose(); }
});

test('presence identifies other editors by authenticated names and clears on departure', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada);
  const b = await device(browser, mock, bob);
  try {
    await connected(mock, 2);
    await expect(a.page.getByTestId('presence').getByTestId('collaborator')).toContainText(['Bob']);
    await expect(b.page.getByTestId('presence').getByTestId('collaborator')).toContainText(['Ada']);
    await b.context.close();
    await connected(mock, 1);
    await expect(a.page.getByTestId('presence').getByTestId('collaborator').filter({ hasText: 'Bob' })).toHaveCount(0);
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});

test('committed-text cursors are relative and private drafts do not expose cursor offsets', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada), b = await device(browser, mock, bob);
  try {
    await connected(mock, 2);
    await cell(b.page, 0).locator('.head').click();
    await cell(a.page, 0).locator('.head').click();
    const editor = cell(a.page, 0).locator('.cm-content');
    await editor.press('Home'); await editor.press('Shift+ArrowRight');
    await expect(b.page.locator('.remote-caret')).toHaveAttribute('aria-label', "Ada's cursor");
    await editor.fill('watch x_1^7 e(iji)');
    await expect(b.page.locator('.remote-caret')).toHaveCount(0);
    expect(new Doc(mock.room(ID).doc).list()[0].text).toBe('watch x_1 e(iji)');
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});

test('canvas selection rectangles are shown to collaborators', async ({ browser }) => {
  const { mock, ada, bob } = fixture();
  const a = await device(browser, mock, ada), b = await device(browser, mock, bob);
  try {
    await connected(mock, 2);
    for (const page of [a.page, b.page]) {
      await cell(page, 0).locator('.head').click();
      await cell(page, 0).getByTestId('open-canvas').click();
      await expect(page.locator('.hit-piece').first()).toBeVisible();
    }
    await a.page.locator('.hit-piece').first().click();
    await expect(b.page.getByTestId('remote-canvas-selection')).toContainText('Ada');
  } finally { await a.context.close(); await b.context.close(); mock.dispose(); }
});

test('two editors cannot accept one shared AI proposal twice; the winner can undo its change', async ({ browser }) => {
  const { mock, ada, bob, cy } = fixture();
  const b = await device(browser, mock, bob), c = await device(browser, mock, cy);
  try {
    await connected(mock, 2);
    const doc = new Doc(mock.room(ID).doc);
    doc.propose({ id: 'shared-add', author: ada.id, time: Date.now(), title: 'Add the next watch', status: 'pending', payload: {
      id: 1, key: 'shared-add', kind: 'write', target: 'add a watch', note: 'Add the next watch', state: 'open',
      base: JSON.parse(JSON.stringify(doc.inputs())), operation: { name: 'cell_add', args: { text: 'watch x_1^4 e(iji)', note: 'Add the next watch' } },
    } });
    const waiting: import('@playwright/test').Route[] = [];
    for (const page of [b.page, c.page]) {
      await page.route('**/proposals/*/decision', async route => {
        waiting.push(route);
        if (waiting.length === 2) await Promise.all(waiting.map(r => r.fallback()));
      });
      await page.getByTestId('ai-toggle').click();
      await expect(page.getByTestId('proposal')).toContainText('Add the next watch');
    }
    const responses = [b.page, c.page].map(page => page.waitForResponse(r => r.url().endsWith('/shared-add/decision')));
    await Promise.all([b.page, c.page].map(page => page.getByTestId('proposal').getByRole('button', { name: 'Allow', exact: true }).click()));
    const statuses = await Promise.all(responses.map(async response => (await response).status()));
    expect([...statuses].sort()).toEqual([200, 409]);
    expect(doc.list().filter(x => x.text === 'watch x_1^4 e(iji)')).toHaveLength(1);
    const winner = statuses[0] === 200 ? b.page : c.page;
    await winner.getByRole('button', { name: 'Undo', exact: true }).click();
    await expect.poll(() => doc.list().some(x => x.text === 'watch x_1^4 e(iji)')).toBe(false);
    doc.privateDoc.destroy();
  } finally { await b.context.close(); await c.context.close(); mock.dispose(); }
});

test('room fixture: awareness identity is server-owned and viewer updates are rejected', async ({ browser }) => {
  const { mock, viewer } = fixture();
  const context = await browser.newContext();
  await mock.attach(context, { user: viewer, next: null });
  const page = await context.newPage();
  try {
    await page.route((url) => url.pathname === '/', (route) => route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>Wire fixture</title>' }));
    await page.goto('/');
    const state = encoding.createEncoder();
    encoding.writeVarUint(state, 1);
    encoding.writeVarUint(state, 77);
    encoding.writeVarUint(state, 1);
    encoding.writeVarString(state, JSON.stringify({ user: { name: 'Spoofed owner' }, cell: 'c1', ai: 'thinking' }));
    const message = encoding.createEncoder();
    encoding.writeVarUint(message, 1);
    encoding.writeVarUint8Array(message, encoding.toUint8Array(state));
    const update = encoding.createEncoder();
    encoding.writeVarUint(update, 0);
    const malicious = new Y.Doc();
    malicious.getMap('meta').set('name', 'Viewer must not write');
    sync.writeUpdate(update, Y.encodeStateAsUpdate(malicious));
    const before = Buffer.from(mock.only().data);
    const result = await page.evaluate(async ({ id, user, presence, update }) => {
      const response = await fetch(`/api/docs/${id}/live-ticket`, { method: 'POST', headers: { 'x-csrf-token': `csrf-${user}` } });
      const { ticket } = await response.json();
      const socket = new WebSocket(`${location.origin.replace('http', 'ws')}/api/docs/${id}/live?ticket=${ticket}`);
      socket.binaryType = 'arraybuffer';
      return new Promise<{ awareness: number[]; code: number }>((resolve, reject) => {
        let received: number[] = [];
        socket.onerror = () => reject(new Error('socket error'));
        socket.onopen = () => socket.send(new Uint8Array(presence));
        socket.onmessage = (event) => {
          const bytes = new Uint8Array(event.data);
          if (bytes[0] === 1) { received = [...bytes]; socket.send(new Uint8Array(update)); }
        };
        socket.onclose = (event) => resolve({ awareness: received, code: event.code });
      });
    }, { id: ID, user: viewer.id, presence: [...encoding.toUint8Array(message)], update: [...encoding.toUint8Array(update)] });
    expect(result.code).toBe(4003);
    const d = decoding.createDecoder(new Uint8Array(result.awareness));
    expect(decoding.readVarUint(d)).toBe(1);
    const fields = decoding.createDecoder(decoding.readVarUint8Array(d));
    expect(decoding.readVarUint(fields)).toBe(1);
    expect(decoding.readVarUint(fields)).toBe(77);
    decoding.readVarUint(fields);
    expect(JSON.parse(decoding.readVarString(fields))).toMatchObject({ user: { id: viewer.id, name: 'Visitor' }, cell: 'c1', ai: 'thinking' });
    expect(mock.only().data).toEqual(before);
    malicious.destroy();
  } finally { await context.close(); mock.dispose(); }
});

// Exercise the fixture independently of UI readiness with an actual browser WebSocket.
test('room fixture: authenticated ticket, real sync, persisted ack, and ticket replay rejection', async ({ browser }) => {
  const { mock, ada } = fixture();
  const context = await browser.newContext();
  await mock.attach(context, { user: ada, next: null });
  const page = await context.newPage();
  try {
    await page.route((url) => url.pathname === '/', (route) => route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>Wire fixture</title>' }));
    await page.goto('/');
    const ticket = await page.evaluate(async ({ id, user }) => {
      const r = await fetch(`/api/docs/${id}/live-ticket`, { method: 'POST', headers: { 'x-csrf-token': `csrf-${user}` } });
      return (await r.json()).ticket as string;
    }, { id: ID, user: ada.id });
    const local = new Y.Doc();
    local.getMap('meta').set('name', 'Wire commit');
    const e = encoding.createEncoder();
    encoding.writeVarUint(e, 0);
    sync.writeUpdate(e, Y.encodeStateAsUpdate(local));
    const ack = await page.evaluate(async ({ ticket, id, bytes }) => {
      const socket = new WebSocket(`${location.origin.replace('http', 'ws')}/api/docs/${id}/live?ticket=${ticket}`);
      socket.binaryType = 'arraybuffer';
      return await new Promise<number[]>((resolve, reject) => {
        socket.onopen = () => socket.send(new Uint8Array(bytes));
        socket.onerror = () => reject(new Error('socket error'));
        socket.onmessage = (event) => { const bytes = new Uint8Array(event.data); if (bytes[0] === 2) { resolve([...bytes]); socket.close(); } };
      });
    }, { ticket, id: ID, bytes: [...encoding.toUint8Array(e)] });
    const d = decoding.createDecoder(new Uint8Array(ack));
    expect(decoding.readVarUint(d)).toBe(2);
    expect(JSON.parse(decoding.readVarString(d))).toEqual({ generation: 1, saved: true });
    const persisted = new Y.Doc();
    Y.applyUpdate(persisted, mock.only().data);
    expect(persisted.getMap('meta').get('name')).toBe('Wire commit');
    const close = await page.evaluate(({ ticket, id }) => new Promise<number>((resolve) => {
      const socket = new WebSocket(`${location.origin.replace('http', 'ws')}/api/docs/${id}/live?ticket=${ticket}`);
      socket.onclose = (event) => resolve(event.code);
    }), { ticket, id: ID });
    expect(close).toBe(4001);
    persisted.destroy(); local.destroy();
  } finally { await context.close(); mock.dispose(); }
});
