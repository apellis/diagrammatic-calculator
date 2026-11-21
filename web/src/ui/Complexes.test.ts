import { expect, it, vi } from 'vitest';

vi.mock('solid-js/web', async (original) => ({
  ...await original<typeof import('solid-js/web')>(),
  delegateEvents: () => {},
}));
import { applyEntryEdit, complexCellText, fallbackRead, flagsFor, mapOf, readStructured, sameRef, structuredHead, type FailureJ } from './Complexes';

it('reads a complex or module cell: name, JSON, and the options after it', () => {
  const r = readStructured('complex R := {"terms": [{"at": 0, "summands": [{"object": "i@1"}]}], "differentials": []} cap=4');
  expect(r).toMatchObject({ kind: 'complex', name: 'R', tail: 'cap=4' });
  expect(r?.value.terms).toHaveLength(1);
  expect(readStructured('module L := {"head": "i@1"}')).toMatchObject({ kind: 'module', name: 'L', value: { head: 'i@1' }, tail: '' });
  expect(readStructured('module L := {"head": "a}b"}')?.value.head).toBe('a}b');
  expect(readStructured('module L := {"head":')).toBeNull();
  expect(readStructured('check e')).toBeNull();
});

it('writes a collapsed cell as its shape, not its JSON', () => {
  expect(structuredHead('module L := {"head": "i@1"}')).toBe('L := P(i@1) / rad');
  expect(structuredHead('module M := {"generators": [{"object": "i@1"}], "relations": [{"object": "i@1", "shift": 2}], "matrix": [["x_1 e(i@1)"]]}'))
    .toBe('M := coker(P(i@1)⟨2⟩ → P(i@1))');
  expect(structuredHead('complex R := {"terms": [{"at": -1, "summands": [{"object": "i@1", "shift": 2}]}, {"at": 0, "summands": [{"object": "i@1"}]}], "differentials": [], "resolves": {"module": "L"}}'))
    .toBe('R := P(i@1)⟨2⟩ → P(i@1) → L');
});

it('writes back a tool result without the defaults, still naming the module cell', () => {
  const text = complexCellText('R', {
    dimension: 1, indexing: 'cohomological', odd_signs: 'koszul', squares: 'commute',
    terms: [{ at: 0, summands: [{ object: 'i@1', shift: 0 }] }], differentials: [],
    resolves: { module: { generators: [{ object: 'i@1' }] } },
  } as never, 'L', 'cap=4');
  expect(text).toBe('complex R := {"terms":[{"at":0,"summands":[{"object":"i@1"}]}],"differentials":[],"resolves":{"module":"L"}} cap=4');
  expect(readStructured(text)?.value.resolves?.module).toBe('L');
});

it('an entry edit fills a missing map with zeros and leaves the rest alone', () => {
  const c = { terms: [{ at: -1, summands: [{ object: 'a' }] }, { at: 0, summands: [{ object: 'b' }, { object: 'c' }] }], differentials: [] };
  const n = applyEntryEdit(c, { ref: { map: 'differential', from: -1, axis: 0 }, row: 1, col: 0, rows: 2, cols: 1, text: 'x' });
  expect(n.differentials).toEqual([{ from: -1, axis: 0, matrix: [[null], ['x']] }]);
  expect(c.differentials).toEqual([]);
  const m = applyEntryEdit({ generators: [], relations: [], matrix: [['y', 'z']] }, { ref: { map: 'presentation' }, row: 0, col: 1, rows: 1, cols: 2, text: null });
  expect(m.matrix).toEqual([['y', null]]);
  const r = applyEntryEdit({ resolves: { module: 'L', augmentation: [['1']] } }, { ref: { map: 'augmentation' }, row: 0, col: 0, rows: 1, cols: 1, text: '2' });
  expect(r.resolves).toEqual({ module: 'L', augmentation: [['2']] });
});

it('an entry edit of a chain map component creates or updates the component at its position', () => {
  const f = { source: 'C', target: 'D', map: [{ at: 0, matrix: [['1']] }] };
  const g = applyEntryEdit(f as never, { ref: { map: 'component', at: -1 }, row: 0, col: 1, rows: 1, cols: 2, text: 'x' }) as typeof f;
  expect(g.map).toEqual([{ at: 0, matrix: [['1']] }, { at: -1, matrix: [[null, 'x']] }]);
  const h = applyEntryEdit(g as never, { ref: { map: 'component', at: 0 }, row: 0, col: 0, rows: 1, cols: 1, text: null }) as typeof f;
  expect(h.map[0]).toEqual({ at: 0, matrix: [[null]] });
  expect(f.map).toHaveLength(1);
  expect(h.source).toBe('C');
});

it('compares references to maps', () => {
  expect(sameRef({ map: 'differential', from: -1, axis: 0 }, { map: 'differential', from: [-1], axis: 0 })).toBe(true);
  expect(sameRef({ map: 'differential', from: -1, axis: 0 }, { map: 'differential', from: -2, axis: 0 })).toBe(false);
  expect(sameRef({ map: 'component', at: 0 }, { map: 'component', at: [0] })).toBe(true);
  expect(sameRef({ map: 'augmentation' }, { map: 'presentation' })).toBe(false);
});

it('marks the maps and entries a failed check involves', () => {
  const fs = [
    { location: 'd ∘ d from -2', detail: '', maps: [{ map: 'differential', from: -2, axis: 0 }, { map: 'differential', from: -1, axis: 0 }] },
    { location: 'degree', detail: '', maps: [{ map: 'differential', from: -1, axis: 0, row: 0, col: 1 }] },
  ] as unknown as FailureJ[];
  const a = flagsFor(fs, { map: 'differential', from: -2, axis: 0 });
  expect(a.map).toBe(true);
  expect(a.entries.size).toBe(0);
  const b = flagsFor(fs, { map: 'differential', from: -1, axis: 0 });
  expect(b.map).toBe(true);
  expect([...b.entries]).toEqual(['0,1']);
  expect(flagsFor(fs, { map: 'augmentation' })).toEqual({ map: false, entries: new Set() });
});

it('reads a cell the engine rejected from its text, keeping every entry', () => {
  const text = 'complex C := {"terms":[{"at":-1,"summands":[{"object":"i@1","shift":2},{"object":"i@1"}]},{"at":0,"summands":[{"object":"i@1"}]}],"differentials":[{"from":-1,"matrix":[["x_1 e(i@1) +"]]}]}';
  const s = fallbackRead(text, 'cannot read x_1 e(i@1) +');
  expect(s?.error).toBe('cannot read x_1 e(i@1) +');
  const m = s && mapOf(s, { map: 'differential', from: -1, axis: 0 });
  // one row (the target), two columns (the source); the missing entry is zero
  expect(m?.m).toEqual([['x_1 e(i@1) +', null]]);
  expect(m?.title).toBe('the differential from −1');
  // a module given by its head has nothing to show until the engine finds its presentation
  expect(fallbackRead('module L := {"head": "i@1"}', 'error')).toBeNull();
  const cm = fallbackRead('chainmap F := {"source": "C", "target": "C", "map": [{"at": 0, "matrix": [["e(i@1)"]]}]}', 'error',
    () => ({ terms: [{ at: 0, summands: [{ object: 'i@1' }] }] }));
  expect(cm && mapOf(cm, { map: 'component', at: 0 })?.m).toEqual([['e(i@1)']]);
});
