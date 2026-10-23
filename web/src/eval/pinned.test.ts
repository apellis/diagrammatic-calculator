import { describe, expect, it } from 'vitest';
import { comparePinned, recomputePinned } from './pinned';
import { nodeEngine } from './testing';
import { DEFAULT_CONTEXT } from '../doc/model';

describe('frozen pinned results', () => {
  it('compares the mathematical output and provenance, not drawing IDs or key order', () => {
    expect(comparePinned({ id: 'wt0', kind: 'watch', normal_form: 'x', svg: 'old', provenance: { strategy: 'exact' } },
      { provenance: { strategy: 'exact' }, normal_form: 'x', kind: 'watch', id: 'wt7', svg: 'new' })).toBe(true);
    expect(comparePinned({ normal_form: 'x', provenance: { strategy: 'exact' } }, { normal_form: 'x', provenance: { strategy: 'other' } })).toBe(false);
  });
  it('recomputes the retained dependency workspace in the real engine', async () => {
    const request = { workspace: `diagcalc-workspace/1\ncontext ${DEFAULT_CONTEXT}\ndef a := x_1 e(iji)  # id=df0\nwatch a  # id=wt1\n`, summands: 200, scalars: [] };
    const engine = nodeEngine();
    const first = await recomputePinned(engine, request);
    const second = await recomputePinned(engine, request);
    expect(first.status).toBe('ok');
    expect(comparePinned(first, second)).toBe(true);
    expect(comparePinned({ ...first, normal_form: '0' }, second)).toBe(false);
  });
});
