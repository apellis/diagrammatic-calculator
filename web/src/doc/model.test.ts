import { describe, expect, it } from 'vitest';
import { Doc } from './model';

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
