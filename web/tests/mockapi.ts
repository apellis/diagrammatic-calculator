// A stand-in for the accounts API (web/server/) in the browser tests: the same routes and rules
// (roles, CSRF token, versioned saves), in memory, shared by every page of a test so several
// browsers can be several devices and several people.
import type { BrowserContext, Page, Request as PwRequest, Route } from '@playwright/test';

export interface User {
  id: string;
  email: string;
  name: string;
}

interface Doc {
  id: string;
  owner: string;
  name: string;
  created: number;
  edited: number;
  version: number;
  data: Buffer;
  link: { role: 'viewer' | 'editor'; token: string } | null;
  files: Map<string, string>;
}

export interface Logged {
  who: string | null;
  method: string;
  path: string;
  csrf: string | null;
  link: string | null;
}

/** One browser's session with the mock: who is signed in, and who signs in next. */
export interface Session {
  user: User | null;
  next: User | null;
}

export class MockAccounts {
  docs = new Map<string, Doc>();
  shares = new Map<string, Map<string, 'viewer' | 'editor'>>();
  users = new Map<string, User>();
  log: Logged[] = [];
  mails: { to: string; doc: string }[] = [];
  private n = 0;

  user(email: string, name: string): User {
    const u = { id: `user-${email.split('@')[0]}`, email, name };
    this.users.set(u.id, u);
    return u;
  }

  /** Serves /api/ to every page of a browser context, as `session.user`. */
  async attach(context: BrowserContext | Page, session: Session) {
    await context.route((url) => url.pathname.startsWith('/api/'), (route) => this.handle(route, session));
  }

  private role(d: Doc, u: User | null, link: string | null): 'owner' | 'editor' | 'viewer' | null {
    const rank = { viewer: 1, editor: 2, owner: 3 } as const;
    let r: 'owner' | 'editor' | 'viewer' | null = null;
    if (u && d.owner === u.id) r = 'owner';
    else if (u) r = this.shares.get(d.id)?.get(u.email) ?? null;
    if (link && d.link && link === d.link.token && (!r || rank[d.link.role] > rank[r])) r = d.link.role;
    return r;
  }

  private meta(d: Doc, role: string) {
    const o = this.users.get(d.owner)!;
    return {
      id: d.id, name: d.name, created: d.created, edited: d.edited, version: d.version, size: d.data.length, role,
      owner: { email: o.email, name: o.name },
      ...(role === 'owner' ? { link: d.link } : {}),
    };
  }

  private async handle(route: Route, session: Session) {
    const req: PwRequest = route.request();
    const url = new URL(req.url());
    const path = url.pathname.slice('/api/'.length);
    const method = req.method();
    const h = req.headers();
    const u = session.user;
    this.log.push({ who: u?.email ?? null, method, path, csrf: h['x-csrf-token'] ?? null, link: h['x-link-token'] ?? null });
    const json = (body: unknown, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
    const fail = (status: number, error: string, extra = {}) => json({ error, ...extra }, status);

    if (path === 'me' && method === 'GET') return json({ accounts: true, user: u, ...(u ? { csrf: `csrf-${u.id}` } : {}) });
    if (path === 'auth/login') {
      session.user = session.next;
      return route.fulfill({ status: 302, headers: { location: url.searchParams.get('return') ?? '/' } });
    }
    if (method !== 'GET') {
      if (!u) return fail(401, 'sign in first');
      if (h['x-csrf-token'] !== `csrf-${u.id}`) return fail(403, 'missing or wrong CSRF token');
    }
    if (path === 'auth/logout' && method === 'POST') {
      session.user = null;
      return route.fulfill({ status: 204 });
    }
    if (path === 'docs' && method === 'GET') {
      if (!u) return fail(401, 'sign in first');
      const all = [...this.docs.values()];
      return json({
        owned: all.filter((d) => d.owner === u.id).map((d) => this.meta(d, 'owner')),
        shared: all.filter((d) => d.owner !== u.id && this.shares.get(d.id)?.has(u.email)).map((d) => this.meta(d, this.shares.get(d.id)!.get(u.email)!)),
      });
    }
    if (path === 'docs' && method === 'POST') {
      const id = `doc${String(++this.n).padStart(13, '0')}`;
      const now = Date.now();
      this.docs.set(id, { id, owner: u!.id, name: (req.postDataJSON() as { name: string }).name, created: now, edited: now, version: 0, data: Buffer.alloc(0), link: null, files: new Map() });
      return json(this.meta(this.docs.get(id)!, 'owner'), 201);
    }
    const m = /^docs\/([\w-]{16})(?:\/(snapshot|shares|files)(?:\/(.+))?)?$/.exec(path);
    const d = m && this.docs.get(m[1]);
    const role = d ? this.role(d, u, h['x-link-token'] ?? null) : null;
    if (!d || !role) return fail(404, 'no such document');
    const [, , sub, name] = m!;
    const atLeast = (r: 'editor' | 'owner') => (r === 'owner' ? role === 'owner' : role !== 'viewer');
    if (!sub) {
      if (method === 'GET') return json(this.meta(d, role));
      if (method === 'PATCH') {
        const b = req.postDataJSON() as { link?: string };
        if (b.link !== undefined) {
          if (role !== 'owner') return fail(403, 'only the owner can do this');
          d.link = b.link === 'off' ? null : { role: b.link as 'viewer' | 'editor', token: d.link?.token ?? `token${d.id}` };
        }
        return json(this.meta(d, role));
      }
      if (method === 'DELETE') {
        if (role !== 'owner') return fail(403, 'only the owner can do this');
        this.docs.delete(d.id);
        return route.fulfill({ status: 204 });
      }
    }
    if (sub === 'snapshot') {
      if (method === 'GET') return route.fulfill({ status: 200, body: d.data, headers: { 'content-type': 'application/octet-stream', 'x-doc-version': String(d.version), 'x-doc-role': role } });
      if (!atLeast('editor')) return fail(403, 'you can only view this document');
      if (Number((h['if-match'] ?? '').replace(/"/g, '')) !== d.version) return fail(409, 'changed', { version: d.version });
      d.data = req.postDataBuffer() ?? Buffer.alloc(0);
      d.version++;
      d.edited = Date.now();
      if (h['x-doc-name']) d.name = decodeURIComponent(h['x-doc-name']);
      return json({ version: d.version });
    }
    if (sub === 'shares' && !name) {
      if (method === 'GET') {
        if (!atLeast('editor')) return fail(403, 'you can only view this document');
        const o = this.users.get(d.owner)!;
        return json({ owner: { email: o.email, name: o.name }, shares: [...(this.shares.get(d.id) ?? [])].map(([email, r]) => ({ email, role: r, created: 0 })) });
      }
      if (method === 'PUT') {
        if (role !== 'owner') return fail(403, 'only the owner can do this');
        const b = req.postDataJSON() as { email: string; role: 'viewer' | 'editor'; notify: boolean };
        const email = b.email.toLowerCase();
        if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) return fail(400, 'not an e-mail address');
        const s = this.shares.get(d.id) ?? new Map();
        const fresh = !s.has(email);
        s.set(email, b.role);
        this.shares.set(d.id, s);
        const notified = fresh && b.notify;
        if (notified) this.mails.push({ to: email, doc: d.id });
        return json({ email, role: b.role, notified });
      }
    }
    if (sub === 'shares' && name && method === 'DELETE') {
      this.shares.get(d.id)?.delete(decodeURIComponent(name));
      return route.fulfill({ status: 204 });
    }
    if (sub === 'files' && name && method === 'PUT') {
      if (!atLeast('editor')) return fail(403, 'you can only view this document');
      d.files.set(decodeURIComponent(name), req.postData() ?? '');
      return json({ name, size: 0 }, 201);
    }
    if (sub === 'files' && !name && method === 'GET') {
      return json({ files: [...d.files].sort(([a], [b]) => a.localeCompare(b)).map(([n, t]) => ({ name: n, size: Buffer.byteLength(t), uploaded: Date.now() })) });
    }
    if (sub === 'files' && name && method === 'GET') {
      const t = d.files.get(decodeURIComponent(name));
      return t === undefined ? fail(404, 'no such file') : route.fulfill({ status: 200, contentType: 'text/plain', body: t });
    }
    if (sub === 'files' && name && method === 'DELETE') {
      if (!atLeast('editor')) return fail(403, 'you can only view this document');
      d.files.delete(decodeURIComponent(name));
      return route.fulfill({ status: 204 });
    }
    return fail(405, 'method not allowed');
  }

  /** The one document the mock holds. */
  only(): Doc {
    const all = [...this.docs.values()];
    if (all.length !== 1) throw new Error(`expected one document, found ${all.length}`);
    return all[0];
  }
}
