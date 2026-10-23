// Tests of the limits and protections added after the security review of 2026-10-23: storage
// quotas and early rejection of large bodies, share notifications that cannot be used for phishing
// or spam, what link holders see, shares bound to accounts, the configured origin, session idle
// timeout, rotation and sign-out everywhere, save races and resumable deletion.
import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from './auth';
import { access, getSnapshot, LIMITS, sweep } from './docs';
import { defang } from './mail';
import { clearKeyCache } from './oidc';
import { ORIGIN, World, type Browser } from './testing';
import type { ReadyEnv } from './env';

let w: World;
beforeAll(async () => {
  w = await World.create();
});
afterAll(() => w.dispose());

const saved = { ...LIMITS };
afterEach(() => {
  Object.assign(LIMITS, saved);
  w.mails = [];
  w.resendFails = false;
});

let n = 0;
/** A fresh person, signed in, whose account is `ageDays` old. */
async function person(name = 'P', ageDays = 2): Promise<Browser> {
  const k = ++n;
  const b = await w.signedIn({ sub: `g-h${k}`, email: `h${k}@example.org`, name });
  await w.DB.prepare('UPDATE users SET created = created - ? WHERE google_sub = ?').bind(ageDays * 86400_000, `g-h${k}`).run();
  return b;
}
const json = async <T = Record<string, unknown>>(r: Response) => (await r.json()) as T;
const share = (b: Browser, id: string, email: string, extra: Record<string, unknown> = {}) =>
  b.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email, role: 'viewer', ...extra } });

describe('storage', () => {
  it('caps the files kept with one document', async () => {
    LIMITS.filesPerDoc = 5;
    const a = await person();
    const id = await a.newDoc();
    const codes: number[] = [];
    for (let i = 0; i < 7; i++) codes.push((await a.fetch(`/api/docs/${id}/files/f${i}.dcw`, { method: 'PUT', body: 'x'.repeat(100) })).status);
    expect(codes).toEqual([201, 201, 201, 201, 201, 409, 409]);
    // replacing a file that is there is still allowed
    expect((await a.fetch(`/api/docs/${id}/files/f0.dcw`, { method: 'PUT', body: 'y' })).status).toBe(201);
  });

  it("charges every write, by the owner or a link editor, to the owner's storage", async () => {
    LIMITS.ownerBytes = 1000;
    const a = await person();
    const id = await a.newDoc('Q', new Uint8Array(400));
    const link = await json<{ link: { token: string } }>(await a.fetch(`/api/docs/${id}`, { method: 'PATCH', json: { link: 'editor' } }));
    const stranger = await person();
    const h = { 'x-link-token': link.link.token };
    expect((await stranger.fetch(`/api/docs/${id}/files/a.dcw`, { method: 'PUT', headers: h, body: 'x'.repeat(500) })).status).toBe(201);
    expect((await stranger.fetch(`/api/docs/${id}/files/b.dcw`, { method: 'PUT', headers: h, body: 'x'.repeat(200) })).status).toBe(413);
    const other = await a.newDoc('R', new Uint8Array(50));
    expect((await a.fetch(`/api/docs/${other}/snapshot`, { method: 'PUT', headers: { 'x-doc-generation': '1', 'if-match': '"1"' }, body: new Uint8Array(150) })).status).toBe(413);
    expect((await a.fetch(`/api/docs/${other}/snapshot`, { method: 'PUT', headers: { 'x-doc-generation': '1', 'if-match': '"1"' }, body: new Uint8Array(90) })).status).toBe(200);
  });

  it('refuses an oversized body from its Content-Length, before reading it', async () => {
    const a = await person();
    const id = await a.newDoc();
    let pulled = 0;
    const body = new ReadableStream<Uint8Array>({
      pull(c) {
        pulled++;
        c.enqueue(new Uint8Array(1024));
      },
    });
    const r = await a.fetch(`/api/docs/${id}/files/big.dcw`, { method: 'PUT', body, headers: { 'content-length': String(LIMITS.fileBytes + 1) } });
    expect(r.status).toBe(413);
    expect(pulled).toBeLessThanOrEqual(1);
  });

  it('stops reading a body without a length as soon as it is too large', async () => {
    LIMITS.snapshotBytes = 10_000;
    const a = await person();
    const id = await a.newDoc();
    let sent = 0;
    const body = new ReadableStream<Uint8Array>({
      pull(c) {
        sent += 1024;
        if (sent > 1_000_000) c.close();
        else c.enqueue(new Uint8Array(1024));
      },
    });
    const r = await a.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', body, headers: { 'x-doc-generation': '1', 'if-match': '"1"' } });
    expect(r.status).toBe(413);
    expect(sent).toBeLessThan(100_000);
  });

  it('finishes deleting stored objects in later requests when one runs out of rounds', async () => {
    const a = await person();
    const id = await a.newDoc();
    await a.fetch(`/api/docs/${id}/files/x.dcw`, { method: 'PUT', body: 'x' });
    LIMITS.deleteRounds = 0;
    expect((await a.fetch(`/api/docs/${id}`, { method: 'DELETE' })).status).toBe(204);
    expect((await a.fetch(`/api/docs/${id}`)).status).toBe(404);
    expect((await w.FILES.list({ prefix: `docs/${id}/` })).objects.length).toBeGreaterThan(0);
    LIMITS.deleteRounds = saved.deleteRounds;
    expect(await sweep(w.env as ReadyEnv)).toBe(true);
    expect((await w.FILES.list({ prefix: `docs/${id}/` })).objects).toHaveLength(0);
    expect((await w.DB.prepare('SELECT COUNT(*) AS n FROM pending_deletes').first<{ n: number }>())!.n).toBe(0);
  });

  it('deletes an account even when its objects cannot all be deleted at once', async () => {
    const a = await person();
    const id = await a.newDoc();
    LIMITS.deleteRounds = 0;
    expect((await a.fetch('/api/me', { method: 'DELETE' })).status).toBe(204);
    LIMITS.deleteRounds = saved.deleteRounds;
    // the next document anyone creates finishes the work
    await (await person()).newDoc();
    expect((await w.FILES.list({ prefix: `docs/${id}/` })).objects).toHaveLength(0);
  });
});

describe('share notifications', () => {
  it('name the inviter by their verified address and keep link-like text out', async () => {
    const eve = await person('Google Security Team');
    const id = await eve.newDoc('Your account is suspended - verify at evil.example.com within 24h');
    expect(await json(await share(eve, id, 'victim@example.net'))).toMatchObject({ notified: true });
    const m = w.mails[0];
    expect(m.subject).toBe('A document was shared with you on diagrammatic-calculator');
    expect(m.subject + m.text + m.html).not.toContain('evil.example.com');
    expect(m.text).toMatch(/^Google Security Team \(h\d+@example\.org\) shared a document with you/);
    expect(defang('see https://x.example/a, www.y.org or z.example.co.uk/p, not A2.x or Lemma 3.4')).toBe(
      'see [link removed] [link removed] or [link removed] not A2.x or Lemma 3.4',
    );
  });

  it('are not sent from accounts younger than a day', async () => {
    const a = await person('New', 0);
    const id = await a.newDoc();
    expect(await json(await share(a, id, 'someone@example.net'))).toMatchObject({ notified: false, mail: 'new-account' });
    expect(w.mails).toHaveLength(0);
  });

  it('are limited per recipient and in all, and the limit holds for concurrent shares', async () => {
    const senders = await Promise.all([person(), person(), person(), person()]);
    const docs = await Promise.all(senders.map((s) => s.newDoc()));
    const sent = async (i: number, to: string) => (await json<{ notified: boolean }>(await share(senders[i], docs[i], to))).notified;
    expect([await sent(0, 'target@example.net'), await sent(1, 'target@example.net'), await sent(2, 'target@example.net'), await sent(3, 'target@example.net')]).toEqual([
      true,
      true,
      true,
      false,
    ]);
    LIMITS.mailsPerDay = 3;
    const results = await Promise.all([0, 1, 2, 3, 4, 5].map((k) => sent(3, `c${k}@example.net`)));
    expect(results.filter(Boolean)).toHaveLength(3);
    const everyone = (await w.DB.prepare('SELECT COUNT(*) AS n FROM sent_mail').first<{ n: number }>())!.n;
    LIMITS.mailsGlobalPerDay = everyone;
    expect(await sent(0, 'one-more@example.net')).toBe(false);
  });

  it('stop for a recipient who unsubscribes, by the link or one-click', async () => {
    const a = await person();
    const id = await a.newDoc();
    await share(a, id, 'quits@example.net');
    const url = new URL(w.mails[0].headers!['List-Unsubscribe'].slice(1, -1));
    const anon = w.browser();
    const page = await anon.fetch(url.pathname + url.search);
    expect(page.status).toBe(200);
    expect(page.headers.get('content-security-policy')).toContain("default-src 'none'");
    expect(await page.text()).toContain('<form method="post">');
    // a GET changes nothing: still notified
    const id2 = await a.newDoc();
    expect(await json(await share(a, id2, 'quits@example.net'))).toMatchObject({ notified: true });
    // RFC 8058 one-click: a POST with no session and no CSRF token
    const r = await anon.fetch(url.pathname + url.search, { method: 'POST', body: 'List-Unsubscribe=One-Click', headers: { 'content-type': 'application/x-www-form-urlencoded', origin: 'null' } });
    expect(r.status).toBe(200);
    const id3 = await a.newDoc();
    expect(await json(await share(a, id3, 'quits@example.net'))).toMatchObject({ notified: false, mail: 'opted-out' });
    expect((await anon.fetch('/api/mail/unsubscribe?r=nonsense')).status).toBe(400);
  });
});

describe('what people see of each other', () => {
  it("hides the owner's address and the people list from those who have only the link", async () => {
    const a = await person('Owner');
    const id = await a.newDoc('secret project');
    await share(a, id, 'carol@example.org', { notify: false });
    const link = await json<{ link: { token: string } }>(await a.fetch(`/api/docs/${id}`, { method: 'PATCH', json: { link: 'editor' } }));
    const h = { 'x-link-token': link.link.token };
    const anonMeta = await json<{ owner: Record<string, string> }>(await w.browser().fetch(`/api/docs/${id}`, { headers: h }));
    expect(anonMeta.owner).toEqual({ name: 'Owner' });
    const stranger = await person();
    expect((await stranger.fetch(`/api/docs/${id}/shares`, { headers: h })).status).toBe(403);
    // an editor it is shared with still sees both
    const ed = await person();
    await a.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: (await ed.refresh()).user!.email, role: 'editor', notify: false } });
    expect((await ed.fetch(`/api/docs/${id}/shares`)).status).toBe(200);
    expect((await json<{ owner: Record<string, string> }>(await ed.fetch(`/api/docs/${id}`))).owner.email).toMatch(/@example\.org$/);
  });

  it('binds a share to the first account that uses it', async () => {
    const a = await person();
    const id = await a.newDoc();
    await share(a, id, 'reused@example.org', { notify: false });
    const first = await w.signedIn({ sub: 'g-first', email: 'reused@example.org', name: 'First' });
    expect((await first.fetch(`/api/docs/${id}`)).status).toBe(200);
    // later, another Google account presents the same address
    const second = await w.signedIn({ sub: 'g-second', email: 'reused@example.org', name: 'Second' });
    expect((await second.fetch(`/api/docs/${id}`)).status).toBe(404);
    expect((await json<{ shared: { id: string }[] }>(await second.fetch('/api/docs'))).shared.map((d) => d.id)).not.toContain(id);
    const people = await json<{ shares: { email: string; account: { name: string } | null }[] }>(await a.fetch(`/api/docs/${id}/shares`));
    expect(people.shares).toEqual([expect.objectContaining({ email: 'reused@example.org', account: { name: 'First', email: 'reused@example.org' } })]);
  });
});

describe('a configured origin', () => {
  it('is used for the sign-in redirect and e-mail links, and other hosts get no API', async () => {
    const site = 'https://calc.example.net';
    const v = await World.create({ appOrigin: site });
    try {
      const b = v.browser();
      b.origin = site;
      const login = await b.fetch('/api/auth/login');
      expect(new URL(login.headers.get('location')!).searchParams.get('redirect_uri')).toBe(`${site}/api/auth/callback`);
      const old = v.browser(); // an old deployment's own address
      expect((await old.fetch('/api/me')).status).toBe(421);
      expect((await old.fetch('/api/auth/login')).status).toBe(421);
      clearKeyCache(); // this world's Google has its own key under the same key ID
      await b.signIn({ sub: 'g-o', email: 'o@example.org', name: 'O' });
      await v.DB.prepare('UPDATE users SET created = 0').run();
      const id = await b.newDoc();
      await b.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'p@example.org', role: 'viewer' } });
      expect(v.mails[0].text).toContain(`${site}/#/s/${id}`);
      expect(v.mails[0].text).not.toContain(ORIGIN);
    } finally {
      clearKeyCache();
      await v.dispose();
    }
  });
});

describe('sessions', () => {
  it('end after seven days without a request, and requests keep them alive', async () => {
    const a = await person();
    const b = await person();
    for (let d = 0; d < 4; d++) {
      w.clock += 2 * 86400_000;
      expect((await a.refresh()).user).not.toBeNull();
    }
    expect((await b.refresh()).user).toBeNull();
  });

  it('get a fresh ID after a day; the old one works only briefly', async () => {
    const a = await person();
    const before = a.jar.get(SESSION_COOKIE)!;
    w.clock += 86400_000 + 1;
    const r = await a.fetch('/api/me');
    const set = r.headers.getSetCookie().find((c) => c.startsWith(SESSION_COOKIE));
    expect(set).toBeDefined();
    const me = await json<{ csrf: string; user: unknown }>(r);
    a.csrf = me.csrf;
    expect(a.jar.get(SESSION_COOKIE)).not.toBe(before);
    // the new session and its CSRF token work
    expect((await a.fetch('/api/docs', { method: 'POST', json: { name: 'after' } })).status).toBe(201);
    const old = w.browser();
    old.jar.set(SESSION_COOKIE, before);
    expect((await old.refresh()).user).not.toBeNull();
    w.clock += 120_000;
    expect((await old.refresh()).user).toBeNull();
    expect((await a.refresh()).user).not.toBeNull();
  });

  it('can all be ended at once', async () => {
    const a = await w.signedIn({ sub: 'g-many', email: 'many@example.org', name: 'M' });
    const elsewhere = await w.signedIn({ sub: 'g-many', email: 'many@example.org', name: 'M' });
    expect((await a.fetch('/api/auth/logout-all', { method: 'POST' })).status).toBe(204);
    expect((await a.refresh()).user).toBeNull();
    expect((await elsewhere.refresh()).user).toBeNull();
  });
});

describe('save races', () => {
  it('reads the snapshot again when a save replaced it after the row was read', async () => {
    const a = await person();
    const id = await a.newDoc('S', new Uint8Array([1]));
    const me = (await a.refresh()).user!;
    const session = { id: '', user: me, created: 0, expires: 0 };
    const stale = await access(w.env as ReadyEnv, id, session, null);
    expect((await a.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', headers: { 'x-doc-generation': '1', 'if-match': '"1"' }, body: new Uint8Array([2]) })).status).toBe(200);
    await w.DB.prepare('UPDATE documents SET generation = 2 WHERE id = ?').bind(id).run();
    const r = await getSnapshot(w.env as ReadyEnv, stale);
    expect(r.headers.get('x-doc-generation')).toBe('2');
    expect(r.headers.get('x-doc-version')).toBe('2');
    expect(new Uint8Array(await r.arrayBuffer())).toEqual(new Uint8Array([2]));
  });

  it('delete the snapshot of exactly the version they replace, and sweep leftovers', async () => {
    const a = await person();
    const id = await a.newDoc('T', new Uint8Array([1]));
    // a snapshot left behind by an interrupted save
    await w.FILES.put(`docs/${id}/snapshots/leftover`, new Uint8Array([9]));
    w.clock += 3600_000;
    for (let v = 1; v < 32; v++) {
      expect((await a.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', headers: { 'x-doc-generation': '1', 'if-match': `"${v}"` }, body: new Uint8Array([v]) })).status).toBe(200);
    }
    const left = (await w.FILES.list({ prefix: `docs/${id}/snapshots/` })).objects.map((o) => o.key);
    expect(left).toHaveLength(1);
    expect(left[0]).not.toContain('leftover');
  });
});
