import 'fake-indexeddb/auto';
import { expect, it, vi } from 'vitest';
import * as Y from 'yjs';
import { Doc } from './model';
import { createDoc, openDoc, deleteDoc, createFromCloud, updateInfo, replaceLocalDoc } from './store';
vi.mock('../app', () => ({ engine: { call: vi.fn().mockRejectedValue(new Error('use file fallback')) } }));
import { exportDch, parseDch, fillFromHistory } from './dcw';

it('delete clears every intermediate local generation and private database', async () => {
  vi.stubGlobal('navigator', { storage: {} });
  const id = await createDoc('Old', d => d.addCell('old secret'));
  for (let n = 1; n <= 3; n++) {
    const d = new Doc(); d.addCell(`secret ${n}`); d.createBranch('private');
    await replaceLocalDoc(id, d);
  }
  expect((await indexedDB.databases()).some(d => d.name === `diagcalc-doc-${id}-v1`)).toBe(true);
  await deleteDoc(id);
  expect((await indexedDB.databases()).filter(d => d.name?.includes(id))).toEqual([]);
});

it('saves and reopens private checkpoints and branches without sharing them', async () => {
  vi.stubGlobal('navigator', { storage: {} });
  const id = await createDoc('Recovery', d => d.addCell('watch a'));
  const first = await openDoc(id);
  const v = first.doc.checkpoint('retained');
  const branch = first.doc.createBranch('private experiment');
  const scratch = first.doc.openBranch(branch.id)!;
  scratch.updateCell(scratch.list()[0].id, 'watch private');
  first.doc.saveBranch(branch.id, scratch);
  first.doc.updateCell(first.doc.list()[0].id, 'watch b');
  await first.flush(); first.close();
  const reopened = await openDoc(id);
  expect(reopened.doc.list()[0].text).toBe('watch b');
  expect(reopened.doc.at(v.n)?.cells[0].text).toBe('watch a');
  expect(reopened.doc.openBranch(branch.id)?.list()[0].text).toBe('watch private');
  const peer = new Doc(); Y.applyUpdate(peer.ydoc, Y.encodeStateAsUpdate(reopened.doc.ydoc));
  expect(peer.branchList()).toEqual([]); expect(peer.versions()).toEqual([]);
  const imported = new Doc(); fillFromHistory(imported, await parseDch(exportDch(reopened.doc)));
  expect(imported.list()).toEqual(reopened.doc.list());
  expect(imported.at(v.n)?.cells[0].text).toBe('watch a');
  reopened.close(); await deleteDoc(id);
});

it('persists a legacy local migration and reads old versions after reopen', async () => {
  vi.stubGlobal('navigator', { storage: {} });
  const y = new Y.Doc({ gc: false });
  const cell = new Y.Map<unknown>(); cell.set('id', 'w1'); cell.set('text', 'watch old');
  y.getArray('cells').push([cell]);
  const snapshot = Y.encodeSnapshot(Y.snapshot(y));
  y.getArray('history').push([{ n: 1, author: 'you', note: 'old', time: 1, snapshot }]);
  cell.set('text', 'watch new');
  const id = await createFromCloud(Y.encodeStateAsUpdate(y), { id: 'fixture', account: 'owner', role: 'owner', version: 0 }, 'Legacy', 'fixture');
  await updateInfo(id, { cloud: undefined });
  const first = await openDoc(id);
  expect(first.info.localGeneration).toBe(1);
  expect(first.doc.at(1)?.cells[0].text).toBe('watch old');
  await first.flush(); first.close();
  const next = await openDoc(id);
  expect(next.doc.ydoc.gc).toBe(true);
  expect(next.doc.at(1)?.cells[0].text).toBe('watch old');
  expect(next.doc.list()[0].text).toBe('watch new');
  next.close(); await deleteDoc(id);
});

it('migrates legacy history and personal branches into a fresh GC-enabled document', () => {
  const y = new Y.Doc({ gc: false });
  const m = new Y.Map<unknown>(); m.set('id', 'w1'); m.set('text', 'watch old');
  y.getArray('cells').push([m]);
  const snapshot = Y.encodeSnapshot(Y.snapshot(y));
  y.getArray('history').push([{ n: 1, author: 'you', note: 'old', time: 1, snapshot }]);
  m.set('text', 'watch new');
  const branchDoc = new Doc(); branchDoc.addCell('watch branch');
  y.getMap('branches').set('b1', { id: 'b1', name: 'legacy', author: 'you', created: 1, base: { context: 'x', cells: [], hints: {} }, state: Y.encodeStateAsUpdate(branchDoc.ydoc) });
  const migrated = new Doc(y).migrateLegacy();
  expect(migrated.ydoc.gc).toBe(true);
  expect(migrated.list()[0].text).toBe('watch new');
  expect(migrated.at(1)?.cells[0].text).toBe('watch old');
  expect(migrated.openBranch('b1')?.list()[0].text).toBe('watch branch');
  expect(migrated.ydoc.share.has('history')).toBe(false);
  expect(migrated.ydoc.share.has('branches')).toBe(false);
  expect(migrated.privateDoc.getMap('migration').get('legacyUpdate')).toBeInstanceOf(Uint8Array);
});
