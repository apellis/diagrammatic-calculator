import { describe, expect, it, vi } from 'vitest';

vi.mock('solid-js/web', async (original) => ({
  ...await original<typeof import('solid-js/web')>(),
  delegateEvents: () => {},
}));
import { truncationOf, unknownsOf } from './Deform';

describe('the deformation solver', () => {
  it('reads truncate and hdeg from a context line', () => {
    expect(truncationOf('klrw type=A1 ring=QQ truncate=h^2 hdeg=2')).toEqual({ order: 2, hdeg: 2 });
    expect(truncationOf('klrw type=A1 truncate=h^3')).toEqual({ order: 3, hdeg: 0 });
    expect(truncationOf('klrw type=A1 ring=QQ')).toBeNull();
  });
  it('sends a whole differential when every entry is picked, else the entries', () => {
    const size = (from: number): [number, number] => (from === -1 ? [2, 1] : [1, 2]);
    const picked = new Map([[-1, new Set(['0,0', '1,0'])], [0, new Set(['0,1'])], [1, new Set<string>()]]);
    expect(unknownsOf(picked, size)).toEqual([{ from: -1 }, { from: 0, row: 0, col: 1 }]);
  });
});
