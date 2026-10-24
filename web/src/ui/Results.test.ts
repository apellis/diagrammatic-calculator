import { describe, expect, it, vi } from 'vitest';

// Certainty is pure; importing its UI module must not install browser event handlers in Node.
vi.mock('solid-js/web', async (original) => ({
  ...await original<typeof import('solid-js/web')>(),
  delegateEvents: () => {},
}));
import { certainty } from './Results';
import type { WorkspaceRunCell } from '../engine/ops.gen';

const result = (status: 'conditional' | 'assumed' | 'unknown' = 'conditional', reference = 'Brundan-Ellis, arXiv:1701.04133, Nondegeneracy Conjecture'): WorkspaceRunCell => ({
  id: 'certainty-test', status: 'ok', kind: 'watch',
  provenance: { strategy: 'basis normal form', method: 'skm-basis', certificate: { status, reference, hypotheses: 'odd colours' }, notes: [] },
});

describe('result certainty', () => {
  it('uses a compact BE2 assumption without upgrading its certificate', () => {
    const v = result();
    expect(certainty(v)).toMatchObject({ level: 'conditional', mark: 'Assuming nondegeneracy' });
    expect(certainty(v)?.text).toContain('Nondegeneracy Conjecture');
    expect(v.provenance?.certificate.status).toBe('conditional');
  });

  it('does not treat other conjectures, assumptions or unknown certificates as accepted BE2', () => {
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
