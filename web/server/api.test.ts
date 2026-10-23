import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { clearKeyCache } from './oidc';
import { FLOW_COOKIE, SESSION_COOKIE } from './auth';
import { LIMITS } from './docs';
import { handle } from './router';
import { CLIENT_ID, ORIGIN, World, type Browser } from './testing';

const ada = { sub: 'g-ada', email: 'ada@example.org', name: 'Ada' };
const bob = { sub: 'g-bob', email: 'bob@example.org', name: 'Bob' };
const cy = { sub: 'g-cy', email: 'cy@example.org', name: 'Cy' };

let w: World;
beforeAll(async () => {
  w = await World.create();
});
afterAll(() => w.dispose());
beforeEach(() => {
  clearKeyCache();
  w.mails = [];
});

const body = async (r: Response) => new Uint8Array(await r.arrayBuffer());
const put = (b: Browser, id: string, version: number, data: Uint8Array<ArrayBuffer>, headers: Record<string, string> = {}) =>
  b.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', headers: { 'if-match': `"${version}"`, ...headers }, body: data });

describe('without configuration', () => {
  it('reports that accounts are not available, and every other route answers 503', async () => {
    const bare = await World.create({ accounts: false });
    const b = bare.browser();
    expect(await (await b.fetch('/api/me')).json()).toEqual({ accounts: false, user: null });
    expect((await b.fetch('/api/docs')).status).toBe(503);
    expect((await b.fetch('/api/docs', { method: 'POST', json: {} })).status).toBe(503);
    const login = await b.fetch('/api/auth/login');
    expect(login.status).toBe(302);
    expect(login.headers.get('location')).toBe('/?signin=unavailable#/');
    // a secret too short to sign with counts as not configured
    bare.env = { ...w.env, SESSION_SECRET: 'short' };
    expect(await (await b.fetch('/api/me')).json()).toEqual({ accounts: false, user: null });
    await bare.dispose();
  });
});

describe('sign-in', () => {
  it('sends the browser to Google with state, nonce and a PKCE challenge in a sealed cookie', async () => {
    const b = w.browser();
    const r = await b.fetch('/api/auth/login?return=%2F%23%2Fdoc%2Fabc');
    expect(r.status).toBe(302);
    const to = new URL(r.headers.get('location')!);
    expect(to.origin + to.pathname).toBe('https://accounts.google.com/o/oauth2/v2/auth');
    const p = to.searchParams;
    expect(p.get('client_id')).toBe(CLIENT_ID);
    expect(p.get('redirect_uri')).toBe(`${ORIGIN}/api/auth/callback`);
    expect(p.get('response_type')).toBe('code');
    expect(p.get('scope')).toBe('openid email profile');
    expect(p.get('code_challenge_method')).toBe('S256');
    expect(p.get('code_challenge')).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(p.get('state')).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(p.get('nonce')).toMatch(/^[A-Za-z0-9_-]{43}$/);
    const cookie = r.headers.getSetCookie()[0];
    expect(cookie).toMatch(new RegExp(`^${FLOW_COOKIE}=[^;]+; Path=/; Max-Age=600; HttpOnly; Secure; SameSite=Lax$`));
    // the verifier never leaves in the clear: the cookie is signed and holds no challenge
    expect(cookie).not.toContain(p.get('code_challenge'));
  });

  it('signs in, returns to the page, and sets an httpOnly session cookie', async () => {
    const b = w.browser();
    const back = await b.signIn(ada, '/#/doc/abc');
    expect(back.status).toBe(302);
    expect(back.headers.get('location')).toBe('/#/doc/abc');
    const set = back.headers.getSetCookie();
    expect(set.find((c) => c.startsWith(SESSION_COOKIE))).toMatch(/; Path=\/; Max-Age=2592000; HttpOnly; Secure; SameSite=Lax$/);
    expect(set.find((c) => c.startsWith(FLOW_COOKIE))).toContain('Max-Age=0');
    const me = await b.refresh();
    expect(me.accounts).toBe(true);
    expect(me.user).toMatchObject({ email: 'ada@example.org', name: 'Ada' });
    expect(me.csrf).toMatch(/^[A-Za-z0-9_-]{43}$/);
    // the database holds a hash of the session ID, not the ID from the cookie
    const sid = b.jar.get(SESSION_COOKIE)!.split('.')[0];
    const rows = await w.DB.prepare('SELECT id_hash FROM sessions').all<{ id_hash: string }>();
    expect(rows.results.some((r) => r.id_hash === sid)).toBe(false);
  });

  it('keeps one user per Google account and refreshes the address and name', async () => {
    await w.signedIn({ ...cy, name: 'Cy Old' });
    const b = await w.signedIn({ ...cy, email: 'CY@example.org', name: 'Cy New' });
    expect((await b.refresh()).user).toMatchObject({ email: 'cy@example.org', name: 'Cy New' });
    expect((await w.DB.prepare('SELECT COUNT(*) AS n FROM users WHERE google_sub = ?').bind(cy.sub).first<{ n: number }>())!.n).toBe(1);
  });

  it.each([
    ['/', '/'],
    ['/#/doc/x', '/#/doc/x'],
    ['//evil.example', '/'],
    ['/\\evil.example', '/'],
    ['https://evil.example/', '/'],
    ['javascript:alert(1)', '/'],
  ])('returns to %s as %s', async (ret, want) => {
    const b = w.browser();
    expect((await b.signIn(ada, ret)).headers.get('location')).toBe(want);
  });

  async function startedFlow() {
    const b = w.browser();
    const start = await b.fetch('/api/auth/login');
    return { b, ...w.google.authorize(start.headers.get('location')!, ada) };
  }
  const session = (b: Browser) => b.jar.has(SESSION_COOKIE);

  it('refuses a wrong state', async () => {
    const { b, code } = await startedFlow();
    const r = await b.fetch(`/api/auth/callback?code=${code}&state=forged`);
    expect(r.headers.get('location')).toBe('/?signin=failed#/');
    expect(session(b)).toBe(false);
  });

  it('refuses a callback without the browser that started it', async () => {
    const { code, state } = await startedFlow();
    const other = w.browser();
    const r = await other.fetch(`/api/auth/callback?code=${code}&state=${state}`);
    expect(r.headers.get('location')).toBe('/?signin=expired#/');
    expect(session(other)).toBe(false);
  });

  it('refuses a tampered or expired sign-in cookie', async () => {
    const t = await startedFlow();
    t.b.jar.set(FLOW_COOKIE, t.b.jar.get(FLOW_COOKIE)!.replace(/^./, (c) => (c === 'e' ? 'f' : 'e')));
    expect((await t.b.fetch(`/api/auth/callback?code=${t.code}&state=${t.state}`)).headers.get('location')).toBe('/?signin=expired#/');
    const u = await startedFlow();
    w.clock += 11 * 60_000;
    expect((await u.b.fetch(`/api/auth/callback?code=${u.code}&state=${u.state}`)).headers.get('location')).toBe('/?signin=expired#/');
    w.clock -= 11 * 60_000;
  });

  it('reports a cancelled sign-in', async () => {
    const { b, state } = await startedFlow();
    expect((await b.fetch(`/api/auth/callback?error=access_denied&state=${state}`)).headers.get('location')).toBe('/?signin=cancelled#/');
  });

  it('refuses a code the token endpoint rejects (wrong verifier, reused code)', async () => {
    const { b, code, state } = await startedFlow();
    const once = await b.fetch(`/api/auth/callback?code=${code}&state=${state}`);
    expect(once.headers.get('location')).toBe('/');
    const again = w.browser();
    const s = await again.fetch('/api/auth/login');
    const p = new URL(s.headers.get('location')!).searchParams;
    expect((await again.fetch(`/api/auth/callback?code=${code}&state=${p.get('state')}`)).headers.get('location')).toBe('/?signin=failed#/');
  });

  it('refuses an ID token for another sign-in (nonce) or an unverified address', async () => {
    for (const override of [{ nonce: 'replayed' }, { email_verified: false }, { aud: 'other-client' }]) {
      const b = w.browser();
      const start = await b.fetch('/api/auth/login');
      const { code, state } = w.google.authorize(start.headers.get('location')!, ada, override);
      expect((await b.fetch(`/api/auth/callback?code=${code}&state=${state}`)).headers.get('location')).toBe('/?signin=failed#/');
      expect(session(b)).toBe(false);
    }
  });
});

describe('sessions and CSRF', () => {
  it('ignores a session cookie with a forged signature', async () => {
    const b = await w.signedIn(ada);
    const [id] = b.jar.get(SESSION_COOKIE)!.split('.');
    b.jar.set(SESSION_COOKIE, `${id}.AAAA`);
    expect((await b.refresh()).user).toBeNull();
  });

  it('expires sessions after thirty days', async () => {
    const b = await w.signedIn(ada);
    w.clock += 31 * 86400_000;
    expect((await b.refresh()).user).toBeNull();
    w.clock -= 31 * 86400_000;
  });

  it('signs out here and in the database', async () => {
    const b = await w.signedIn(ada);
    const cookie = b.jar.get(SESSION_COOKIE)!;
    const r = await b.fetch('/api/auth/logout', { method: 'POST' });
    expect(r.status).toBe(204);
    expect(b.jar.has(SESSION_COOKIE)).toBe(false);
    // the old cookie no longer works anywhere
    b.jar.set(SESSION_COOKIE, cookie);
    expect((await b.refresh()).user).toBeNull();
  });

  it('needs the CSRF token, from the same session, on every state-changing request', async () => {
    const a = await w.signedIn(ada);
    const other = await w.signedIn(ada);
    const create = (headers: Record<string, string>) => a.fetch('/api/docs', { method: 'POST', json: { name: 'x' }, headers });
    expect((await create({ 'x-csrf-token': '' })).status).toBe(403);
    expect((await a.fetch('/api/docs', { method: 'POST', json: { name: 'x' }, headers: { 'x-csrf-token': 'wrong' } })).status).toBe(403);
    expect((await create({ 'x-csrf-token': other.csrf })).status).toBe(403);
    expect((await create({ origin: 'https://evil.example' })).status).toBe(403);
    expect((await create({ 'sec-fetch-site': 'cross-site' })).status).toBe(403);
    expect((await create({ 'sec-fetch-site': 'same-origin' })).status).toBe(201);
    // signing out needs it too, so another site cannot sign people out
    expect((await a.fetch('/api/auth/logout', { method: 'POST', headers: { 'x-csrf-token': 'wrong' } })).status).toBe(403);
    expect((await a.refresh()).user).not.toBeNull();
  });

  it('refuses state-changing requests without a session', async () => {
    const b = w.browser();
    expect((await b.fetch('/api/docs', { method: 'POST', json: {} })).status).toBe(401);
    expect((await b.fetch('/api/docs')).status).toBe(401);
  });
});

describe('documents', () => {
  let a: Browser;
  beforeAll(async () => {
    a = await w.signedIn(ada);
  });

  it('are created, listed, saved and read back exactly', async () => {
    const r = await a.fetch('/api/docs', { method: 'POST', json: { name: '  Gram\nmatrices ' } });
    expect(r.status).toBe(201);
    const doc = (await r.json()) as { id: string; name: string; version: number; role: string; link: unknown };
    expect(doc).toMatchObject({ name: 'Gram matrices', version: 0, role: 'owner', link: null });
    expect((await a.fetch(`/api/docs/${doc.id}/snapshot`)).headers.get('x-doc-version')).toBe('0');
    const data = new Uint8Array(1000).map((_, i) => (i * 7) % 256);
    const saved = await put(a, doc.id, 0, data, { 'x-doc-name': encodeURIComponent('Gram matrices, A₂') });
    expect(await saved.json()).toEqual({ version: 1 });
    const got = await a.fetch(`/api/docs/${doc.id}/snapshot`);
    expect(got.headers.get('x-doc-version')).toBe('1');
    expect(got.headers.get('x-doc-role')).toBe('owner');
    expect(await body(got)).toEqual(data);
    const list = (await (await a.fetch('/api/docs')).json()) as { owned: { id: string; name: string; version: number }[] };
    expect(list.owned.find((d) => d.id === doc.id)).toMatchObject({ name: 'Gram matrices, A₂', version: 1 });
  });

  it('save only on top of the version they were read at (409 tells the current one)', async () => {
    const id = await a.newDoc();
    expect((await put(a, id, 1, new Uint8Array([4]))).status).toBe(200);
    const stale = await put(a, id, 1, new Uint8Array([5]));
    expect(stale.status).toBe(409);
    expect(await stale.json()).toMatchObject({ version: 2 });
    expect(await body(await a.fetch(`/api/docs/${id}/snapshot`))).toEqual(new Uint8Array([4]));
    expect((await a.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', body: new Uint8Array([6]) })).status).toBe(428);
    expect((await put(a, id, 2, new Uint8Array())).status).toBe(400);
    // only the current snapshot is kept
    const objects = await w.FILES.list({ prefix: `docs/${id}/snapshots/` });
    expect(objects.objects).toHaveLength(1);
  });

  it('refuse snapshots over the size limit', async () => {
    const id = await a.newDoc();
    expect((await put(a, id, 1, new Uint8Array(LIMITS.snapshotBytes + 1))).status).toBe(413);
  });

  it('answer 404 for unknown or malformed IDs', async () => {
    expect((await a.fetch('/api/docs/AAAAAAAAAAAAAAAA')).status).toBe(404);
    expect((await a.fetch('/api/docs/..%2F..%2Fetc')).status).toBe(404);
    expect((await a.fetch('/api/nothing')).status).toBe(404);
  });

  it('keep exported files', async () => {
    const id = await a.newDoc();
    const text = 'context klr type=A2\n';
    expect((await a.fetch(`/api/docs/${id}/files/notes.dcw`, { method: 'PUT', body: text })).status).toBe(201);
    expect((await a.fetch(`/api/docs/${id}/files/notes.exe`, { method: 'PUT', body: text })).status).toBe(400);
    expect((await a.fetch(`/api/docs/${id}/files/..%2Fsnapshot.dcw`, { method: 'PUT', body: text })).status).toBe(400);
    const list = (await (await a.fetch(`/api/docs/${id}/files`)).json()) as { files: { name: string; size: number }[] };
    expect(list.files).toEqual([expect.objectContaining({ name: 'notes.dcw', size: text.length })]);
    const f = await a.fetch(`/api/docs/${id}/files/notes.dcw`);
    expect(await f.text()).toBe(text);
    expect(f.headers.get('content-disposition')).toBe('attachment; filename="notes.dcw"');
  });

  it('are deleted with their shares and stored objects', async () => {
    const id = await a.newDoc();
    await a.fetch(`/api/docs/${id}/files/x.dcw`, { method: 'PUT', body: 'x' });
    await a.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'bob@example.org', role: 'viewer', notify: false } });
    expect((await a.fetch(`/api/docs/${id}`, { method: 'DELETE' })).status).toBe(204);
    expect((await a.fetch(`/api/docs/${id}`)).status).toBe(404);
    expect((await w.FILES.list({ prefix: `docs/${id}/` })).objects).toHaveLength(0);
    expect((await w.DB.prepare('SELECT COUNT(*) AS n FROM shares WHERE doc_id = ?').bind(id).first<{ n: number }>())!.n).toBe(0);
  });
});

describe('sharing and access', () => {
  let owner: Browser, editor: Browser, viewer: Browser, stranger: Browser, anon: Browser, id: string;
  beforeAll(async () => {
    owner = await w.signedIn(ada);
    id = await owner.newDoc('Shared notes');
    await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'Bob@Example.org', role: 'editor', notify: false } });
    await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'cy@example.org', role: 'viewer', notify: false } });
    editor = await w.signedIn(bob);
    viewer = await w.signedIn(cy);
    stranger = await w.signedIn({ sub: 'g-dee', email: 'dee@example.org', name: 'Dee' });
    anon = w.browser();
  });

  it('gives each person exactly their role', async () => {
    const status = async (b: Browser, path: string, init: RequestInit & { json?: unknown } = {}) => (await b.fetch(path, init)).status;
    const doc = `/api/docs/${id}`;
    // reading
    for (const [b, role] of [[owner, 'owner'], [editor, 'editor'], [viewer, 'viewer']] as const) {
      expect(((await (await b.fetch(doc)).json()) as { role: string }).role).toBe(role);
      expect(await status(b, `${doc}/snapshot`)).toBe(200);
    }
    for (const b of [stranger, anon]) {
      expect(await status(b, doc)).toBe(404);
      expect(await status(b, `${doc}/snapshot`)).toBe(404);
    }
    // only the owner sees the link settings
    expect('link' in ((await (await editor.fetch(doc)).json()) as object)).toBe(false);
    // writing
    const v = Number((await owner.fetch(`${doc}/snapshot`)).headers.get('x-doc-version'));
    expect(await status(viewer, `${doc}/snapshot`, { method: 'PUT', headers: { 'if-match': `"${v}"` }, body: new Uint8Array([9]) })).toBe(403);
    expect(await status(stranger, `${doc}/snapshot`, { method: 'PUT', headers: { 'if-match': `"${v}"` }, body: new Uint8Array([9]) })).toBe(404);
    expect(await status(editor, `${doc}/snapshot`, { method: 'PUT', headers: { 'if-match': `"${v}"` }, body: new Uint8Array([9]) })).toBe(200);
    expect(await status(viewer, doc, { method: 'PATCH', json: { name: 'mine' } })).toBe(403);
    expect(await status(editor, doc, { method: 'PATCH', json: { name: 'Shared notes' } })).toBe(200);
    // the owner's rights
    expect(await status(editor, doc, { method: 'PATCH', json: { link: 'viewer' } })).toBe(403);
    expect(await status(editor, `${doc}/shares`, { method: 'PUT', json: { email: 'x@example.org', role: 'editor' } })).toBe(403);
    expect(await status(editor, `${doc}/shares/cy%40example.org`, { method: 'DELETE' })).toBe(403);
    expect(await status(editor, doc, { method: 'DELETE' })).toBe(403);
    expect(await status(viewer, `${doc}/shares`)).toBe(403);
    expect(await status(editor, `${doc}/shares`)).toBe(200);
    expect(await status(viewer, `${doc}/files/a.dcw`, { method: 'PUT', body: 'x' })).toBe(403);
  });

  it('lists shared documents for their recipients, case-insensitively', async () => {
    const list = (await (await editor.fetch('/api/docs')).json()) as { owned: unknown[]; shared: { id: string; role: string; owner: { email: string } }[] };
    expect(list.shared.find((d) => d.id === id)).toMatchObject({ role: 'editor', owner: { email: 'ada@example.org' } });
    const shares = (await (await owner.fetch(`/api/docs/${id}/shares`)).json()) as { shares: { email: string; role: string }[] };
    expect(shares.shares.map((s) => `${s.email} ${s.role}`)).toEqual(['bob@example.org editor', 'cy@example.org viewer']);
  });

  it('works for someone who signs in only after being invited', async () => {
    const doc = await owner.newDoc('Later');
    await owner.fetch(`/api/docs/${doc}/shares`, { method: 'PUT', json: { email: 'eve@example.org', role: 'viewer', notify: false } });
    const eve = await w.signedIn({ sub: 'g-eve', email: 'eve@example.org', name: 'Eve' });
    expect(((await (await eve.fetch(`/api/docs/${doc}`)).json()) as { role: string }).role).toBe('viewer');
  });

  it('lets someone remove themselves, and the owner remove anyone', async () => {
    const doc = await owner.newDoc('Leaving');
    for (const e of ['bob@example.org', 'cy@example.org']) await owner.fetch(`/api/docs/${doc}/shares`, { method: 'PUT', json: { email: e, role: 'viewer', notify: false } });
    expect((await editor.fetch(`/api/docs/${doc}/shares/bob%40example.org`, { method: 'DELETE' })).status).toBe(204);
    expect((await editor.fetch(`/api/docs/${doc}`)).status).toBe(404);
    expect((await owner.fetch(`/api/docs/${doc}/shares/cy%40example.org`, { method: 'DELETE' })).status).toBe(204);
    expect((await viewer.fetch(`/api/docs/${doc}`)).status).toBe(404);
  });

  it('validates shares', async () => {
    const s = (json: unknown) => owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json });
    expect((await s({ email: 'not an address', role: 'viewer' })).status).toBe(400);
    expect((await s({ email: 'x@example.org', role: 'owner' })).status).toBe(400);
    expect((await s({ email: 'ada@example.org', role: 'viewer' })).status).toBe(400);
    expect((await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', body: '{"email":"x@example.org"}', headers: { 'content-type': 'text/plain' } })).status).toBe(415);
  });

  it('shares by link: view without an account, edit only signed in, and a new link after turning it off', async () => {
    const doc = await owner.newDoc('Linked', new Uint8Array([7, 7]));
    const on = (await (await owner.fetch(`/api/docs/${doc}`, { method: 'PATCH', json: { link: 'viewer' } })).json()) as { link: { role: string; token: string } };
    expect(on.link.role).toBe('viewer');
    const token = on.link.token;
    const withLink = { 'x-link-token': token };
    const snap = await anon.fetch(`/api/docs/${doc}/snapshot`, { headers: withLink });
    expect(snap.status).toBe(200);
    expect(snap.headers.get('x-doc-role')).toBe('viewer');
    expect(await body(snap)).toEqual(new Uint8Array([7, 7]));
    expect((await anon.fetch(`/api/docs/${doc}/snapshot`, { headers: { 'x-link-token': token.slice(1) + 'A' } })).status).toBe(404);
    expect((await stranger.fetch(`/api/docs/${doc}/snapshot`, { method: 'PUT', headers: { ...withLink, 'if-match': '"1"' }, body: new Uint8Array([1]) })).status).toBe(403);

    // editing by link: the same link, now with edit rights
    const ed = (await (await owner.fetch(`/api/docs/${doc}`, { method: 'PATCH', json: { link: 'editor' } })).json()) as { link: { token: string } };
    expect(ed.link.token).toBe(token);
    expect((await stranger.fetch(`/api/docs/${doc}/snapshot`, { method: 'PUT', headers: { ...withLink, 'if-match': '"1"' }, body: new Uint8Array([1]) })).status).toBe(200);
    expect((await anon.fetch(`/api/docs/${doc}/snapshot`, { method: 'PUT', headers: { ...withLink, 'if-match': '"2"' }, body: new Uint8Array([1]) })).status).toBe(401);
    // a link never lowers a role someone has, and link holders cannot manage sharing
    expect(((await (await viewer.fetch(`/api/docs/${doc}`, { headers: withLink })).json()) as { role: string }).role).toBe('editor');
    expect((await stranger.fetch(`/api/docs/${doc}`, { method: 'PATCH', headers: withLink, json: { link: 'off' } })).status).toBe(403);

    await owner.fetch(`/api/docs/${doc}`, { method: 'PATCH', json: { link: 'off' } });
    expect((await anon.fetch(`/api/docs/${doc}/snapshot`, { headers: withLink })).status).toBe(404);
    const again = (await (await owner.fetch(`/api/docs/${doc}`, { method: 'PATCH', json: { link: 'viewer' } })).json()) as { link: { token: string } };
    expect(again.link.token).not.toBe(token);
    expect((await owner.fetch(`/api/docs/${doc}`, { method: 'PATCH', json: { link: 'owner' } })).status).toBe(400);
  });
});

describe('share notifications', () => {
  it('e-mails a new person once, with everything inserted escaped', async () => {
    const owner = await w.signedIn({ sub: 'g-mal', email: 'mal@example.org', name: 'Mal <b>' });
    const id = await owner.newDoc('KLR <script>alert(1)</script>');
    const r = await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'new@example.org', role: 'editor' } });
    expect(await r.json()).toEqual({ email: 'new@example.org', role: 'editor', notified: true });
    expect(w.mails).toHaveLength(1);
    const m = w.mails[0];
    expect(m.from).toBe('Calculator <share@send.example.org>');
    expect(m.to).toEqual(['new@example.org']);
    expect(m.auth).toBe('Bearer re_test_key');
    expect(m.subject).toBe('Mal <b> shared “KLR <script>alert(1)</script>” with you');
    expect(m.text).toContain(`${ORIGIN}/#/s/${id}`);
    expect(m.text).toContain('You can edit it.');
    expect(m.html).not.toContain('<script>');
    expect(m.html).toContain('&#60;script&#62;');
    expect(m.html).toContain('Mal &#60;b&#62;');
    // changing the role, or sharing without notify, sends nothing
    await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'new@example.org', role: 'viewer' } });
    await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'quiet@example.org', role: 'viewer', notify: false } });
    expect(w.mails).toHaveLength(1);
  });

  it('reports a refused message and limits messages per day', async () => {
    const owner = await w.signedIn({ sub: 'g-spam', email: 'spam@example.org', name: 'S' });
    const id = await owner.newDoc();
    w.resendFails = true;
    const r = await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'r0@example.org', role: 'viewer' } });
    expect(((await r.json()) as { notified: boolean }).notified).toBe(false);
    w.resendFails = false;
    const limit = LIMITS.mailsPerDay;
    LIMITS.mailsPerDay = 3;
    try {
      let sent = 0;
      for (let i = 1; i <= 5; i++) {
        const x = await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: `r${i}@example.org`, role: 'viewer' } });
        if (((await x.json()) as { notified: boolean }).notified) sent++;
      }
      expect(sent).toBe(3);
      w.clock += 86400_000 + 1;
      const later = await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'later@example.org', role: 'viewer' } });
      expect(((await later.json()) as { notified: boolean }).notified).toBe(true);
    } finally {
      LIMITS.mailsPerDay = limit;
    }
  });

  it('sends nothing when e-mail is not configured', async () => {
    const quiet = await World.create({ email: false });
    const owner = await quiet.signedIn(ada);
    const id = await owner.newDoc();
    const r = await owner.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'bob@example.org', role: 'viewer' } });
    expect(((await r.json()) as { notified: boolean }).notified).toBe(false);
    expect(quiet.mails).toHaveLength(0);
    await quiet.dispose();
  });
});

describe('deleting an account', () => {
  it('removes the user, sessions, documents, stored objects and shares to them', async () => {
    const zed = await w.signedIn({ sub: 'g-zed', email: 'zed@example.org', name: 'Zed' });
    const id = await zed.newDoc();
    const other = await w.signedIn({ sub: 'g-yan', email: 'yan@example.org', name: 'Yan' });
    const theirs = await other.newDoc();
    await other.fetch(`/api/docs/${theirs}/shares`, { method: 'PUT', json: { email: 'zed@example.org', role: 'viewer', notify: false } });
    expect((await zed.fetch('/api/me', { method: 'DELETE' })).status).toBe(204);
    expect(zed.jar.has(SESSION_COOKIE)).toBe(false);
    expect((await other.fetch(`/api/docs/${id}`)).status).toBe(404);
    expect((await w.FILES.list({ prefix: `docs/${id}/` })).objects).toHaveLength(0);
    expect((await w.DB.prepare("SELECT COUNT(*) AS n FROM shares WHERE email = 'zed@example.org'").first<{ n: number }>())!.n).toBe(0);
    expect((await w.DB.prepare("SELECT COUNT(*) AS n FROM users WHERE email = 'zed@example.org'").first<{ n: number }>())!.n).toBe(0);
  });
});

describe('logging', () => {
  const spies: ReturnType<typeof vi.spyOn>[] = [];
  afterEach(() => spies.splice(0).forEach((s) => s.mockRestore()));

  it('never writes user data to the log', async () => {
    const lines: string[] = [];
    for (const k of ['log', 'info', 'warn', 'error', 'debug'] as const) spies.push(vi.spyOn(console, k).mockImplementation((...a: unknown[]) => void lines.push(a.map(String).join(' '))));
    const b = await w.signedIn({ sub: 'g-log', email: 'secret.person@example.org', name: 'Secret Person' });
    const id = await b.newDoc('Private title');
    await b.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'other.secret@example.org', role: 'viewer' } });
    // an internal error is logged by route pattern and error kind only
    const broken = { ...w.env, DB: { ...w.DB, prepare: () => { throw new TypeError('secret.person@example.org'); } } };
    const r = await handle(new Request(`${ORIGIN}/api/docs/${id}`, { headers: { cookie: `${SESSION_COOKIE}=${b.jar.get(SESSION_COOKIE)}` } }), broken, w.deps);
    expect(r.status).toBe(500);
    expect(await r.json()).toEqual({ error: 'internal error' });
    expect(lines).toEqual(['api: GET docs/:id: TypeError']);
    expect(lines.join('\n')).not.toMatch(/secret|private|example\.org/i);
  });
});
