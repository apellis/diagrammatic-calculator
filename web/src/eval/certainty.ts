// How far a computed value can be trusted, read off its provenance: one reading for the results,
// the outline, checks, branch comparisons, exports, and what the AI is told, so they never differ.
import type { Provenance } from '../engine/ops.gen';

/** How far a value can be trusted, read off its provenance (and, for a check, its verdict). */
export interface Certainty {
  /** proven, or resting on an open conjecture, an assumption, or nothing known. */
  level: 'proven' | 'conditional' | 'assumed' | 'unknown';
  /** A short mark for summaries ('' when proven). */
  mark: string;
  /** One line saying what the value rests on. */
  text: string;
}

/** What the reading needs of a value: a cell's value or an operation's response. */
export interface Certified {
  kind?: string;
  holds?: boolean;
  certified_by?: string;
  provenance?: Provenance;
}

/**
 * The open conjecture a certificate names, such as "Nondegeneracy Conjecture" (an author's
 * possessive before it, as in "Brundan-Ellis's Nondegeneracy Conjecture", is not part of the name).
 */
export function conjecture(p: Provenance): string | null {
  const m = /([A-Z][A-Za-z'-]*(?:[ -][A-Z][A-Za-z'-]*)* Conjecture)/.exec(`${p.certificate.reference} ${p.certificate.hypotheses}`);
  return m ? m[1].replace(/^.*['’]s\s+/, '') : null;
}

/** Whether a conditional certificate rests on Brundan-Ellis's Nondegeneracy Conjecture (arXiv:1701.04133). */
function be2Nondegeneracy(p: Provenance): boolean {
  return conjecture(p) === 'Nondegeneracy Conjecture' &&
    /\barXiv:1701\.04133(?:v\d+)?\b/.test(`${p.certificate.reference} ${p.certificate.hypotheses}`);
}

export function certainty(v: Certified): Certainty | null {
  const p = v.provenance;
  // (an operation that rewrites by hand has a provenance map of its summands instead: no certificate)
  if (!p || typeof p.certificate !== 'object' || !p.certificate) return null;
  // a watch or check (or an operation's response, which has no kind) whose normal form is 0 or
  // whose check holds by relations alone needs no certificate
  if ((v.kind === undefined || v.kind === 'check' || v.kind === 'watch') && v.certified_by === 'computation') {
    return { level: 'proven', mark: '', text: 'proven by the computation alone: every step is a relation of the presentation' };
  }
  switch (p.certificate.status) {
    case 'lean':
      return { level: 'proven', mark: '', text: 'proven: the strategy is checked in Lean' };
    case 'literature':
      return { level: 'proven', mark: '', text: 'proven: the strategy rests on a published theorem' };
    case 'conditional': {
      const c = conjecture(p);
      const on = c ? `the ${c}` : 'an open conjecture';
      const what = v.holds !== undefined && v.kind !== 'solve' ? (v.holds ? 'that it holds' : 'that it fails')
        : v.kind === 'solve' ? 'that these are all the solutions' : 'that the terms of the normal form are independent';
      // The accepted working assumption is specifically BE2 nondegeneracy, not all conjectures.
      return { level: 'conditional', mark: be2Nondegeneracy(p) ? 'Assuming nondegeneracy' : 'conditional', text: `conditional on ${on}: ${what} rests on it` };
    }
    case 'assumed':
      return { level: 'assumed', mark: 'assumed', text: 'rests on an assumption: the representation is assumed faithful' };
    default:
      return { level: 'unknown', mark: 'unproven', text: 'unproven: whether the representation is faithful is not known' };
  }
}

/**
 * How a value was obtained, in a few words for a reader who sees no badge (the AI, a copied
 * file): the strategy and what the value rests on.
 */
export function obtained(v: Certified): string | null {
  const c = certainty(v);
  return c && v.provenance ? `${v.provenance.strategy} (${v.provenance.method}); ${c.text}` : null;
}
