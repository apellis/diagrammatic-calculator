import { describe, expect, it } from 'vitest';
import { proseHtml } from './prose';

const formulas = (t: string) => (proseHtml(t).match(/class="katex"/g) ?? []).length;
const text = (t: string) => proseHtml(t).replace(/<span class="katex-mathml">.*?<\/span>/g, '').replace(/<[^>]+>/g, '');

describe('certificate prose', () => {
  it('typesets the short formulas of presentation files', () => {
    expect(formulas('coefficients in an integral domain; Q_ab != 0 for a != b')).toBe(2);
    expect(formulas('t_ij = 1, s_ij^pq = 0, c_{lambda,i} = 1. Every step')).toBe(3);
    expect(formulas('Q_ab(u, v) = Q_ba(v, u)')).toBe(1);
    expect(proseHtml('Q_ab != 0')).toContain('≠');
  });

  it('leaves names, versions, and plain words alone', () => {
    expect(formulas('Q = klq2 (the default)')).toBe(0);
    expect(formulas('Khovanov-Lauda, arXiv:0807.3250, Proposition 3.11 and Theorem 1.2')).toBe(0);
    expect(formulas('odd-lean: EvenBridge.nondegeneracyConjecture_of_isEven and SKM theorem_8_1')).toBe(0);
    expect(proseHtml('categorification-lean: KL3.Diagram.canonicalBasis.')).toContain('<code>KL3.Diagram.canonicalBasis</code>.');
    expect(text('Brundan-Ellis, Super Kac-Moody 2-categories')).toBe('Brundan–Ellis, Super Kac–Moody 2-categories');
  });

  it('escapes what it does not typeset', () => {
    expect(proseHtml('a <b> & "c"')).toBe('a &lt;b&gt; &amp; &quot;c&quot;');
  });
});
