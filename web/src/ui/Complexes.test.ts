import { expect, it, vi } from 'vitest';

vi.mock('solid-js/web', async (original) => ({
  ...await original<typeof import('solid-js/web')>(),
  delegateEvents: () => {},
}));
import { complexCellText, readStructured, structuredHead } from './Complexes';

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
