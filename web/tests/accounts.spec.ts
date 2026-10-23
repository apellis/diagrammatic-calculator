import { expect, test, type Browser, type Page } from '@playwright/test';
import { addCell, cell, newDoc, typeCell } from './helpers';
import { MockAccounts, type Session, type User } from './mockapi';

/** A browser of its own (its own storage: another device), served by the mock as `user`. */
async function device(browser: Browser, mock: MockAccounts, user: User | null) {
  const context = await browser.newContext();
  const session: Session = { user, next: null };
  await mock.attach(context, session);
  return { page: await context.newPage(), session, context };
}

const saved = (page: Page) => expect(page.getByTestId('sync')).toHaveText('Saved to your account', { timeout: 15_000 });

test('without an accounts API the app is the anonymous app, unchanged', async ({ page }) => {
  // the preview server answers /api/me with the page itself, as a static host would
  await page.goto('/');
  await expect(page.locator('h1')).toHaveText('diagrammatic-calculator');
  await expect(page.locator('.doclist > p.muted').first()).toHaveText('Exact computations in diagrammatic algebras. Documents live in this browser; export them to keep a copy.');
  await expect(page.getByTestId('account')).toHaveCount(0);
  await expect(page.getByTestId('shared-with-me')).toHaveCount(0);
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iji)');
  await expect(cell(page, 0).getByTestId('summary')).not.toHaveText('');
  await expect(page.getByTestId('sync')).toHaveCount(0);
  await expect(page.getByTestId('share')).toHaveCount(0);
});

test('signed out, Sign in leaves for Google and returns to the page; a failed sign-in says so', async ({ browser }) => {
  const mock = new MockAccounts();
  const { page } = await device(browser, mock, null);
  await page.goto('/?signin=failed#/');
  await expect(page.getByTestId('notice')).toHaveText('Signing in did not work. Please try again.');
  await expect(page).toHaveURL(/\/#\/$/);
  await expect(page.locator('.doclist > p.muted').first()).toContainText('Documents live in this browser');
  await page.getByTestId('sign-in').click();
  await expect.poll(() => mock.log.some((l) => l.path === 'auth/login')).toBe(true);
  expect(mock.log.find((l) => l.path === 'auth/login')).toMatchObject({ method: 'GET' });
  // the mock signed nobody in, so the page is back, signed out
  await expect(page.getByTestId('sign-in')).toBeVisible();
});

test('the phase 6 gate: a document made anonymously survives sign-in, opens on a second device, and is shared read-only', async ({ browser }) => {
  test.setTimeout(120_000);
  const mock = new MockAccounts();
  const ada = mock.user('ada@example.org', 'Ada');
  const bob = mock.user('bob@example.org', 'Bob');

  // made anonymously
  const a = await device(browser, mock, null);
  await newDoc(a.page);
  await typeCell(a.page, 'def a := x_1 e(iji)');
  await expect(cell(a.page, 0).getByTestId('summary')).not.toHaveText('');
  await a.page.goto('/#/');
  await expect(a.page.locator('tr[data-doc]')).toHaveCount(1);

  // signing in saves it to the account
  a.session.next = ada;
  await a.page.getByTestId('sign-in').click();
  await expect(a.page.getByTestId('notice')).toHaveText('Saved 1 document from this browser to your account.');
  await expect(a.page.getByTestId('account-name')).toHaveText('Ada');
  await expect(a.page.locator('tr[data-doc][data-cloud]')).toHaveCount(1);
  const doc = mock.only();
  expect(doc.owner).toBe(ada.id);
  expect(doc.version).toBe(1);
  expect(mock.log.filter((l) => l.method !== 'GET').every((l) => l.csrf === `csrf-${ada.id}`)).toBe(true);

  // an edit is saved to the account shortly after
  await a.page.locator('tr[data-doc] td.open').click();
  await saved(a.page);
  await addCell(a.page, 'watch a a');
  await expect.poll(() => mock.only().version, { timeout: 15_000 }).toBe(2);
  await saved(a.page);

  // a second device signed in to the same account has it
  const b = await device(browser, mock, ada);
  await b.page.goto('/');
  await expect(b.page.locator('tr[data-doc]')).toHaveCount(1);
  await b.page.locator('tr[data-doc] td.open').click();
  await expect(b.page.locator('.cell[data-cell]:not([data-cell="new"])')).toHaveCount(2);
  await expect(cell(b.page, 0).locator('.head .text')).toHaveAttribute('data-text', /x_1 e\(iji\)/);
  await saved(b.page);

  // shared read-only with a second account
  await a.page.getByTestId('share').click();
  const dialog = a.page.getByRole('dialog', { name: 'Share' });
  await dialog.getByLabel('E-mail address').fill('bob@example.org');
  await dialog.getByRole('button', { name: 'Share', exact: true }).click();
  await expect(dialog.getByTestId('share-note')).toHaveText('Shared with bob@example.org, who was sent an e-mail.');
  await expect(dialog.locator('tr[data-person="bob@example.org"]')).toHaveCount(1);
  expect(mock.shares.get(doc.id)?.get('bob@example.org')).toBe('viewer');
  expect(mock.mails).toEqual([{ to: 'bob@example.org', doc: doc.id }]);
  await dialog.getByRole('button', { name: 'Done' }).click();

  const c = await device(browser, mock, bob);
  await c.page.goto('/');
  const shared = c.page.getByTestId('shared-with-me');
  await expect(shared.locator(`tr[data-shared="${doc.id}"]`)).toContainText('can view');
  await shared.locator('td.open').click();
  await expect(c.page).toHaveURL(/#\/doc\//);
  await expect(c.page.locator('.cell[data-cell]:not([data-cell="new"])')).toHaveCount(2);
  await expect(c.page.getByTestId('sync')).toHaveText('View only');
  // A viewer cannot enter an editor or add a cell (including legacy REST documents).
  await expect(c.page.locator('[data-testid="add-cell"]:enabled')).toHaveCount(0);
  await cell(c.page, 0).locator('.head').click();
  await expect(c.page.locator('.cm-content[contenteditable="true"]')).toHaveCount(0);
  expect(mock.log.some((l) => l.who === bob.email && l.method === 'PUT')).toBe(false);
  expect(mock.only().version).toBe(2);
  // and the owner's sharing controls are not theirs
  await c.page.getByTestId('share').click();
  await expect(c.page.getByRole('dialog', { name: 'Share' })).toContainText('Only the owner can change who has access.');

  await a.context.close();
  await b.context.close();
  await c.context.close();
});

test('on a shared computer, signing out can remove the account\'s documents from the browser, keeping unsaved changes', async ({ browser }) => {
  test.setTimeout(90_000);
  const mock = new MockAccounts();
  const ada = mock.user('ada@example.org', 'Ada');
  const bob = mock.user('bob@example.org', 'Bob');
  const a = await device(browser, mock, ada);
  await newDoc(a.page);
  await typeCell(a.page, 'def a := x_1 e(iji)');
  await saved(a.page);
  const shared = mock.only().id;
  mock.shares.set(shared, new Map([['bob@example.org', 'viewer']]));

  // Bob has an owned document with an unsaved edit, and a clean viewer copy of Ada's.
  const b = await device(browser, mock, bob);
  await newDoc(b.page);
  await typeCell(b.page, 'def b := x_2 e(iji)');
  await saved(b.page);
  const ownedUrl = b.page.url();
  await b.page.goto('/#/');
  await b.page.getByTestId('shared-with-me').locator(`tr[data-shared="${shared}"] td.open`).click();
  await expect(b.page.getByTestId('sync')).toHaveText('View only');
  await b.page.goto('/#/');
  const owned = [...mock.docs.values()].find((d) => d.owner === bob.id)!;
  // Simulate unavailable snapshot storage, not editable viewer permissions. Logout remains online.
  await b.context.route(`**/api/docs/${owned.id}/snapshot`, (route) =>
    route.request().method() === 'PUT' ? route.abort('internetdisconnected') : route.fallback());
  await b.page.goto(ownedUrl);
  await expect(cell(b.page, 0)).toBeVisible();
  await addCell(b.page, 'watch x_2 e(iji)');
  await b.page.goto('/#/');
  await expect(b.page.locator('tr[data-doc]')).toHaveCount(2);

  await b.page.getByTestId('sign-out').click();
  const ask = b.page.getByRole('dialog', { name: 'Sign out' });
  await ask.getByTestId('sign-out-clear').check();
  await ask.getByTestId('sign-out-confirm').click();
  await expect(b.page.getByTestId('sign-in')).toBeVisible();
  await expect(b.page.getByTestId('notice')).toHaveText(
    'Removed 1 document of the account from this browser. Kept 1 document with changes the account does not have; export or delete it if this computer is shared.',
  );
  await expect(b.page.locator('tr[data-doc]')).toHaveCount(1);
  await expect(b.page.locator('tr[data-doc]')).not.toContainText('shared, can view');

  // Signing in again retains the owned unsaved document and lists the shared document.
  b.session.next = bob;
  await b.page.getByTestId('sign-in').click();
  await expect(b.page.getByTestId('account-name')).toHaveText('Bob');
  await expect(b.page.locator('tr[data-doc]')).toHaveCount(1);
  await expect(b.page.getByTestId('shared-with-me').locator(`tr[data-shared="${shared}"]`)).toBeVisible();
  await b.page.locator('tr[data-doc] td.open').click();
  await expect(cell(b.page, 1).locator('.head .text')).toHaveAttribute('data-text', 'x_2 e(iji)');
  await a.context.close();
  await b.context.close();
});

test('the files kept with a document are listed, and can be downloaded, opened and removed', async ({ browser }) => {
  const mock = new MockAccounts();
  const ada = mock.user('ada@example.org', 'Ada');
  const a = await device(browser, mock, ada);
  await newDoc(a.page);
  await typeCell(a.page, 'def a := x_1 e(iji)');
  await saved(a.page);
  const exported = a.page.waitForEvent('download');
  await a.page.keyboard.press('ControlOrMeta+Shift+S');
  await exported;
  await expect.poll(() => mock.only().files.size).toBe(1);
  const name = [...mock.only().files.keys()][0];

  await a.page.keyboard.press('ControlOrMeta+K');
  await a.page.getByRole('option', { name: /Files kept with this document/ }).click();
  const dialog = a.page.getByRole('dialog', { name: 'Files kept with this document' });
  const row = dialog.locator(`tr[data-file="${name}"]`);
  await expect(row).toContainText(name);
  const down = a.page.waitForEvent('download');
  await row.getByRole('button', { name: 'Download' }).click();
  expect((await down).suggestedFilename()).toBe(name);
  await row.getByRole('button', { name: 'Remove' }).click();
  await expect(dialog.getByTestId('kept-files')).toContainText('No files yet');
  expect(mock.only().files.size).toBe(0);
  await dialog.getByRole('button', { name: 'Done' }).click();

  // a file kept with the document opens as a new document
  mock.only().files.set('older.dcw', 'diagcalc-workspace/1\ncontext klr type=A2 nu=2i+j ring=QQ\nwatch x_2 e(iji)  # id=wt1\n');
  await a.page.keyboard.press('ControlOrMeta+K');
  await a.page.getByRole('option', { name: /Files kept with this document/ }).click();
  const before = a.page.url();
  await a.page.getByRole('dialog', { name: 'Files kept with this document' }).locator('tr[data-file="older.dcw"]').getByRole('button', { name: 'Open as a new document' }).click();
  await expect(a.page).not.toHaveURL(before);
  await expect(cell(a.page, 0).locator('.head .text')).toHaveAttribute('data-text', 'x_2 e(iji)');
  await a.context.close();
});

test('a view link opens the document without an account', async ({ browser }) => {
  const mock = new MockAccounts();
  const ada = mock.user('ada@example.org', 'Ada');
  const a = await device(browser, mock, ada);
  await newDoc(a.page);
  await saved(a.page);
  // the page's timers are held from here, so the save that follows an edit after a pause never
  // comes by itself: opening Share saves the edit at once, and the link shares it
  await a.page.clock.install();
  await a.page.clock.pauseAt(await a.page.evaluate(() => Date.now() + 1000));
  await typeCell(a.page, 'def a := x_1 e(iji)');
  // Enter writes the cell once the engine has checked it
  await expect(cell(a.page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := x_1 e(iji)');
  await expect(a.page.getByTestId('sync')).toHaveText('Saving…');
  await a.page.getByTestId('share').click();
  await saved(a.page);
  expect(mock.only().data.includes('x_1 e(iji)')).toBe(true);
  await a.page.clock.resume();
  const dialog = a.page.getByRole('dialog', { name: 'Share' });
  await dialog.getByTestId('link-role').selectOption('viewer');
  const id = mock.only().id;
  await expect(dialog.getByTestId('share-link')).toHaveValue(new RegExp(`/#/s/${id}/token${id}$`));
  const url = await dialog.getByTestId('share-link').inputValue();

  const d = await device(browser, mock, null);
  await d.page.goto(url.replace(/^https?:\/\/[^/]+/, ''));
  await expect(d.page).toHaveURL(/#\/doc\//);
  await expect(d.page.locator('.cell[data-cell]:not([data-cell="new"])')).toHaveCount(1);
  await expect(d.page.getByTestId('sync')).toHaveText('View only');
  expect(mock.log.filter((l) => l.who === null && l.path.startsWith('docs/')).every((l) => l.link === `token${id}`)).toBe(true);

  // without the link and without an account, the page asks to sign in
  const e = await device(browser, mock, null);
  await e.page.goto(`/#/s/${id}`);
  await expect(e.page.getByTestId('open-shared')).toContainText('Sign in to open this document.');

  // the owner is told that sharing sends the whole history, and can share a clean copy instead
  await expect(dialog.getByTestId('share-history-note')).toContainText('including text that was later deleted');
  const before = a.page.url();
  await dialog.getByRole('link', { name: 'Share a clean copy instead' }).click();
  await expect(a.page).not.toHaveURL(before);
  await expect(a.page).toHaveURL(/#\/doc\//);
  await expect(cell(a.page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := x_1 e(iji)');
  await a.context.close();
  await d.context.close();
  await e.context.close();
});

test('signing out keeps the documents here; deleting an owned document deletes it from the account', async ({ browser }) => {
  const mock = new MockAccounts();
  const ada = mock.user('ada@example.org', 'Ada');
  const a = await device(browser, mock, ada);
  await newDoc(a.page);
  await typeCell(a.page, 'def a := x_1 e(iji)');
  await saved(a.page);
  await a.page.goto('/#/');
  await a.page.getByTestId('sign-out').click();
  await a.page.getByRole('dialog', { name: 'Sign out' }).getByTestId('sign-out-confirm').click();
  await expect(a.page.getByTestId('sign-in')).toBeVisible();
  expect(mock.log.find((l) => l.path === 'auth/logout')).toMatchObject({ method: 'POST', csrf: `csrf-${ada.id}` });
  await expect(a.page.locator('tr[data-doc]')).toHaveCount(1);
  await expect(a.page.getByTestId('shared-with-me')).toHaveCount(0);

  a.session.next = ada;
  await a.page.getByTestId('sign-in').click();
  await expect(a.page.getByTestId('account-name')).toHaveText('Ada');
  await a.page.locator('tr[data-doc]').getByRole('button', { name: 'Delete' }).click();
  const confirm = a.page.getByRole('dialog', { name: 'Delete the document' });
  await expect(confirm).toContainText('from your account and this browser');
  await confirm.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(a.page.locator('tr[data-doc]')).toHaveCount(0);
  expect(mock.docs.size).toBe(0);
  await a.context.close();
});
