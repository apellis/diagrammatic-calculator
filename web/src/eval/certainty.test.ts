import { describe, expect, it } from 'vitest';
import { certainty, conjecture, obtained } from './certainty';
import type { Provenance } from '../engine/ops.gen';

const prov = (status: string, reference: string, hypotheses = ''): Provenance =>
  ({ strategy: 'basis normal form', method: 'm', certificate: { status, reference, hypotheses } }) as Provenance;

describe('certificate presentation', () => {
  it('names the BE2 conjecture without an author possessive, wherever the certificate cites the paper', () => {
    // the quiver Hecke strategy of be2 names the conjecture and the paper in its hypotheses
    const p = prov('conditional', 'odd-lean: QuiverHecke.span_nf (Kang-Kashiwara-Tsuchioka, arXiv:1107.1039v2, Corollary 3.15)',
      "distinct normal forms differ follows from Brundan-Ellis's Nondegeneracy Conjecture (arXiv:1701.04133v2, Section 8)");
    expect(conjecture(p)).toBe('Nondegeneracy Conjecture');
    expect(certainty({ kind: 'watch', provenance: p })).toEqual({ level: 'conditional', mark: 'Assuming nondegeneracy',
      text: 'conditional on the Nondegeneracy Conjecture: that the terms of the normal form are independent rests on it' });
  });

  it('keeps KL literature certificates as literature, and says what a conditional solve rests on', () => {
    const kl = prov('literature', 'Khovanov-Lauda, arXiv:0807.3250, Proposition 3.11 and Theorem 1.2');
    expect(certainty({ kind: 'watch', provenance: kl })).toMatchObject({ level: 'proven', mark: '' });
    expect(obtained({ kind: 'watch', provenance: kl })).toBe('basis normal form (m); proven: the strategy rests on a published theorem');
    const be2 = prov('conditional', 'Brundan-Ellis (arXiv:1701.04133v2), Section 8 (Nondegeneracy Conjecture)');
    expect(certainty({ kind: 'solve', provenance: be2 })?.text).toBe('conditional on the Nondegeneracy Conjecture: that these are all the solutions rests on it');
  });

  it('reads operation responses (no kind) like cells, and ignores a provenance map', () => {
    const be2 = prov('conditional', 'Brundan-Ellis (arXiv:1701.04133v2), Section 8 (Nondegeneracy Conjecture)');
    expect(certainty({ holds: false, certified_by: 'certificate', provenance: be2 })?.text).toContain('that it fails rests on it');
    expect(certainty({ holds: true, certified_by: 'computation', provenance: be2 })?.level).toBe('proven');
    expect(certainty({ provenance: [{ word: 'x', layers: [] }] as unknown as Provenance })).toBeNull();
  });
});
