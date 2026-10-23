import { describe, expect, it } from 'vitest';
import { contextTex, render, ringTex, toTex } from './tex';

describe('typesetting text', () => {
  it('writes generators, scripts and idempotents as mathematics', () => {
    expect(toTex('x_1^2 psi_1 e(iij)')).toBe('x_{1}^{2}\\psi_{1}e(\\mathit{iij})');
    expect(toTex('q^-1 x_12')).toBe('q^{-1}x_{12}');
    expect(toTex('Q_ij(x_1, x_2) e(ij)')).toBe('Q_{\\mathit{ij}}(x_{1},\\,x_{2})e(\\mathit{ij})');
    expect(toTex('Qbar_ij(x_1, x_2, x_3)')).toContain('\\bar{Q}_{\\mathit{ij}}');
    expect(toTex('a := psi_1 e(ij)')).toBe('a \\coloneqq \\psi_{1}e(\\mathit{ij})');
    expect(toTex('f (x) g')).toBe('f \\otimes g');
  });

  it('keeps names apart where a space separates them, and writes checks as operators', () => {
    expect(toTex('idempotent(ab)')).toBe('\\operatorname{idempotent}(\\mathit{ab})');
    expect(toTex('ab cd')).toBe('\\mathit{ab}\\,\\mathit{cd}');
    expect(toTex('e e - e = 0 for c1, c2')).toBe('ee-e=0\\quad\\text{for}\\quad c_{1},\\,c_{2}');
    expect(toTex('(q+1)/q psi_1')).toBe('(q+1)/q\\psi_{1}');
  });

  it('writes contexts and rings the way mathematicians do', () => {
    expect(ringTex('QQ')).toBe('\\mathbb{Q}');
    expect(ringTex('QQ(q)')).toBe('\\mathbb{Q}(q)');
    expect(ringTex('ZZ[I]')).toBe('\\mathbb{Z}[I]');
    expect(ringTex('GF(5)')).toBe('\\mathbb{F}_{5}');
    expect(contextTex('type=A2 nu=2i+j ring=QQ')).toBe('\\text{type }A_{2},\\quad \\nu = 2i+j,\\quad \\mathbb{Q}');
    expect(contextTex('type=A1 odd=i mu=-1')).toBe('\\text{type }A_{1},\\quad \\mathrm{odd} = i,\\quad \\mu = -1');
  });

  it('gives KaTeX something it can typeset for every printed form', () => {
    for (const t of ['x_1^2 psi_1 e(iij) - 2 q^-1 psi_2 e(iji) + c1 e(iij)', 'idempotent(a)', 'a := (q+1)/q x_1 * psi_1', 'f ⊗ g ∘ h', 'x_{12}^{3}', 'weird # $ % & ~ \\ { }']) {
      expect(render(toTex(t)), t).not.toBeNull();
    }
    expect(render(contextTex('type=B2 nu=i+2j ring=GF(5) Lambda=2i dotcap=3'))).not.toBeNull();
  });
});
