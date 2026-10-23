// A stand-in for the accounts API (web/server/) in the browser tests: the same routes and rules
// (roles, CSRF token, versioned saves), in memory, shared by every page of a test so several
// browsers can be several devices and several people.
import { MockRoom } from './live-fixture';
import { createHash } from 'node:crypto';
import * as Y from 'yjs';
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
  generation: number;
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
  versions = new Map<string, Map<string, { id: string; time: number; author: string; note: string; size: number; data: Buffer }>>();
  branches = new Map<string, Map<string, { id: string; name: string; updated: number; size: number; data: Buffer }>>();
  checkpoint(id: string, key: string, author: string, note: string, data = this.docs.get(id)!.data) {
    const versions = this.versions.get(id) ?? new Map();
    versions.set(key, { id: key, time: Date.now(), author, note, size: data.length, data: Buffer.from(data) });
    this.versions.set(id, versions);
  }
  shares = new Map<string, Map<string, 'viewer' | 'editor'>>();
  users = new Map<string, User>();
  log: Logged[] = [];
  mails: { to: string; doc: string }[] = [];
  private n = 0;
  readonly rooms = new Map<string, MockRoom>();
  private tickets = new Map<string, { doc: string; session: Session; user: User | null; link: string | null }>();
  private ticketNumber = 0;
  constructor(readonly enableLive = false) {}

  room(id: string): MockRoom {
    let room = this.rooms.get(id);
    if (!room) {
      const doc = this.docs.get(id)!;
      room = new MockRoom(doc.data, (data) => { doc.data = data; });
      this.rooms.set(id, room);
    }
    return room;
  }

  revoke(id: string, user: User) {
    this.shares.get(id)?.delete(user.email);
    this.rooms.get(id)?.disconnect(user, 4003);
  }

  dispose() {
    for (const room of this.rooms.values()) room.dispose();
  }

  user(email: string, name: string): User {
    const u = { id: `user-${email.split('@')[0]}`, email, name };
    this.users.set(u.id, u);
    return u;
  }

  /** Serves /api/ to every page of a browser context, as `session.user`. */
  async attach(context: BrowserContext | Page, session: Session) {
    if (this.enableLive) await context.routeWebSocket(/\/api\/docs\/[\w-]+\/live\?/, (socket) => {
      const url = new URL(socket.url());
      const ticket = url.searchParams.get('ticket') ?? '';
      const grant = this.tickets.get(ticket);
      this.tickets.delete(ticket);
      const id = url.pathname.split('/')[3];
      const doc = this.docs.get(id);
      if (!grant || grant.doc !== id || grant.session !== session || grant.user?.id !== session.user?.id) return socket.close({ code: 4001 });
      const role = doc && this.role(doc, session.user, grant.link);
      if (!role) return socket.close({ code: 4003 });
      this.room(id).connect(socket, session.user, role);
    });
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
      id: d.id, name: d.name, created: d.created, edited: d.edited, version: d.version, generation: d.generation, size: d.data.length, role,
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

    if (path === 'me' && method === 'GET') return json({ accounts: true, live: this.enableLive, user: u, ...(u ? { csrf: `csrf-${u.id}` } : {}) });
    if (path === 'auth/login') {
      session.user = session.next;
      return route.fulfill({ status: 302, headers: { location: url.searchParams.get('return') ?? '/' } });
    }
    const anonymousTicket = !u && method === 'POST' && /^docs\/[\w-]{16}\/live-ticket$/.test(path) && !!h['x-link-token'];
    if (method !== 'GET' && !anonymousTicket) {
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
      this.docs.set(id, { id, owner: u!.id, name: (req.postDataJSON() as { name: string }).name, created: now, edited: now, version: 0, generation: 1, data: Buffer.alloc(0), link: null, files: new Map() });
      return json(this.meta(this.docs.get(id)!, 'owner'), 201);
    }
    const m = /^docs\/([\w-]{16})(?:\/(snapshot|shares|files|live-ticket|proposals|versions|branches|migrate)(?:\/(.+))?)?$/.exec(path);
    const d = m && this.docs.get(m[1]);
    const role = d ? this.role(d, u, h['x-link-token'] ?? null) : null;
    if (!d || !role) return fail(404, 'no such document');
    const [, , sub, name] = m!;
    const atLeast = (r: 'editor' | 'owner') => (r === 'owner' ? role === 'owner' : role !== 'viewer');
    if (sub === 'versions' || sub === 'branches') {
      const key = name && decodeURIComponent(name);
      if (key && !/^[A-Za-z0-9_-]{1,100}$/.test(key)) return fail(400, 'invalid version or branch id');
      if (sub === 'branches' && !u) return fail(401, 'sign in first');
      const scope = `${d.id}:${u?.id}`;
      if (sub === 'branches' && !this.branches.has(scope)) this.branches.set(scope, new Map());
      const records = sub === 'versions' ? this.versions.get(d.id) : this.branches.get(scope);
      if (method === 'GET') {
        if (!key) return json({ [sub]: [...(records?.values() ?? [])].sort((a, b) =>
          ('time' in b ? b.time : b.updated) - ('time' in a ? a.time : a.updated) || b.id.localeCompare(a.id)
        ).map(({ data: _data, ...meta }) => meta) });
        const record = records?.get(key);
        return record ? route.fulfill({ contentType: 'application/octet-stream', body: record.data }) : fail(404, 'no such version or branch');
      }
      if (sub === 'branches' && key && method === 'PUT') {
        const data = req.postDataBuffer() ?? Buffer.alloc(0);
        if (!data.length) return fail(400, 'empty branch');
        let title: string;
        try { title = decodeURIComponent(h['x-branch-name'] ?? h['x-doc-name'] ?? key).replace(/[\u0000-\u001f\u007f]/g, ' ').trim().slice(0, 200) || 'Untitled'; }
        catch { return fail(400, 'bad X-Branch-Name'); }
        const record = { id: key, name: title, updated: Date.now(), size: data.length, data };
        this.branches.get(scope)!.set(key, record);
        const { data: _data, ...meta } = record;
        return json(meta, 201);
      }
      if (sub === 'branches' && key && method === 'DELETE') {
        this.branches.get(scope)!.delete(key);
        return route.fulfill({ status: 204 });
      }
      return fail(405, 'method not allowed');
    }
    if (sub === 'migrate' && method === 'POST') {
      if (role !== 'owner') return fail(403, 'only the owner can do this');
      if (Number(h['x-doc-generation']) !== d.generation) return fail(409, 'generation mismatch', { generation: d.generation });
      const data = req.postDataBuffer();
      if (!data?.length) return fail(400, 'empty snapshot');
      const staged = new Y.Doc();
      try { Y.applyUpdate(staged, data); } catch { return fail(400, 'invalid document update'); } finally { staged.destroy(); }
      this.checkpoint(d.id, `before-migration-${d.generation}`, u!.id, 'before migration');
      this.rooms.get(d.id)?.dispose(); this.rooms.delete(d.id);
      d.data = data; d.generation++; d.version++; d.edited = Date.now();
      return json({ version: d.version, generation: d.generation });
    }
    if (sub === 'proposals' && name?.endsWith('/decision') && method === 'POST') {
      if (!this.enableLive) return fail(503, 'live unavailable');
      if (!atLeast('editor')) return fail(403, 'view only');
      const room = this.room(d.id), key = decodeURIComponent(name.slice(0, -'/decision'.length));
      const record = room.doc.getMap<{ status: string }>('proposals').get(key);
      if (record?.status !== 'pending') return fail(409, 'proposal already decided');
      if (h['x-proposal-base'] !== createHash('sha256').update(Y.encodeStateAsUpdate(room.doc)).digest('hex')) return fail(409, 'document changed');
      const bytes = new Uint8Array(req.postDataBuffer()!);
      const next = new Y.Doc(); Y.applyUpdate(next, Y.encodeStateAsUpdate(room.doc)); Y.applyUpdate(next, bytes);
      const status = next.getMap<{ status: string }>('proposals').get(key)?.status; next.destroy();
      if (!['accepted', 'rejected'].includes(status ?? '') || status !== h['x-proposal-state']) return fail(400, 'invalid decision');
      Y.applyUpdate(room.doc, bytes);
      return json({ saved: true });
    }
    if (sub === 'live-ticket' && method === 'POST') {
      if (!this.enableLive) return fail(503, 'live unavailable');
      if (d.generation === 0) return fail(409, 'owner migration required', { generation: 0 });
      const ticket = `ticket-${++this.ticketNumber}`;
      this.tickets.set(ticket, { doc: d.id, session, user: u, link: h['x-link-token'] ?? null });
      return json({ ticket, role, generation: d.generation });
    }
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
      if (method === 'GET') return route.fulfill({ status: 200, body: d.data, headers: { 'content-type': 'application/octet-stream', 'x-doc-version': String(d.version), 'x-doc-role': role, 'x-doc-generation': String(d.generation) } });
      if (!atLeast('editor')) return fail(403, 'you can only view this document');
      if (this.enableLive && d.generation === 0) return fail(409, 'owner migration required', { generation: 0 });
      if (h['x-doc-generation'] !== undefined && Number(h['x-doc-generation']) !== d.generation) return fail(409, 'generation mismatch', { generation: d.generation });
      if (Number((h['if-match'] ?? '').replace(/"/g, '')) !== d.version) return fail(409, 'changed', { version: d.version });
      d.data = req.postDataBuffer() ?? Buffer.alloc(0);
      d.version++;
      d.edited = Date.now();
      if (h['x-doc-name']) d.name = decodeURIComponent(h['x-doc-name']);
      return json({ version: d.version, generation: d.generation });
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
