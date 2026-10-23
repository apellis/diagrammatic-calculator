import { expect, it } from 'vitest';
import * as Y from 'yjs';
import { Doc } from './model';

const copy = (d: Doc) => { const y = new Y.Doc(); Y.applyUpdate(y, Y.encodeStateAsUpdate(d.ydoc)); return new Doc(y); };
const sync = (a: Doc, b: Doc) => { const x = Y.encodeStateAsUpdate(a.ydoc), y = Y.encodeStateAsUpdate(b.ydoc); Y.applyUpdate(a.ydoc, y); Y.applyUpdate(b.ydoc, x); };
it('merges disconnected committed text edits and moves without replacing identity', () => {
  const a = new Doc(); const id = a.addCell('watch a + b'); a.addCell('watch c');
  const b = copy(a); const cell = a.cells.get(0); const text = cell.get('text');
  expect(text).toBeInstanceOf(Y.Text);
  a.updateCell(id, 'watch aa + b'); a.moveCell(id, 1);
  b.updateCell(id, 'watch a + bb'); sync(a, b);
  expect(a.list()).toEqual(b.list()); expect(a.list()[1].text).toBe('watch aa + bb');
  expect(a.cells.get(0)).toBe(cell); expect(cell.get('text')).toBe(text);
  a.undo(); expect(a.list()[0].id).toBe(id);
  a.undo(); expect(a.list()[0].text).toBe('watch a + bb');
});
it('shares serializable proposals and attributes frozen results', () => {
  const a = new Doc(); a.setIdentity('alice');
  a.propose({ id: 'p1', author: 'alice', time: 1, title: 'Edit', payload: { cell: 'w1', text: 'watch b' }, status: 'pending' });
  a.pin({ name: 'result', cellId: 'w1', cellText: 'watch a', context: 'x', version: 1, key: 'result', value: null, time: 1 });
  const b = copy(a);
  expect(b.proposalList()).toEqual(a.proposalList());
  expect(b.snapshotList()[0].author).toBe('alice');
  expect(() => a.propose({ id: 'x', author: 'alice', time: 1, title: 'bad', payload: (() => {}) as never, status: 'pending' })).toThrow('invalid proposal');
});

it('keeps recovery and branches private and scopes origins per session', () => {
  const a = new Doc(); a.addCell('watch a'); a.createBranch('private');
  const b = copy(a);
  expect(b.branchList()).toEqual([]); expect(b.versions()).toEqual([]);
  expect(a.ydoc.share.has('history')).toBe(false); expect(a.ydoc.share.has('branches')).toBe(false);
  expect(a.ydoc.gc).toBe(true);
  a.setIdentity('alice'); b.setIdentity('bob');
  expect(a.author).not.toBe(b.author);
  b.addCell('watch b'); sync(a, b); a.undo();
  expect(a.list().map(c => c.text)).toContain('watch b');
});
