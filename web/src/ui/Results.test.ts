import { describe, expect, it, vi } from 'vitest';

// Certainty is pure; importing its UI module must not install browser event handlers in Node.
vi.mock('solid-js/web', async (original) => ({
  ...await original<typeof import('solid-js/web')>(),
  delegateEvents: () => {},
}));
import { certainty, dotTitle, summary } from './Results';
import type { WorkspaceRunCell } from '../engine/ops.gen';

const result = (status: 'conditional' | 'assumed' | 'unknown' = 'conditional', reference = 'Brundan-Ellis, arXiv:1701.04133, Nondegeneracy Conjecture'): WorkspaceRunCell => ({
  id: 'certainty-test', status: 'ok', kind: 'watch',
  provenance: { strategy: 'basis normal form', method: 'skm-basis', certificate: { status, reference, hypotheses: 'odd colours' }, notes: [] },
});

it('never labels a stale check as holds or fails', () => {
  for (const holds of [true, false]) {
    expect(summary({ id: 'k', text: 'check idempotent(e)', status: 'stale', value: { id: 'k', kind: 'check', status: 'ok', holds } })).toBe('recomputing…');
  }
});

it("says what a cell's dot means: whether the computation is current, not a check's verdict", () => {
  const value = { id: 'k', kind: 'check', status: 'ok', holds: false } as const;
  expect(dotTitle({ id: 'k', text: 'check idempotent(e)', status: 'fresh', value })).toBe('Computed');
  expect(dotTitle({ id: 'k', text: 'check idempotent(e)', status: 'stale', value })).toBe('Recomputing');
  expect(dotTitle({ id: 'k', text: 'check idempotent(e)', status: 'running' })).toBe('Computing');
  expect(dotTitle({ id: 'k', text: 'check idempotent(', status: 'failed', parseError: 'unexpected end' })).toBe('Computation failed');
  expect(dotTitle(undefined)).toBe('Not computed yet');
});

describe('result certainty', () => {
  it('uses a compact BE2 assumption without upgrading its certificate', () => {
    const v = result();
    expect(certainty(v)).toMatchObject({ level: 'conditional', mark: 'Assuming nondegeneracy' });
    expect(certainty(v)?.text).toContain('Nondegeneracy Conjecture');
    expect(v.provenance?.certificate.status).toBe('conditional');
  });

  it('does not treat other conjectures, assumptions, or unknown certificates as accepted BE2', () => {
    expect(certainty(result('conditional', 'Another Nondegeneracy Conjecture'))).toMatchObject({ level: 'conditional', mark: 'conditional' });
    expect(certainty(result('assumed'))).toMatchObject({ level: 'assumed', mark: 'assumed' });
    expect(certainty(result('unknown'))).toMatchObject({ level: 'unknown', mark: 'unproven' });
  });

  it('preserves computation-only proofs but never promotes a solve', () => {
    for (const kind of ['watch', 'check'] as const) {
      expect(certainty({ ...result(), kind, certified_by: 'computation' })).toMatchObject({ level: 'proven', mark: '' });
    }
    expect(certainty({ ...result(), kind: 'solve', certified_by: 'computation' })?.level).toBe('conditional');
  });
});

it('sums up a complex whose homology search stopped at a degree cap without calling it not exact', async () => {
  const { complexSummary } = await import('./Results');
  const w = { from: 0, to: 6, truncated: true };
  expect(complexSummary({ kind: 'complex', homology: { exact: false, positions: [{ at: -1, zero: true, pieces: [], window: w }, { at: 0, zero: true, pieces: [], window: w }] } }))
    .toBe('no homology found in the degrees checked');
  expect(complexSummary({ kind: 'complex', homology: { exact: false, positions: [{ at: 0, zero: false, pieces: [{ test: 'i@1', degree: 0, dimension: 1 }], window: null }] } })).toBe('not exact');
  expect(complexSummary({ kind: 'complex', homology: { exact: true, positions: [] } })).toBe('exact');
  // a resolution checked only in a degree window says so
  const res = { kind: 'complex' as const, complex: { resolves: { module: 'L' } } as never };
  expect(complexSummary({ ...res, resolution: { holds: true, resolved: true, length: 1, complete: false } })).toBe('resolves in the degrees checked, length 1');
  expect(complexSummary({ ...res, resolution: { holds: true, resolved: true, length: 1, complete: true } })).toBe('resolves, length 1');
});
