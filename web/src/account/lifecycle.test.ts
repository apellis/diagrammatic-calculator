import 'fake-indexeddb/auto';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import * as Y from 'yjs';
import { IndexeddbPersistence } from 'y-indexeddb';
import { api, loadAccount, openFromAccount, signOut, syncAll, uploadOnce } from './account';
import { type CloudDoc, type User } from './api';
import { Doc } from '../doc/model';
import { createDoc, deleteDoc, getInfo, listDocs, openDoc, replaceLocalDoc, updateInfo, type CloudLink } from '../doc/store';

const user: User = { id: 'alice', email: 'alice@example.test', name: 'Alice' };
const deletionMark = (y: Y.Doc) => JSON.stringify([...Y.decodeUpdate(Y.encodeStateAsUpdate(y)).ds.clients].sort(([a], [b]) => a - b));
const meta = (id = 'remote'): CloudDoc => ({ id, name: 'Current', role: 'owner', version: 1, generation: 1, created: 1, edited: 1, size: 1 });
const link = (y: Y.Doc): CloudLink => ({ id: 'remote', account: user.id, role: 'owner', version: 1, generation: 1, sv: Y.encodeStateVector(y), deletions: deletionMark(y), synced: Date.now() + 10000 });

beforeEach(async () => {
  vi.stubGlobal('navigator', { storage: {} });
  vi.spyOn(api, 'me').mockResolvedValue({ accounts: true, user });
  vi.spyOn(api, 'logout').mockResolvedValue();
  await loadAccount(true);
});
afterEach(async () => {
  vi.restoreAllMocks();
  for (const d of await listDocs()) await deleteDoc(d.id);
  vi.unstubAllGlobals();
});

async function stored(id: string, generation: number, fn: (y: Y.Doc) => unknown | Promise<unknown>) {
  const y = new Y.Doc({ gc: false });
  const p = new IndexeddbPersistence(`diagcalc-doc-${id}${generation ? `-v${generation}` : ''}`, y);
  await p.whenSynced;
  try { await fn(y); await p.set('flushed', 1); }
  finally { await p.destroy(); y.destroy(); }
}

it('keeps recovery local through initial upload, list sync and reopening', async () => {
  const id = await createDoc('Recovery', d => d.addCell('private'), { localOnly: true });
  const create = vi.spyOn(api, 'create');
  vi.spyOn(api, 'list').mockResolvedValue({ owned: [], shared: [] });
  const o = await openDoc(id);
  await expect(uploadOnce(id, o.doc.ydoc, o.doc.name, user)).rejects.toThrow('local-only');
  expect(await syncAll(user)).toEqual({ uploaded: 0, downloaded: 0, updated: 0, failed: 0 });
  expect(create).not.toHaveBeenCalled(); expect((await getInfo(id))?.cloud).toBeUndefined();
  o.close();
  expect(await signOut(true)).toEqual({ removed: 0, kept: 1 });
});

it('preserves synced documents with private branches but clears checkpoint-only history', async () => {
  const branchId = await createDoc('Private', d => { d.addCell('main'); d.createBranch('unsent'); });
  const historyId = await createDoc('History', d => { d.addCell('main'); d.checkpoint('local checkpoint'); });
  for (const id of [branchId, historyId]) await stored(id, 0, y => updateInfo(id, { cloud: link(y) }));
  vi.spyOn(api, 'list').mockResolvedValue({ owned: [meta()], shared: [] });
  expect(await signOut(true)).toEqual({ removed: 1, kept: 1 });
  const o = await openDoc(branchId); expect(o.doc.branchList()[0].name).toBe('unsent'); o.close();
  expect(await getInfo(historyId)).toBeUndefined();
});

it('uploads the active local generation, never the legacy database or private sidecar', async () => {
  const id = await createDoc('Legacy', d => d.addCell('watch old'));
  const fresh = new Doc(); fresh.meta.set('name', 'Current'); fresh.addCell('watch current');
  const branch = fresh.createBranch('private draft');
  await replaceLocalDoc(id, fresh);
  vi.spyOn(api, 'list').mockResolvedValue({ owned: [], shared: [] });
  vi.spyOn(api, 'create').mockResolvedValue(meta());
  const save = vi.spyOn(api, 'save').mockResolvedValue({ version: 2, generation: 1 });
  expect((await syncAll(user)).uploaded).toBe(1);
  const received = new Doc(); Y.applyUpdate(received.ydoc, save.mock.calls[0][1]);
  expect(received.list()[0].text).toBe('watch current');
  expect(received.branchList()).toEqual([]);
  const reopened = await openDoc(id);
  expect(reopened.doc.branch(branch.id)?.name).toBe('private draft');
  reopened.close();
});

it('syncs deletion-only active-generation edits even when the edited timestamp did not advance', async () => {
  const id = await createDoc('Legacy');
  const fresh = new Doc(); fresh.meta.set('name', 'Current'); fresh.ydoc.getText('probe').insert(0, 'delete me');
  await replaceLocalDoc(id, fresh, link(fresh.ydoc));
  await stored(id, 1, y => y.getText('probe').delete(0, 9));
  vi.spyOn(api, 'list').mockResolvedValue({ owned: [meta()], shared: [] });
  const save = vi.spyOn(api, 'save').mockResolvedValue({ version: 2, generation: 1 });
  await syncAll(user);
  expect(save).toHaveBeenCalledTimes(1);
  const remote = new Y.Doc(); Y.applyUpdate(remote, Y.encodeStateAsUpdate(fresh.ydoc));
  Y.applyUpdate(remote, save.mock.calls[0][1]);
  expect(remote.getText('probe').toString()).toBe('');
  expect((await getInfo(id))?.cloud?.version).toBe(2);
});

it.each(['owner', 'viewer'] as const)('keeps offline deletion-only %s edits on clear sign-out', async role => {
  const id = await createDoc('Current', d => d.ydoc.getText('probe').insert(0, 'x'));
  await stored(id, 0, y => updateInfo(id, { cloud: { ...link(y), role } }));
  await stored(id, 0, y => y.getText('probe').delete(0, 1));
  vi.spyOn(api, 'list').mockRejectedValue(new TypeError('offline'));
  expect(await signOut(true)).toEqual({ removed: 0, kept: 1 });
  expect(await getInfo(id)).toBeDefined();
  await stored(id, 0, y => expect(y.getText('probe').toString()).toBe(''));
});

it('keeps deletion-only edits with an unknown legacy deletion baseline', async () => {
  const id = await createDoc('Current', d => d.ydoc.getText('probe').insert(0, 'x'));
  await stored(id, 0, y => updateInfo(id, { cloud: { ...link(y), deletions: undefined } }));
  await stored(id, 0, y => y.getText('probe').delete(0, 1));
  vi.spyOn(api, 'list').mockRejectedValue(new TypeError('offline'));
  expect(await signOut(true)).toEqual({ removed: 0, kept: 1 });
});

it('clears a confirmed synced active generation without touching another account', async () => {
  const id = await createDoc('Legacy');
  const fresh = new Doc(); fresh.meta.set('name', 'Current'); fresh.addCell('watch current');
  await replaceLocalDoc(id, fresh, link(fresh.ydoc));
  const other = await createDoc('Other');
  await updateInfo(other, { cloud: { ...link(fresh.ydoc), id: 'other', account: 'bob' } });
  vi.spyOn(api, 'list').mockResolvedValue({ owned: [meta()], shared: [] });
  expect(await signOut(true)).toEqual({ removed: 1, kept: 0 });
  expect(await getInfo(id)).toBeUndefined();
  expect((await getInfo(other))?.cloud?.account).toBe('bob');
});

it('preserves a generation-fenced copy on clear sign-out instead of trusting its stale baseline', async () => {
  const id = await createDoc('Old generation');
  await stored(id, 0, y => updateInfo(id, { cloud: link(y) }));
  vi.spyOn(api, 'list').mockResolvedValue({ owned: [{ ...meta(), generation: 2, version: 2 }], shared: [] });
  expect(await signOut(true)).toEqual({ removed: 0, kept: 1 });
  expect(await getInfo(id)).toBeDefined();
});

it('records snapshot generation and deletion baseline when opening from an account', async () => {
  const y = new Doc(); y.meta.set('name', 'Current'); y.ydoc.getText('probe').insert(0, 'x'); y.ydoc.getText('probe').delete(0, 1);
  vi.spyOn(api, 'snapshot').mockResolvedValue({ data: Y.encodeStateAsUpdate(y.ydoc), version: 3, generation: 2, role: 'owner' });
  const id = await openFromAccount(user, meta());
  expect((await getInfo(id))?.cloud).toMatchObject({ generation: 2, deletions: deletionMark(y.ydoc) });
});
