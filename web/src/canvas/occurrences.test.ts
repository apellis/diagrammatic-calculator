import { describe, expect, it } from 'vitest';
import { nodeEngine } from '../eval/testing';
import { carry, fits, idMap, lettersOf, strandAnchors } from './occurrences';
import type { Layout } from './geometry';

const engine = nodeEngine();
const draws: Record<string, string> = { x: 'dot', psi: 'crossing' };

async function layout(context: string, term: string): Promise<Layout[]> {
  const o = await engine.call('term.layout', { context, term });
  if (o.status !== 'done' || o.response.status !== 'ok') throw new Error(JSON.stringify(o));
  return o.response.summands;
}

/** A counter per letter, as the document keeps them. */
function counter(start: Record<string, number> = {}) {
  const next = { ...start };
  return (letter: string) => {
    const n = next[letter] ?? 1;
    next[letter] = n + 1;
    return `${letter}${n}`;
  };
}

describe('occurrence IDs', () => {
  it('are lettered by how each layer is drawn', async () => {
    const [l] = await layout('nilhecke nu=2', 'psi_1 x_1');
    expect(lettersOf(l)).toEqual(['d', 'c']);
    expect(fits(['d4', 'c1'], lettersOf(l))).toBe(true);
    expect(fits(['c1', 'd4'], lettersOf(l))).toBe(false);
    expect(fits(['d4'], lettersOf(l))).toBe(false);
  });

  it('survive a dot slide; the correction term has none to keep', async () => {
    const o = await engine.call('slide', { context: 'nilhecke nu=2', term: 'psi_1 x_1', summand: 1, layer: 1, direction: 'up', steps: 0 });
    if (o.status !== 'done' || o.response.status !== 'ok') throw new Error(JSON.stringify(o));
    const ids = carry(o.response.provenance, (k) => (k === 1 ? ['d7', 'c3'] : undefined), (g) => draws[g], counter({ d: 9, c: 5 }));
    const [slid] = await layout('nilhecke nu=2', o.response.term).then((ls) => ls.filter((l) => l.order.length === 2));
    // the dot is now above the crossing: same IDs, new layers
    expect(lettersOf(slid)).toEqual(['c', 'd']);
    expect(ids[slid.word]).toEqual(['c3', 'd7']);
  });

  it('are fresh for an inserted piece, and kept for the others', async () => {
    const o = await engine.call('term.edit', { context: 'klr type=A2 nu=2i+j', term: 'x_3 e(iji)', summand: 1, action: 'insert', generator: 'psi', below: 0, position: 2 });
    if (o.status !== 'done' || o.response.status !== 'ok') throw new Error(JSON.stringify(o));
    const ids = carry(o.response.provenance, () => ['d2'], (g) => draws[g], counter({ c: 4 }));
    expect(ids[o.response.word]).toEqual(['c4', 'd2']);
  });

  it('take the oldest ID when equal words are added together, and never repeat one in a summand', () => {
    const fresh = counter({ d: 10 });
    const prov = [{ word: 'w', layers: [{ generator: 'x', from: [[1, 1], [2, 1]] }, { generator: 'x', from: [[2, 1]] }] }];
    expect(carry(prov, (k) => (k === 1 ? ['d5'] : ['d3']), (g) => draws[g], fresh)).toEqual({ w: ['d3', 'd10'] });
  });

  it('carry through text typed by hand: unchanged pieces keep theirs, a typed braid its crossings', async () => {
    const o = await engine.call('term.diff', {
      context: 'klr type=A2 nu=2i+j',
      before: 'psi_1 x_1 e(iij) + x_3 e(iij)',
      term: 'psi_1 x_2 x_1 e(iij) + 2 x_3 e(iij) + psi_2 e(iij)',
    });
    if (o.status !== 'done' || o.response.status !== 'ok') throw new Error(JSON.stringify(o));
    const r = o.response;
    const stored: Record<string, string[]> = { 'psi_1 x_1 e(iij)': ['d1', 'c1'], 'x_3 e(iij)': ['d2'] };
    const ids = carry(r.provenance, (k) => stored[r.before[k - 1]], (g) => draws[g], counter({ d: 3, c: 2 }));
    expect(ids).toEqual({ 'x_3 e(iij)': ['d2'], 'psi_2 e(iij)': ['c2'], 'psi_1 x_1 x_2 e(iij)': ['d1', 'd3', 'c1'] });
    const b = await engine.call('term.diff', { context: 'klr type=A2 nu=2i+j', before: 'psi_1 psi_2 psi_1 e(iji)', term: 'psi_2 psi_1 psi_2 e(iji)' });
    if (b.status !== 'done' || b.response.status !== 'ok') throw new Error(JSON.stringify(b));
    expect(carry(b.response.provenance, () => ['c4', 'c5', 'c6'], (g) => draws[g], counter())).toEqual({ 'psi_2 psi_1 psi_2 e(iji)': ['c4', 'c5', 'c6'] });
  });

  it('anchor strands where they start: the bottom edge, else the piece they start at', async () => {
    const [klr] = await layout('klr type=A2 nu=2i+j', 'psi_1 x_2 e(iij)');
    expect(strandAnchors(klr, new Map())).toEqual(['bottom 1', 'bottom 2', 'bottom 3']);
    // a bubble starts at its cup; a zigzag is the strand from the bottom
    const [bubble, zigzag] = await layout('kl type=A1 mu=0', 'cap_2 x_1 cup_{1,i} e(i) + cap_1 x_2 cup_{1,i*} e()');
    expect(strandAnchors(bubble, idMap(['u7', 'd2', 'n3']))).toEqual(['on u7']);
    expect(strandAnchors(bubble, new Map())).toEqual([undefined]);
    expect(strandAnchors(zigzag, new Map())).toEqual(['bottom 1']);
  });
});
