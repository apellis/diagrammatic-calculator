import { afterAll, beforeAll, expect, it } from 'vitest';
import { World, type Browser } from './testing';
import { LIMITS, ownerBytes } from './docs';
import { pruneVersions } from './versions';
import type { ReadyEnv } from './env';
let w: World;
let a: Browser;
let b: Browser;
let id: string;
let au: string;
let bu: string;
beforeAll(async () => {
  w = await World.create();
  a = await w.signedIn({ sub: 'branch-a', email: 'ba@example.org' });
  b = await w.signedIn({ sub: 'branch-b', email: 'bb@example.org' });
  au = (await a.refresh()).user!.id;
  bu = (await b.refresh()).user!.id;
  id = (await (await a.fetch('/api/docs', { method: 'POST', json: {} })).json()).id;
  await a.fetch(`/api/docs/${id}/shares`, { method: 'PUT', json: { email: 'bb@example.org', role: 'viewer', notify: false } });
});
afterAll(() => w.dispose());
const put = (browser: Browser, branch = 'same', bytes = [1, 2]) => browser.fetch(`/api/docs/${id}/branches/${branch}`, { method: 'PUT', body: new Uint8Array(bytes), headers: { 'x-doc-name': encodeURIComponent('What if?') } });
it('keeps branches personal including for viewers, with account quota and replacement cleanup', async () => {
  expect((await put(a)).status).toBe(201);
  expect((await (await a.fetch(`/api/docs/${id}/branches`)).json()).branches[0].name).toBe('What if?');
  expect((await (await b.fetch(`/api/docs/${id}/branches`)).json()).branches).toEqual([]);
  expect((await b.fetch(`/api/docs/${id}/branches/same`)).status).toBe(404);
  expect((await put(b, 'same', [3])).status).toBe(201);
  expect(await ownerBytes(w.env as ReadyEnv, au)).toBe(2);
  expect(await ownerBytes(w.env as ReadyEnv, bu)).toBe(1);
  expect((await put(a, 'same', [4, 5, 6])).status).toBe(201);
  expect(new Uint8Array(await (await b.fetch(`/api/docs/${id}/branches/same`)).arrayBuffer())).toEqual(new Uint8Array([3]));
  expect((await w.FILES.list({ prefix: `docs/${id}/branches/${au}/` })).objects).toHaveLength(1);
  expect((await a.fetch(`/api/docs/${id}/branches/same`, { method: 'DELETE' })).status).toBe(204);
  expect((await w.FILES.list({ prefix: `docs/${id}/branches/${au}/` })).objects).toHaveLength(0);
  expect((await b.fetch(`/api/docs/${id}/branches/same`)).status).toBe(200);
});
it('enforces personal byte quota atomically and does not leave rejected objects', async () => {
  await w.DB.prepare('UPDATE documents SET size = ? WHERE id = ?').bind(LIMITS.ownerBytes - 1, id).run();
  const results = await Promise.all([put(a, 'q1', [1]), put(a, 'q2', [2])]);
  expect(results.map(r => r.status).sort()).toEqual([201, 409]);
  expect((await w.FILES.list({ prefix: `docs/${id}/branches/${au}/` })).objects).toHaveLength(1);
  await w.DB.prepare('UPDATE documents SET size = 0 WHERE id = ?').bind(id).run();
});
it('lists checkpoints without object keys; authorized viewers can read, others cannot', async () => {
  const key = `docs/${id}/versions/checkpoint`;
  await w.FILES.put(key, new Uint8Array([9]));
  await w.DB.prepare('INSERT INTO doc_versions(id,doc_id,time,author,note,size,object_key) VALUES (?,?,?,?,?,?,?)').bind('checkpoint', id, w.clock, au, 'saved', 1, key).run();
  const r = await (await b.fetch(`/api/docs/${id}/versions`)).json();
  expect(r.versions).toEqual([{ id: 'checkpoint', time: w.clock, author: au, note: 'saved', size: 1 }]);
  expect(new Uint8Array(await (await b.fetch(`/api/docs/${id}/versions/checkpoint`)).arrayBuffer())).toEqual(new Uint8Array([9]));
  expect((await w.browser().fetch(`/api/docs/${id}/versions/checkpoint`)).status).toBe(404);
});
it('retains latest 50 and daily checkpoints for 30 days and cleans deleted R2 objects', async () => {
  for (let i = 0; i < 55; i++) {
    const key = `docs/${id}/versions/v${i}`;
    await w.FILES.put(key, new Uint8Array([i]));
    await w.DB.prepare('INSERT INTO doc_versions VALUES (?,?,?,?,?,?,?)').bind(`v${i}`, id, w.clock - i * 1000 - 1000, au, '', 1, key).run();
  }
  for (const [version, ago] of [['daily', 86_400_000], ['same-day', 86_401_000], ['expired', 31 * 86_400_000]] as const) {
    const key = `docs/${id}/versions/${version}`;
    await w.FILES.put(key, new Uint8Array([1]));
    await w.DB.prepare('INSERT INTO doc_versions VALUES (?,?,?,?,?,?,?)').bind(version, id, w.clock - ago, au, '', 1, key).run();
  }
  await pruneVersions(w.env as ReadyEnv, id, w.clock);
  expect((await w.DB.prepare('SELECT COUNT(*) n FROM doc_versions WHERE doc_id = ?').bind(id).first<{ n: number }>())!.n).toBe(51);
  expect((await a.fetch(`/api/docs/${id}/versions/daily`)).status).toBe(200);
  expect((await a.fetch(`/api/docs/${id}/versions/same-day`)).status).toBe(404);
  expect((await a.fetch(`/api/docs/${id}/versions/expired`)).status).toBe(404);
  // pending deletion sweep is bounded, later API requests finish the rest.
  const { sweep } = await import('./docs');
  await sweep(w.env as ReadyEnv);
  expect((await w.FILES.list({ prefix: `docs/${id}/versions/` })).objects).toHaveLength(51);
});
it('account deletion cleans personal branch objects on another owner document', async () => {
  expect((await b.fetch('/api/me', { method: 'DELETE' })).status).toBe(204);
  expect((await w.FILES.list({ prefix: `docs/${id}/branches/${bu}/` })).objects).toHaveLength(0);
  expect((await a.fetch(`/api/docs/${id}`)).status).toBe(200);
});
