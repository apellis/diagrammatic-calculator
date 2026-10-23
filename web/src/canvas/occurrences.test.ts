import { describe, expect, it } from 'vitest';
import { nodeEngine } from '../eval/testing';
import { carry, fits, lettersOf } from './occurrences';
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
});
