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
