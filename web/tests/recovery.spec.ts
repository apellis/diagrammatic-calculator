import { test, expect, type Browser, type Page } from '@playwright/test';
import * as Y from 'yjs';
import { Doc } from '../src/doc/model';
import { decodeBranch } from '../src/account/recovery';
import { MockAccounts, type User } from './mockapi';
import { cell } from './helpers';

const ID = 'recover000000001';
const CONTEXT = 'klr type=A2 nu=2i+j ring=QQ';
function state(text = 'watch x_1 e(iji)') {
  const d = new Doc(); d.setContext(CONTEXT); d.addCell(text);
  const bytes = Buffer.from(Y.encodeStateAsUpdate(d.ydoc));
  d.ydoc.destroy(); d.privateDoc.destroy(); return bytes;
}
function fixture(legacy = false) {
  const mock = new MockAccounts(true);
  const ada = mock.user('ada@example.org', 'Ada');
  const bob = mock.user('bob@example.org', 'Bob');
  let data = state();
  if (legacy) {
    const y = new Y.Doc({ gc: false });
    y.getMap('meta').set('context', CONTEXT);
    const c = new Y.Map(); c.set('id', 'w1'); c.set('text', 'watch x_1 e(iji)');
    y.getArray('cells').push([c]);
    const branch = new Doc(); branch.setContext(CONTEXT); branch.addCell('watch x_1^5 e(iji)');
    y.getMap('branches').set('legacy-personal', { id: 'legacy-personal', name: 'Legacy experiment', author: ada.id, created: 1, base: branch.inputs(), state: Y.encodeStateAsUpdate(branch.ydoc) });
    data = Buffer.from(Y.encodeStateAsUpdate(y));
    y.destroy(); branch.ydoc.destroy(); branch.privateDoc.destroy();
  }
  mock.docs.set(ID, { id: ID, owner: ada.id, name: 'Recovery algebra', created: 1, edited: 1, version: 1, generation: legacy ? 0 : 1, data, link: null, files: new Map() });
  mock.shares.set(ID, new Map([[bob.email, 'editor']]));
  return { mock, ada, bob };
}
async function device(browser: Browser, mock: MockAccounts, user: User) {
  const context = await browser.newContext();
  await mock.attach(context, { user, next: null });
  const page = await context.newPage(); await page.goto(`/#/s/${ID}`);
  await expect(cell(page, 0)).toBeVisible();
  return { context, page };
}
const text = (page: Page, value: string) => expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', value);
async function edit(page: Page, value: string) {
  await cell(page, 0).locator('.head').click();
  const editor = cell(page, 0).locator('.cm-content'); await editor.fill(value); await editor.press('Enter');
}
async function localIndex(page: Page) {
  return page.evaluate(() => new Promise<Array<{ id: string; cloud?: { id: string; generation: number } }>>((resolve, reject) => {
    const open = indexedDB.open('diagcalc'); open.onerror = () => reject(open.error);
    open.onsuccess = () => {
      const r = open.result.transaction('docs').objectStore('docs').getAll();
      r.onsuccess = () => { resolve(r.result); open.result.close(); };
      r.onerror = () => reject(r.error);
    };
  }));
}

test('owner upgrades generation zero through UI and keeps the original local recovery', async ({ browser }) => {
  const { mock, ada } = fixture(true); const a = await device(browser, mock, ada);
  try {
    const oldUrl = a.page.url(); const oldId = (await localIndex(a.page)).find(d => oldUrl.endsWith(d.id))!.id;
    await a.page.getByRole('button', { name: 'Upgrade for live editing', exact: true }).click();
    await expect(a.page).not.toHaveURL(oldUrl);
    await expect.poll(() => mock.docs.get(ID)!.generation).toBe(1);
    await text(a.page, 'x_1 e(iji)');
    await expect(a.page.getByLabel('Branches').locator('option')).toContainText(['1 branch', 'Legacy experiment']);
    const index = await localIndex(a.page);
    expect(index.find(d => d.id === oldId)?.cloud).toBeUndefined();
    expect(index.find(d => d.id !== oldId)?.cloud).toMatchObject({ id: ID, generation: 1 });
    expect(mock.versions.get(ID)?.get('before-migration-0')?.data).toBeDefined();
    const shared = new Y.Doc(); Y.applyUpdate(shared, mock.docs.get(ID)!.data);
    expect(shared.getMap('branches').size).toBe(0); shared.destroy();
    await a.page.goto(oldUrl); await text(a.page, 'x_1 e(iji)');
    await expect(a.page.getByLabel('Branches').locator('option')).toContainText(['1 branch', 'Legacy experiment']);
  } finally { await a.context.close(); mock.dispose(); }
});

test('nonowner generation zero is read-only and cannot reopen before owner migration', async ({ browser }) => {
  const { mock, bob } = fixture(true); const b = await device(browser, mock, bob);
  try {
    const oldUrl = b.page.url(); const before = Buffer.from(mock.docs.get(ID)!.data);
    await expect(b.page.getByRole('button', { name: 'Upgrade for live editing', exact: true })).toHaveCount(0);
    await expect(b.page.locator('[data-testid="add-cell"]:enabled')).toHaveCount(0);
    await cell(b.page, 0).locator('.head').click();
    await expect(b.page.locator('.cm-content[contenteditable="true"]')).toHaveCount(0);
    await b.page.getByRole('button', { name: 'Reopen account document safely' }).click();
    await expect(b.page.getByRole('alert')).toContainText('owner must upgrade');
    await expect(b.page).toHaveURL(oldUrl);
    expect(mock.docs.get(ID)!.data).toEqual(before);
    expect(mock.log.filter(l => l.method === 'PUT' || l.path.endsWith('/migrate'))).toEqual([]);
  } finally { await b.context.close(); mock.dispose(); }
});

test('generation mismatch safely reopens current cloud state and preserves offline old copy', async ({ browser }) => {
  const { mock, ada } = fixture(); const a = await device(browser, mock, ada);
  try {
    await expect.poll(() => mock.rooms.get(ID)?.peers.size).toBe(1);
    const oldUrl = a.page.url();
    await a.context.route('**/live-ticket', route => route.abort('internetdisconnected'));
    await a.context.setOffline(true); mock.room(ID).disconnect(ada);
    await expect(a.page.getByTestId('sync')).toContainText(/offline|reconnect/i);
    await edit(a.page, 'watch x_1^7 e(iji)');
    mock.room(ID).dispose(); mock.rooms.delete(ID);
    mock.docs.get(ID)!.data = state('watch x_2^3 e(iji)'); mock.docs.get(ID)!.generation = 2; mock.docs.get(ID)!.version++;
    await a.context.unroute('**/live-ticket'); await a.context.setOffline(false);
    await expect(a.page.getByText('The account document has a newer generation.', { exact: false })).toBeVisible();
    await a.page.getByRole('button', { name: 'Reopen account document safely' }).click();
    await expect(a.page).not.toHaveURL(oldUrl); await text(a.page, 'x_2^3 e(iji)');
    const index = await localIndex(a.page);
    expect(index.find(d => oldUrl.endsWith(d.id))?.cloud).toBeUndefined();
    await a.page.goto(oldUrl); await text(a.page, 'x_1^7 e(iji)');
    expect((await localIndex(a.page)).find(d => oldUrl.endsWith(d.id))?.cloud, 'recovery stays local even when reopened while signed in').toBeUndefined();
    const shared = new Y.Doc(); Y.applyUpdate(shared, mock.docs.get(ID)!.data);
    expect(new Doc(shared).list().map(c => c.text)).toEqual(['watch x_2^3 e(iji)']); shared.destroy();
  } finally { await a.context.close(); mock.dispose(); }
});

test('personal branch follows same user to second device, never another collaborator', async ({ browser }) => {
  const { mock, ada, bob } = fixture(); const a = await device(browser, mock, ada);
  try {
    await a.page.getByTestId('branch-start').click(); await a.page.getByLabel('Branch name').fill('Ada private experiment');
    await a.page.getByRole('button', { name: 'Start', exact: true }).click();
    await edit(a.page, 'watch x_1^9 e(iji)');
    await expect.poll(() => [...(mock.branches.get(`${ID}:${ada.id}`)?.values() ?? [])].some(b => {
      const branch = decodeBranch(b.data), y = new Y.Doc(); Y.applyUpdate(y, branch.state);
      const saved = new Doc(y).list().some(c => c.text === 'watch x_1^9 e(iji)'); y.destroy(); return saved;
    })).toBe(true);
    await a.page.getByRole('button', { name: 'Leave', exact: true }).click();
    await text(a.page, 'x_1 e(iji)');
    const second = await device(browser, mock, ada), other = await device(browser, mock, bob);
    try {
      await expect(second.page.getByLabel('Branches').locator('option')).toContainText(['1 branch', 'Ada private experiment']);
      await second.page.getByLabel('Branches').selectOption({ label: 'Ada private experiment' });
      await text(second.page, 'x_1^9 e(iji)');
      await expect.poll(() => mock.log.some(l => l.who === bob.email && l.path.endsWith('/branches'))).toBe(true);
      await expect(other.page.getByLabel('Branches')).toHaveCount(0); await text(other.page, 'x_1 e(iji)');
      expect(mock.branches.get(`${ID}:${bob.id}`)?.size).toBe(0);
    } finally { await second.context.close(); await other.context.close(); }
  } finally { await a.context.close(); mock.dispose(); }
});

test('cloud checkpoint is a read-only preview and opens as detached personal branch', async ({ browser }) => {
  const { mock, ada } = fixture(); mock.checkpoint(ID, 'saved-checkpoint', 'Ada', 'Before experiment', state('watch x_2^4 e(iji)'));
  const a = await device(browser, mock, ada);
  try {
    await a.page.getByRole('button', { name: 'Cloud checkpoints', exact: true }).click();
    const dialog = a.page.getByRole('dialog', { name: 'Cloud checkpoints' });
    await expect(dialog).toContainText('Ada · Before experiment');
    await dialog.getByRole('button', { name: 'Preview checkpoint' }).click();
    const preview = a.page.getByRole('region', { name: 'Read-only checkpoint preview' });
    await expect(preview).toContainText('watch x_2^4 e(iji)');
    await expect(preview.locator('input,textarea,[contenteditable="true"]')).toHaveCount(0);
    await text(a.page, 'x_1 e(iji)');
    await preview.getByRole('button', { name: 'Open as personal branch' }).click();
    await expect(a.page.getByTestId('branch')).toContainText('Checkpoint: Before experiment');
    await text(a.page, 'x_2^4 e(iji)');
    await a.page.getByRole('button', { name: 'Leave', exact: true }).click(); await text(a.page, 'x_1 e(iji)');
    expect(new Doc(mock.room(ID).doc).list().map(c => c.text)).toEqual(['watch x_1 e(iji)']);
  } finally { await a.context.close(); mock.dispose(); }
});
