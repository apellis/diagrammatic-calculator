import { describe, expect, it } from 'vitest';
import { nodeEngine } from '../eval/testing';
import { identityTerm, orientedIdentity, swapBoundary } from './diagram-start';

describe('starting boundary', () => {
  const objects = ['iij', 'iji', 'jii'];
  it('defaults to the first authoritative object, never an invented sequence', () => {
    expect(identityTerm(objects[0], objects)).toBe('e(iij)');
    expect(identityTerm('ij', objects)).toBeUndefined();
    expect(identityTerm(undefined, [])).toBeUndefined();
    expect(identityTerm('', [''])).toBe('e()');
  });
  it('swaps adjacent strands only into an authoritative object', () => {
    expect(swapBoundary(['i', 'i', 'j'], 1, objects)).toBe('iji');
    expect(swapBoundary(['i', 'i', 'j'], 0, objects)).toBeUndefined();
    expect(swapBoundary(['i', 'j'], 0, ['ij'])).toBeUndefined();
    expect(swapBoundary(['i', 'j'], -1, objects)).toBeUndefined();
    expect(swapBoundary(['i', 'j'], 1, objects)).toBeUndefined();
    expect(swapBoundary(['red', 'blue'], 0, ['bluered'])).toBe('bluered');
  });
  it.each(['klr type=A2 nu=2i+j ring=QQ', 'nilhecke nu=3 ring=QQ', 'odd-nilhecke nu=3 ring=QQ'])(
    'validates every offered identity with the real engine: %s', async (context) => {
      const engine = nodeEngine();
      const d = await engine.call('context.describe', { context });
      if (d.status !== 'done' || d.response.status !== 'ok') throw new Error('context failed');
      expect(d.response.objects.length).toBeGreaterThan(0);
      for (const object of d.response.objects) {
        const term = identityTerm(object, d.response.objects)!;
        const r = await engine.call('term.layout', { context, term });
        if (r.status !== 'done' || r.response.status !== 'ok') throw new Error('identity failed');
        expect(r.response.terms).toBe(1);
        expect(r.response.summands[0].source).toBe(object);
        expect(r.response.summands[0].target).toBe(object);
        expect(r.response.summands[0].pieces).toEqual([]);
      }
    },
  );
  it('builds explicit oriented boundaries from declared colours', async () => {
    const r = await nodeEngine().call('context.describe', { context: 'kl type=A1 mu=2 ring=QQ' });
    if (r.status !== 'done' || r.response.status !== 'ok') throw new Error('context failed');
    expect(r.response.objects).toEqual([]);
    expect(identityTerm(r.response.objects[0], r.response.objects)).toBeUndefined();
    expect(orientedIdentity('j', r.response.colours)).toBeUndefined();
    expect(orientedIdentity('i**', r.response.colours)).toBeUndefined();
    expect(swapBoundary(['i', 'i*'], 0)).toBe('i* i');
    for (const object of ['', 'i', 'i i*', 'i* i*']) {
      const term = orientedIdentity(object, r.response.colours)!;
      const layout = await nodeEngine().call('term.layout', { context: r.response.context, term });
      if (layout.status !== 'done' || layout.response.status !== 'ok') throw new Error('identity failed');
      expect(layout.response.summands[0].source).toBe(object);
    }
  });
});
