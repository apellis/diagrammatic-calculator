import 'fake-indexeddb/auto';
import { expect, it, vi } from 'vitest';
import * as Y from 'yjs';
import { Doc } from '../doc/model';
import { createFromCloud, openDoc, getInfo, listDocs } from '../doc/store';
import { Api, ApiError } from './api';
import { recoverCloud, encodeBranch, decodeBranch, PersonalBranches, cloudCheckpoint } from './recovery';
vi.stubGlobal('navigator', { storage: {} });

function personalApi(api: Api) {
  const branches = new Map<string, Uint8Array>();
  vi.spyOn(api, 'branches').mockResolvedValue({ branches: [] });
  vi.spyOn(api, 'putBranch').mockImplementation(async (_id, key, bytes) => { branches.set(key, bytes); });
  vi.spyOn(api, 'branch').mockImplementation(async (_id, key) => {
    const bytes = branches.get(key); if (!bytes) throw new ApiError(404, 'missing'); return bytes;
  });
  return branches;
}

async function legacy() {
  const y = new Y.Doc({ gc: false });
  const c = new Y.Map(); c.set('id', 'w1'); c.set('text', 'server'); y.getArray('cells').push([c]);
  const branch = new Doc(); branch.addCell('secret');
  y.getMap('branches').set('b1', { id: 'b1', name: 'private', author: 'owner', created: 1, base: branch.inputs(), state: Y.encodeStateAsUpdate(branch.ydoc) });
  const data = Y.encodeStateAsUpdate(y);
  const id = await createFromCloud(data, { id: 'cloud', account: 'owner', role: 'owner', version: 1, generation: 0 }, 'Old', null);
  const o = await openDoc(id);
  o.doc.addCell('unsynced');
  o.doc.checkpoint('keep me');
  return { o, data };
}

it('owner migration preserves detached source, private recovery, and legacy personal branches', async () => {
  const { o, data } = await legacy();
  const api = new Api(); let current = { data, version: 1, generation: 0, role: 'owner' as const };
  const uploads = personalApi(api);
  vi.spyOn(api, 'snapshot').mockImplementation(async () => current);
  vi.spyOn(api, 'migrate').mockImplementation(async (_id, update) => { current = { data: update, version: 2, generation: 1, role: 'owner' }; return current; });
  const result = await recoverCloud(o, api, 'migrate', 'owner');
  expect(uploads.size).toBe(1);
  expect(decodeBranch([...uploads.values()][0]).name).toBe('private');
  const fresh = await openDoc(result.id);
  expect(fresh.doc.list().map(c => c.text)).toContain('unsynced');
  expect(fresh.doc.branch('b1')?.state).toBeInstanceOf(Uint8Array);
  expect(fresh.doc.versions().some(v => v.note === 'keep me')).toBe(true);
  expect((await getInfo(o.info.id))?.cloud).toBeUndefined();
  expect((await getInfo(o.info.id))?.localOnly).toBe(true);
  expect(fresh.info.localOnly).toBe(false);
  expect(o.doc.list().map(c => c.text)).toContain('unsynced');
  const peer = new Doc(); Y.applyUpdate(peer.ydoc, Y.encodeStateAsUpdate(fresh.doc.ydoc));
  expect(peer.branchList()).toEqual([]); expect(peer.versions()).toEqual([]);
  o.close(); fresh.close();
});

it.each([401, 403, 409])('failed migration (%s) keeps original and durable prepared recovery', async status => {
  const { o, data } = await legacy(); const api = new Api(); personalApi(api);
  const before = new Set((await listDocs()).map(d => d.id));
  vi.spyOn(api, 'snapshot').mockResolvedValue({ data, version: 1, generation: 0, role: 'owner' });
  vi.spyOn(api, 'migrate').mockRejectedValue(new ApiError(status, 'access or generation changed'));
  await expect(recoverCloud(o, api, 'migrate', 'owner')).rejects.toThrow('access or generation changed');
  const preparedInfo = (await listDocs()).find(d => !before.has(d.id))!;
  expect(preparedInfo.cloud).toBeUndefined();
  expect(preparedInfo.localOnly).toBe(true);
  const prepared = await openDoc(preparedInfo.id);
  expect(prepared.doc.list().map(c => c.text)).toContain('unsynced');
  expect(prepared.doc.branch('b1')).toBeDefined(); prepared.close();
  expect((await getInfo(o.info.id))?.cloud?.id).toBe('cloud');
  expect(o.doc.list().map(c => c.text)).toContain('unsynced'); o.close();
});

it('reopen never merges stale local state into a new generation', async () => {
  const { o } = await legacy(); const server = new Doc(); server.addCell('current'); const api = new Api();
  vi.spyOn(api, 'snapshot').mockResolvedValue({ data: Y.encodeStateAsUpdate(server.ydoc), version: 4, generation: 2, role: 'editor' });
  const { id } = await recoverCloud(o, api, 'reopen', 'owner'); const fresh = await openDoc(id);
  expect(fresh.doc.list().map(c => c.text)).toEqual(['current']);
  expect(o.doc.list().map(c => c.text)).toContain('unsynced');
  expect(fresh.doc.versions().some(v => v.note === 'keep me')).toBe(true);
  o.close(); fresh.close();
});

it('branch codec preserves bytes; divergent personal records fork instead of overwriting', async () => {
  const d = new Doc(); d.addCell('a'); const b = d.createBranch('mine');
  expect(decodeBranch(encodeBranch(b))).toEqual(b);
  const remote = { ...b, name: 'remote' }; const api = new Api(); const uploads = new Map<string, Uint8Array>();
  vi.spyOn(api, 'branches').mockResolvedValue({ branches: [{ id: 'remote-key', name: 'remote', updated: 1, size: 1 }] });
  vi.spyOn(api, 'branch').mockImplementation(async (_id, key) => uploads.get(key) ?? encodeBranch(remote));
  vi.spyOn(api, 'putBranch').mockImplementation(async (_id, key, bytes) => { uploads.set(key, bytes); });
  const sync = new PersonalBranches(d, api, 'cloud', 'owner'); await sync.sync();
  expect(d.branch(b.id)?.name).toBe('mine');
  expect(d.branchList().some(b => b.name.includes('remote'))).toBe(true);
  expect(uploads.size).toBeGreaterThan(0);
  const peer = new Doc(); Y.applyUpdate(peer.ydoc, Y.encodeStateAsUpdate(d.ydoc)); expect(peer.branchList()).toEqual([]);
  sync.stop();
});

it('checkpoint preview is detached and does not mutate live content', async () => {
  const live = new Doc(); live.addCell('live');
  const old = new Doc(); old.addCell('checkpoint'); old.createBranch('not shared');
  const api = new Api();
  vi.spyOn(api, 'version').mockResolvedValue(Y.encodeStateAsUpdate(old.ydoc));
  const inputs = await cloudCheckpoint(api, 'cloud', 'v1', 'link-token');
  expect(inputs.cells[0].text).toBe('checkpoint');
  expect(live.list()[0].text).toBe('live');
  expect(api.version).toHaveBeenCalledWith('cloud', 'v1', 'link-token');
});

it('role changes and legacy reopen cannot import shared legacy branches as collaborator data', async () => {
  const { o, data } = await legacy(); const api = new Api();
  vi.spyOn(api, 'snapshot').mockResolvedValue({ data, version: 1, generation: 0, role: 'editor' });
  const migrate = vi.spyOn(api, 'migrate');
  await expect(recoverCloud(o, api, 'migrate', 'owner')).rejects.toThrow('Ownership');
  await expect(recoverCloud(o, api, 'reopen', 'owner')).rejects.toThrow('owner must upgrade');
  expect(migrate).not.toHaveBeenCalled();
  expect((await getInfo(o.info.id))?.cloud).toBeDefined(); o.close();
});

it('branch edits upload under fresh keys, verify before pruning, and stop without polling', async () => {
  const d = new Doc(); d.addCell('live'); const b = d.createBranch('one');
  const api = new Api(); const uploads = personalApi(api);
  vi.spyOn(api, 'deleteBranch').mockImplementation(async (_id, key) => { expect(uploads.size).toBe(2); uploads.delete(key); });
  const sync = new PersonalBranches(d, api, 'cloud', 'owner');
  await sync.sync(); const first = [...uploads.keys()][0];
  d.branches.set(b.id, { ...b, name: 'two' }); await sync.sync();
  expect(uploads.size).toBe(1); expect(uploads.has(first)).toBe(false);
  expect(decodeBranch([...uploads.values()][0]).name).toBe('two');
  sync.stop(); d.branches.set(b.id, { ...b, name: 'three' }); await sync.sync();
  expect(api.putBranch).toHaveBeenCalledTimes(2);
});
