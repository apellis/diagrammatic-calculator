import { afterAll, beforeAll, expect, it } from 'vitest';
import { World, ORIGIN, type Browser } from './testing';
let w: World;
let b: Browser;
let id: string;
const forwarded: Request[] = [];
beforeAll(async () => {
  w = await World.create();
  b = await w.signedIn({ sub: 'ticket-owner', email: 'ticket@example.org' });
  id = (await (await b.fetch('/api/docs', { method: 'POST', json: {} })).json()).id;
  Object.assign(w.env, { LIVE: { idFromName: (id: string) => id, get: () => ({ fetch: async (req: Request) => { forwarded.push(req); return Response.json({ forwarded: true }); } }) } });
});
afterAll(() => w.dispose());
it('issues a session-bound ticket and forwards only trusted identity', async () => {
  const r = await b.fetch(`/api/docs/${id}/live-ticket`, { method: 'POST' });
  expect(r.status).toBe(200);
  const { ticket, generation } = await r.json();
  expect(generation).toBe(1);
  const live = `/api/docs/${id}/live?ticket=${encodeURIComponent(ticket)}`;
  expect((await b.fetch(live, { headers: { origin: ORIGIN, upgrade: 'websocket', 'x-dc-identity': 'spoof' } })).status).toBe(200);
  const req = forwarded.at(-1)!;
  expect(new URL(req.url).pathname).toBe('/live');
  expect(req.headers.get('x-dc-doc')).toBe(id);
  expect(JSON.parse(req.headers.get('x-dc-identity')!)).toMatchObject({ generation: 1, link: null });
  expect(req.headers.has('cookie')).toBe(false);
  expect((await w.browser().fetch(live, { headers: { origin: ORIGIN, upgrade: 'websocket' } })).status).toBe(401);
  expect((await b.fetch(live, { headers: { origin: 'https://evil.example', upgrade: 'websocket' } })).status).toBe(403);
  w.clock += 60_001;
  expect((await b.fetch(live, { headers: { origin: ORIGIN, upgrade: 'websocket' } })).status).toBe(401);
});
it('allows only same-origin anonymous link tickets, rechecks link revocation, and hides link tokens', async () => {
  const patch = await b.fetch(`/api/docs/${id}`, { method: 'PATCH', json: { link: 'viewer' } });
  const token = (await patch.json()).link.token;
  const anon = w.browser();
  const r = await anon.fetch(`/api/docs/${id}/live-ticket`, { method: 'POST', headers: { 'x-link-token': token } });
  expect(r.status).toBe(200);
  const { ticket, role } = await r.json();
  expect(role).toBe('viewer');
  expect(ticket).not.toContain(token);
  expect((await anon.fetch(`/api/docs/${id}/live-ticket`, { method: 'POST', headers: { 'x-link-token': token, origin: 'https://evil.example' } })).status).toBe(403);
  expect((await anon.fetch(`/api/docs/${id}/live?ticket=${ticket}`, { headers: { origin: ORIGIN, upgrade: 'websocket' } })).status).toBe(200);
  expect(JSON.parse(forwarded.at(-1)!.headers.get('x-dc-identity')!)).toMatchObject({ session: null, link: token });
  await b.fetch(`/api/docs/${id}`, { method: 'PATCH', json: { link: 'off' } });
  expect(new URL(forwarded.at(-1)!.url).pathname).toBe('/internal/revoke');
  expect((await anon.fetch(`/api/docs/${id}/live?ticket=${ticket}`, { headers: { origin: ORIGIN, upgrade: 'websocket' } })).status).toBe(404);
});
it('fences legacy and missing-generation writes and fails closed without LIVE', async () => {
  expect((await b.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', body: new Uint8Array([1]) })).status).toBe(428);
  await w.DB.prepare('UPDATE documents SET generation = 0 WHERE id = ?').bind(id).run();
  expect((await b.fetch(`/api/docs/${id}/live-ticket`, { method: 'POST' })).status).toBe(409);
  expect((await b.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', headers: { 'x-doc-generation': '0' }, body: new Uint8Array([1]) })).status).toBe(409);
  await w.DB.prepare('UPDATE documents SET generation = 1 WHERE id = ?').bind(id).run();
  const live = w.env.LIVE;
  delete w.env.LIVE;
  expect((await b.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', headers: { 'x-doc-generation': '1' }, body: new Uint8Array([1]) })).status).toBe(503);
  w.env.LIVE = live;
});
it('notifies active rooms after session deletion, without trusting stale sockets', async () => {
  const user = (await b.refresh()).user!.id;
  await w.DB.prepare('INSERT INTO live_sockets(doc_id,socket_id,user_id,since) VALUES (?,?,?,?)').bind(id, 'socket-1', user, w.clock).run();
  const ticket = (await (await b.fetch(`/api/docs/${id}/live-ticket`, { method: 'POST' })).json()).ticket;
  expect((await b.fetch('/api/auth/logout-all', { method: 'POST' })).status).toBe(204);
  expect(new URL(forwarded.at(-1)!.url).pathname).toBe('/internal/revoke');
  expect((await w.DB.prepare('SELECT COUNT(*) n FROM sessions WHERE user_id = ?').bind(user).first<{ n: number }>())!.n).toBe(0);
  expect((await b.fetch(`/api/docs/${id}/live?ticket=${ticket}`, { headers: { origin: ORIGIN, upgrade: 'websocket' } })).status).toBe(401);
  await b.signIn({ sub: 'ticket-owner', email: 'ticket@example.org' });
});

it('rejects viewer writes and migration and emits share/document revocations', async () => {
  const viewer = await w.signedIn({ sub: 'ticket-viewer', email: 'viewer@example.org' });
  expect((await b.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'viewer@example.org', role: 'viewer', notify: false } })).status).toBe(200);
  expect(new URL(forwarded.at(-1)!.url).pathname).toBe('/internal/revoke');
  for (const [path, method] of [['snapshot', 'PUT'], ['migrate', 'POST']]) {
    expect((await viewer.fetch(`/api/docs/${id}/${path}`, { method, headers: { 'x-doc-generation': '1' }, body: new Uint8Array([1]) })).status).toBe(403);
  }
  const ticket = (await (await viewer.fetch(`/api/docs/${id}/live-ticket`, { method: 'POST' })).json()).ticket;
  expect((await b.fetch(`/api/docs/${id}/shares/viewer%40example.org`, { method: 'DELETE' })).status).toBe(204);
  expect(new URL(forwarded.at(-1)!.url).pathname).toBe('/internal/revoke');
  expect((await viewer.fetch(`/api/docs/${id}/live?ticket=${ticket}`, { headers: { origin: ORIGIN, upgrade: 'websocket' } })).status).toBe(404);
  const other = (await (await b.fetch('/api/docs', { method: 'POST', json: {} })).json()).id;
  expect((await b.fetch(`/api/docs/${other}`, { method: 'DELETE' })).status).toBe(204);
  expect(forwarded.at(-1)!.headers.get('x-dc-doc')).toBe(other);
  expect(await w.DB.prepare('SELECT id FROM documents WHERE id = ?').bind(other).first()).toBe(null);
  const csrf = b.csrf;
  b.csrf = '';
  expect((await b.fetch(`/api/docs/${id}/live-ticket`, { method: 'POST' })).status).toBe(403);
  b.csrf = csrf;
});

it('forwards snapshot and owner migration with generation fencing and no R2 side write', async () => {
  const r = await b.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', headers: { 'x-doc-generation': '1' }, body: new Uint8Array([1, 2]) });
  expect(r.status).toBe(200);
  expect(new URL(forwarded.at(-1)!.url).pathname).toBe('/internal/snapshot');
  expect((await w.FILES.list({ prefix: `docs/${id}/` })).objects).toHaveLength(0);
  expect((await b.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', headers: { 'x-doc-generation': '0' }, body: new Uint8Array([1]) })).status).toBe(409);
  expect((await b.fetch(`/api/docs/${id}/migrate`, { method: 'POST', headers: { 'x-doc-generation': '1' }, body: new Uint8Array([1]) })).status).toBe(200);
  expect(new URL(forwarded.at(-1)!.url).pathname).toBe('/internal/migrate');
});
