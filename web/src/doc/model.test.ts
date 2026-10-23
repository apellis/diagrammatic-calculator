import { describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { Doc, idNumber } from './model';

const settle = () => new Promise((r) => setTimeout(r, 0));

describe('document model', () => {
  it('records each step as a version and undoes one step at a time', async () => {
    const d = new Doc();
    d.setContext('nilhecke nu=2');
    const a = d.addCell('def a := psi_1 x_1');
    const w = d.addCell('watch a a');
    d.updateCell(a, 'def a := x_1 psi_1');
    await settle();
    expect(d.list().map((c) => c.text)).toEqual(['def a := x_1 psi_1', 'watch a a']);
    expect(d.versions().map((v) => v.note)).toEqual(['context nilhecke nu=2', 'add def a := psi_1 x_1', 'add watch a a', `edit ${a}`]);
    d.undo();
    await settle();
    expect(d.list()[0].text).toBe('def a := psi_1 x_1');
    expect(d.versions().at(-1)!.note).toBe('undo');
    d.redo();
    expect(d.list()[0].text).toBe('def a := x_1 psi_1');
    d.moveCell(w, 0);
    expect(d.list().map((c) => c.id)).toEqual([w, a]);
    d.removeCell(w);
    expect(d.list().map((c) => c.id)).toEqual([a]);
  });

  it('never reuses a cell ID after undo', () => {
    const d = new Doc();
    const a = d.addCell('watch x_1');
    d.undo();
    const b = d.addCell('watch x_1');
    expect(b).not.toBe(a);
  });

  it('gives cells two-letter IDs and keeps counting after old one-letter ones', () => {
    const d = new Doc();
    const texts = ['def a := x_1', 'watch a', 'solve a = 0 for c', 'check zero(a)', 'note """n"""', '# a remark'];
    expect(texts.map((t) => d.addCell(t))).toEqual(['df1', 'wt2', 'sv3', 'ck4', 'nt5', 'rm6']);
    d.replace('nilhecke nu=2', [{ id: 'd7', text: 'def a := x_1' }, { id: 'w12', text: 'watch a' }], 'restore an old file');
    expect(d.addCell('check zero(a)')).toBe('ck13');
    expect(d.list().map((c) => c.id)).toEqual(['d7', 'w12', 'ck13']);
    expect(idNumber('df12')).toBe(12);
    expect(idNumber('d12')).toBe(12);
    expect(idNumber('preview1x')).toBe(0);
  });

  it('opens old versions read-only', async () => {
    const d = new Doc();
    d.setContext('nilhecke nu=2');
    const a = d.addCell('watch x_1');
    await settle();
    d.updateCell(a, 'watch x_2');
    await settle();
    const v2 = d.at(2)!;
    expect(v2.cells).toEqual([{ id: a, text: 'watch x_1' }]);
    expect(d.list()[0].text).toBe('watch x_2');
    expect(d.toDcw(v2)).toBe(`diagcalc-workspace/1\ncontext nilhecke nu=2\nwatch x_1  # id=${a}\n`);
  });
});

describe('branches', () => {
  it('accepts a branch as one step, merging with changes made meanwhile', async () => {
    const d = new Doc();
    d.setContext('nilhecke nu=2');
    const a = d.addCell('def a := x_1');
    const b = d.addCell('def b := x_2');
    const w = d.addCell('watch a b');
    await settle();
    const br = d.createBranch('what if');
    const t = d.openBranch(br.id)!;
    t.updateCell(a, 'def a := x_1 psi_1');
    t.removeCell(w);
    const n = t.addCell('watch a a', 1);
    t.setHint(a, 'psi_1 x_1', { order: [2, 1] }, 'interchange');
    d.saveBranch(br.id, t);
    // meanwhile the document changes another cell, and one the branch also changed
    d.updateCell(b, 'def b := x_1 x_2');
    await settle();
    const before = d.versions().length;

    const reopened = d.openBranch(br.id)!;
    expect(reopened.list().map((c) => c.text)).toEqual(['def a := x_1 psi_1', 'watch a a', 'def b := x_2']);
    const cmp = d.compareBranch(br.id, reopened);
    expect(cmp.changed).toEqual([{ id: a, from: 'def a := x_1', to: 'def a := x_1 psi_1' }]);
    expect(cmp.removed.map((c) => c.id)).toEqual([w]);
    expect(cmp.added).toEqual([{ id: n, text: 'watch a a' }]);
    expect(cmp.hints).toBe(1);
    expect(cmp.conflicts).toEqual([]);

    d.acceptBranch(br.id, reopened);
    await settle();
    expect(d.list().map((c) => c.text)).toEqual(['def a := x_1 psi_1', 'watch a a', 'def b := x_1 x_2']);
    expect(d.hint(a, 'psi_1 x_1')).toEqual({ order: [2, 1] });
    expect(d.branches.size).toBe(0);
    expect(d.versions().length).toBe(before + 1);
    expect(d.versions().at(-1)!.note).toBe('accept branch what if');
    // accepting undoes as one step
    d.undo();
    expect(d.list().map((c) => c.text)).toEqual(['def a := x_1', 'def b := x_1 x_2', 'watch a b']);
  });

  it('reports a cell both sides changed, and renumbers a clashing new cell', async () => {
    const d = new Doc();
    const a = d.addCell('def a := x_1');
    const br = d.createBranch('x');
    const t = d.openBranch(br.id)!;
    t.updateCell(a, 'def a := x_2');
    const n = t.addCell('watch a');
    const m = d.addCell('watch a a');
    expect(m).toBe(n);
    d.updateCell(a, 'def a := x_3');
    const cmp = d.acceptBranch(br.id, t);
    expect(cmp.conflicts).toEqual([a]);
    const ids = d.list().map((c) => c.id);
    expect(new Set(ids).size).toBe(3);
    expect(d.list().map((c) => c.text)).toEqual(['def a := x_2', 'watch a', 'watch a a']);
  });

  it('makes an AI action one undo step with the AI as author', async () => {
    const d = new Doc();
    d.addCell('def a := x_1');
    await settle();
    await d.asAI(async () => {
      d.addCell('watch a');
      await settle();
      d.addCell('watch a a');
    });
    await settle();
    expect(d.versions().slice(-2).map((v) => v.author)).toEqual(['AI', 'AI']);
    d.undo();
    expect(d.list().map((c) => c.text)).toEqual(['def a := x_1']);
    d.addCell('watch x_1');
    d.undo();
    expect(d.list().length).toBe(1);
  });
});

describe('malformed shared content', () => {
  /** What a hostile or buggy editor can save: valid Yjs, but not the shape this app writes. */
  function hostile(): Uint8Array {
    const good = new Doc();
    good.addCell('def a := x_1');
    const evil = new Y.Doc({ gc: false });
    Y.applyUpdate(evil, Y.encodeStateAsUpdate(good.ydoc));
    (evil.getArray('cells') as Y.Array<unknown>).push(['not a map', 42]);
    const noId = new Y.Map<unknown>();
    noId.set('text', 'no id');
    (evil.getArray('cells') as Y.Array<unknown>).push([noId]);
    const badText = new Y.Map<unknown>();
    badText.set('id', 'd9');
    badText.set('text', { evil: true });
    (evil.getArray('cells') as Y.Array<unknown>).push([badText]);
    const meta = evil.getMap('meta');
    meta.set('name', { toString: 1 });
    meta.set('context', 7);
    meta.set('pinned', 'w1');
    meta.set('edited', 'yesterday');
    (evil.getArray('history') as Y.Array<unknown>).push([{ n: 99, author: 'AI', note: 'x', time: 1, snapshot: new Uint8Array([1, 2, 3]) }, 'junk']);
    (evil.getArray('snapshots') as Y.Array<unknown>).push([{ name: 1 }]);
    (evil.getArray('timeline') as Y.Array<unknown>).push([{ time: 1e300, author: 'AI', kind: 'tool', text: 't' }, null]);
    evil.getMap('branches').set('b1', { id: 'b1' });
    evil.getMap('hints').set('df1 x', { order: 'nope' });
    evil.getMap('counters').set('nextId', 'many');
    return Y.encodeStateAsUpdate(evil);
  }

  it('cannot stop a document from opening, reading or changing', async () => {
    const d = new Doc();
    Y.applyUpdate(d.ydoc, hostile(), { author: 'cloud' });
    expect(d.name).toBe('Untitled');
    expect(d.context).toBe('klr type=A2 nu=2i+j ring=QQ');
    expect(d.pinned).toEqual([]);
    expect(d.edited).toBeUndefined();
    expect(d.list()).toEqual([
      { id: 'df1', text: 'def a := x_1' },
      { id: 'd9', text: '' },
    ]);
    expect(d.at(99)).toBeNull();
    expect(d.snapshotList()).toEqual([]);
    expect(d.branchList()).toEqual([]);
    expect(d.openBranch('b1')).toBeNull();
    expect(d.hint('df1', 'x')).toBeUndefined();
    expect(d.problems()).toEqual(['a malformed name', 'a malformed context', '3 malformed cells', '1 malformed versions', '1 malformed snapshots', '1 malformed timeline entries', '1 malformed branches', '1 malformed layout hints']);
    // editing still works, with fresh IDs
    const id = d.addCell('watch a');
    expect(id).toBe('wt10');
    d.updateCell('d9', 'def b := x_2');
    d.moveCell('d9', 0);
    await settle();
    expect(d.list().map((c) => c.id)).toEqual(['d9', 'df1', 'wt10']);
    d.replace('nilhecke nu=2', [{ id: 'd1', text: 'def a := x_1' }], 'restore');
    expect(d.list()).toEqual([{ id: 'd1', text: 'def a := x_1' }]);
  });

  it('is left out of a clean copy, together with deleted text and history', async () => {
    const d = new Doc();
    const secret = d.addCell('note my password is hunter2');
    d.removeCell(secret);
    d.addCell('def a := x_1');
    Y.applyUpdate(d.ydoc, hostile(), { author: 'cloud' });
    await settle();
    const c = d.cleanCopy('Clean');
    await settle();
    expect(c.problems()).toEqual([]);
    expect(c.name).toBe('Clean');
    expect(c.list()).toEqual(d.list());
    expect(c.versions().map((v) => v.note)).toEqual(['clean copy']);
    const bytes = Y.encodeStateAsUpdate(c.ydoc);
    expect(new TextDecoder().decode(bytes)).not.toContain('hunter2');
    expect(new TextDecoder().decode(Y.encodeStateAsUpdate(d.ydoc))).toContain('hunter2');
    // new cells in the copy get IDs the original never used
    expect(idNumber(c.addCell('watch a'))).toBeGreaterThan(Math.max(...d.list().map((x) => idNumber(x.id))));
  });
});
