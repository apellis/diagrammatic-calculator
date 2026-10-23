import { describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { Api } from './api';
import { ahead, syncDoc, upload } from './sync';
import { Doc } from '../doc/model';

const settle = () => new Promise((r) => setTimeout(r, 0));
const user = { id: 'u1', email: 'ada@example.org', name: 'Ada' };

/** An in-memory accounts API with the server's saving rules (version check, 409). */
function fakeServer(role: 'owner' | 'editor' | 'viewer' = 'owner') {
  const docs = new Map<string, { version: number; data: Uint8Array; name: string }>();
  const log: string[] = [];
  let next = 1;
  const f: typeof fetch = async (input, init) => {
    const url = new URL(String(input), 'https://app.example');
    const method = init?.method ?? 'GET';
    const h = new Headers(init?.headers);
    log.push(`${method} ${url.pathname}`);
    const json = (b: unknown, status = 200) => new Response(JSON.stringify(b), { status, headers: { 'content-type': 'application/json' } });
    if (url.pathname === '/api/docs' && method === 'POST') {
      const id = `doc${String(next++).padStart(13, '0')}`;
      docs.set(id, { version: 0, data: new Uint8Array(), name: JSON.parse(String(init!.body)).name });
      return json({ id, version: 0, role: 'owner', name: docs.get(id)!.name }, 201);
    }
    const m = /^\/api\/docs\/([^/]+)(\/snapshot)?$/.exec(url.pathname);
    const d = m && docs.get(m[1]);
    if (!d) return json({ error: 'no such document' }, 404);
    if (!m![2]) return json({ id: m![1], version: d.version, role, name: d.name });
    if (method === 'GET') return new Response(d.data.slice(), { headers: { 'x-doc-version': String(d.version), 'x-doc-role': role } });
    if (role === 'viewer') return json({ error: 'you can only view this document' }, 403);
    if (h.get('x-csrf-token') !== 'csrf') return json({ error: 'csrf' }, 403);
    const expected = Number(h.get('if-match')!.replace(/"/g, ''));
    if (expected !== d.version) return json({ error: 'changed', version: d.version }, 409);
    d.data = new Uint8Array(await new Response(init!.body).arrayBuffer());
    d.version++;
    d.name = decodeURIComponent(h.get('x-doc-name') ?? d.name);
    return json({ version: d.version });
  };
  const api = new Api('/api', f);
  api.csrf = 'csrf';
  return { api, docs, log };
}

/** What a server copy holds, as cell texts. */
function cellsOf(data: Uint8Array): string[] {
  const y = new Y.Doc({ gc: false });
  Y.applyUpdate(y, data);
  return new Doc(y).list().map((c) => c.text);
}

describe('syncing a document with the account', () => {
  it('tells when a copy has updates another lacks', () => {
    const a = new Y.Doc();
    const before = Y.encodeStateVector(a);
    expect(ahead(Y.encodeStateVector(a), before)).toBe(false);
    a.getMap('m').set('k', 1);
    expect(ahead(Y.encodeStateVector(a), before)).toBe(true);
    expect(ahead(before, Y.encodeStateVector(a))).toBe(false);
    expect(ahead(Y.encodeStateVector(a), undefined)).toBe(true);
  });

  it('uploads a document made anonymously, exactly', async () => {
    const { api, docs } = fakeServer();
    const d = new Doc();
    d.rename('Idempotents');
    d.addCell('def e := psi_1 x_1 e(ii)');
    await settle();
    const link = await upload(api, d.ydoc, d.name, user);
    expect(link).toMatchObject({ account: 'u1', role: 'owner', version: 1 });
    const server = docs.get(link.id)!;
    expect(server.name).toBe('Idempotents');
    expect(cellsOf(server.data)).toEqual(['def e := psi_1 x_1 e(ii)']);
  });

  it('saves nothing when nothing changed, and only the new state when something did', async () => {
    const { api, log } = fakeServer();
    const d = new Doc();
    d.addCell('watch x_1');
    await settle();
    let link = await upload(api, d.ydoc, d.name, user);
    log.length = 0;
    link = await syncDoc(api, d.ydoc, link, d.name);
    expect(log).toEqual([`GET /api/docs/${link.id}`]);
    d.addCell('watch x_2');
    await settle();
    log.length = 0;
    link = await syncDoc(api, d.ydoc, link, d.name);
    expect(log).toEqual([`GET /api/docs/${link.id}`, `PUT /api/docs/${link.id}/snapshot`]);
    expect(link.version).toBe(2);
  });

  it('merges edits made on two devices without losing either', async () => {
    const { api, docs } = fakeServer();
    const a = new Doc();
    a.addCell('def a := x_1');
    await settle();
    let la = await upload(api, a.ydoc, a.name, user);
    // the second device starts from the account's copy
    const b = new Doc(new Y.Doc({ gc: false }));
    let lb = await syncDoc(api, b.ydoc, { ...la, version: 0, sv: undefined }, b.name);
    expect(b.list().map((c) => c.text)).toEqual(['def a := x_1']);
    // both edit offline
    a.addCell('watch a a');
    b.updateCell(b.list()[0].id, 'def a := x_2');
    await settle();
    la = await syncDoc(api, a.ydoc, la, a.name);
    // b still thinks the account is at its old version: its save is refused (409), so it merges and saves again
    lb = await syncDoc(api, b.ydoc, lb, b.name, { version: lb.version, role: 'owner' });
    la = await syncDoc(api, a.ydoc, la, a.name);
    const want = ['def a := x_2', 'watch a a'];
    expect(a.list().map((c) => c.text)).toEqual(want);
    expect(b.list().map((c) => c.text)).toEqual(want);
    expect(cellsOf(docs.get(la.id)!.data)).toEqual(want);
    expect(la.version).toBe(lb.version);
  });

  it('only reads as a viewer', async () => {
    const owner = fakeServer();
    const d = new Doc();
    d.addCell('watch x_1');
    await settle();
    const link = await upload(owner.api, d.ydoc, d.name, user);
    const asViewer = fakeServer('viewer');
    asViewer.docs.set(link.id, owner.docs.get(link.id)!);
    const v = new Doc(new Y.Doc({ gc: false }));
    const lv = await syncDoc(asViewer.api, v.ydoc, { ...link, role: 'viewer', version: 0, sv: undefined }, v.name);
    v.addCell('watch x_2');
    await settle();
    asViewer.log.length = 0;
    const again = await syncDoc(asViewer.api, v.ydoc, lv, v.name);
    expect(again.role).toBe('viewer');
    expect(asViewer.log.some((l) => l.startsWith('PUT'))).toBe(false);
    expect(cellsOf(owner.docs.get(link.id)!.data)).toEqual(['watch x_1']);
  });
});
