import { describe, expect, it } from 'vitest';
import { Evaluator, type CellState } from './evaluator';
import { memoryCache, nodeEngine } from './testing';

const settle = async () => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

function setup() {
  const engine = nodeEngine();
  const cache = memoryCache();
  const seen: CellState[] = [];
  const ev = new Evaluator(engine, cache, { engineId: 'test', summands: 10 }, (s) => seen.push(s));
  return { engine, cache, ev, seen };
}

const ctx = 'klr type=A2 nu=2i+j ring=QQ';

describe('evaluator', () => {
  it('evaluates cells with the definitions they use', async () => {
    const { ev } = setup();
    await ev.update(ctx, [
      { id: 'd1', text: 'def a := psi_1 psi_2 psi_1 e(iji)' },
      { id: 'w2', text: 'watch a a - a' },
      { id: 'k3', text: 'check idempotent(a)' },
      { id: 'n4', text: 'note """hello"""' },
    ]);
    await settle();
    const w = ev.state('w2')!;
    expect(w.status).toBe('fresh');
    expect(w.value).toMatchObject({ kind: 'watch', normal_form: '0', terms: 0 });
    expect(ev.state('k3')!.value).toMatchObject({ holds: true });
    expect(ev.state('d1')!.value).toMatchObject({ kind: 'def', terms: 1 });
    expect(ev.state('n4')!.status).toBe('static');
  });

  it('traces a summand of a watch to the terms of the definitions, also for a cached value', async () => {
    const { ev, cache, engine } = setup();
    const cells = [
      { id: 'd1', text: 'def a := x_1 e(iij) + psi_1 e(iij)' },
      { id: 'w2', text: 'watch a a' },
    ];
    await ev.update(ctx, cells);
    await settle();
    const o = await ev.trace('w2', 'psi_1 x_2 e(iij)');
    expect(o.status).toBe('done');
    const r = (o as { response: { status: string; sources: { factors: { name: string; term: number }[]; text: string }[] } }).response;
    expect(r.status).toBe('ok');
    expect(r.sources.map((s) => s.factors.map((f) => `${f.name}${f.term}`).join(' '))).toEqual(['a1 a2']);
    // a fresh evaluator reading the cache can trace too
    const again = new Evaluator(engine, cache, { engineId: 'test', summands: 10 }, () => {});
    await again.update(ctx, cells);
    await settle();
    const p = await again.trace('w2', 'psi_1 x_2 e(iij)');
    expect(p.status === 'done' && p.response.status).toBe('ok');
  });

  it('recomputes only downstream cells, and undo is a cache hit', async () => {
    const { ev, engine } = setup();
    const cells = [
      { id: 'd1', text: 'def a := x_1 e(iji)' },
      { id: 'd2', text: 'def b := x_3 e(iji)' },
      { id: 'w3', text: 'watch a a' },
      { id: 'w4', text: 'watch b b' },
    ];
    await ev.update(ctx, cells);
    await settle();
    const before = engine.calls.filter((c) => c === 'workspace.run').length;
    const k4 = ev.state('w4')!.key;
    await ev.update(ctx, [{ id: 'd1', text: 'def a := x_2 e(iji)' }, ...cells.slice(1)]);
    await settle();
    expect(engine.calls.filter((c) => c === 'workspace.run').length - before).toBe(2); // d1 and w3
    expect(ev.state('w4')!.key).toBe(k4);
    expect(ev.state('w3')!.value).toMatchObject({ normal_form: 'x_2^2 e(iji)' });
    expect(ev.state('w3')!.previous).toMatchObject({ normal_form: 'x_1^2 e(iji)' });
    const runs = engine.calls.filter((c) => c === 'workspace.run').length;
    await ev.update(ctx, cells);
    await settle();
    expect(engine.calls.filter((c) => c === 'workspace.run').length).toBe(runs);
    expect(ev.state('w3')!.value).toMatchObject({ normal_form: 'x_1^2 e(iji)' });
  });

  it('makes the unknowns of solve cells scalars everywhere', async () => {
    const { ev } = setup();
    await ev.update(ctx, [
      { id: 'd1', text: 'def e := c1 psi_1 psi_2 psi_1 e(iji) + c2 psi_2 psi_1 psi_2 e(iji)' },
      { id: 'w2', text: 'watch e' },
      { id: 's3', text: 'solve e e - e = 0 for c1, c2' },
    ]);
    await settle();
    expect(ev.state('w2')!.status).toBe('fresh');
    expect(ev.state('s3')!.value).toMatchObject({ kind: 'solve', status: 'ok' });
  });

  it('reports parse errors and errors from the engine', async () => {
    const { ev } = setup();
    await ev.update(ctx, [
      { id: 'w1', text: 'watch psi_1 +' },
      { id: 'w2', text: 'watch nosuch' },
    ]);
    await settle();
    expect(ev.state('w1')!.status).toBe('failed');
    expect(ev.state('w1')!.parseError).toBeTruthy();
    expect(ev.state('w2')!.status).toBe('failed');
    expect(ev.state('w2')!.value).toMatchObject({ status: 'error' });
  });
});

it('never drops an invalid shadowing definition and checks an older binding instead', async () => {
  const { ev } = setup();
  await ev.update('nilhecke nu=2 ring=QQ', [
    { id: 'd1', text: 'def e := 1' },
    { id: 'd2', text: 'def e := x_1 +' },
    { id: 'k3', text: 'check idempotent(e)' },
  ]);
  await settle();
  expect(ev.state('k3')?.status).toBe('failed');
  expect(ev.state('k3')?.value).toMatchObject({ status: 'error' });
});

describe('scalars', () => {
  it('gives each cell only the unknowns it mentions', async () => {
    const { ev } = setup();
    await ev.update('klr type=A2 nu=2i+j ring=QQ Lambda=2i+j', [
      { id: 'd1', text: 'def e := c1 e(iji) + c2 psi_1 psi_2 psi_1 e(iji)' },
      { id: 's2', text: 'solve e e - e = 0 for c1, c2' },
      { id: 'd3', text: 'def a := psi_1 psi_2 psi_1 e(iji)' },
      { id: 'k4', text: 'check primitive(a)' },
    ]);
    await settle();
    expect(ev.state('s2')!.value).toMatchObject({ outcome: 'finite', points: [['0', '0'], ['1', '0'], ['0', '1'], ['1', '-1']] });
    expect(ev.state('k4')!.value).toMatchObject({ holds: true });
  });
});
