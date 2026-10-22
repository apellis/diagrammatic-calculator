import { describe, expect, it } from 'vitest';
import { nodeEngine } from '../eval/testing';
import { crossingsPassed, dropTarget, insertAbove, moveBy, moveTo, nextOnStrand, onStrand, piece, piecesIn, rectangle, type Layout } from './geometry';

const engine = nodeEngine();

async function layout(context: string, term: string, orders?: Record<string, number[]>): Promise<Layout[]> {
  const o = await engine.call('term.layout', { context, term, orders });
  if (o.status !== 'done' || o.response.status !== 'ok') throw new Error(JSON.stringify(o));
  return o.response.summands;
}

describe('canvas geometry', () => {
  it('finds the crossings a dot passes and the next piece on its strand', async () => {
    // x_1 below psi_1 psi_2 psi_1 in nilHecke: the dot's strand meets two crossings going up
    const [l] = await layout('nilhecke nu=3', 'psi_1 psi_2 psi_1 x_1');
    const dot = l.pieces.find((p) => p.draw === 'dot')!;
    expect(crossingsPassed(l, dot, 10)).toEqual({ direction: 'up', steps: 2 });
    expect(crossingsPassed(l, dot, 1.6)).toEqual({ direction: 'up', steps: 1 });
    expect(nextOnStrand(l, dot, true)?.draw).toBe('crossing');
    expect(nextOnStrand(l, dot, false)).toBeUndefined();
    // the strand through the dot meets the dot and the crossings it passes through
    expect(onStrand(l, dot.inputs[0]).length).toBe(3);
  });

  it('turns a box into a rectangle of layers and strands', async () => {
    const [l] = await layout('klr type=A2 nu=2i+j', 'psi_2 psi_1 x_3 e(iij)');
    const all = piecesIn(l, [-1, -1], [5, 5]);
    expect(all.sort()).toEqual([1, 2, 3]);
    const bottom = piecesIn(l, [-1, -0.1], [5, 1]);
    expect(rectangle(l, bottom)).toEqual({ layers: [1, 2], strands: [1, 3] });
  });

  it('moves pieces in the drawn order and places drops between rows', async () => {
    const [l] = await layout('nilhecke nu=3', 'psi_1 psi_1 x_3');
    const dot = l.pieces.find((p) => p.draw === 'dot')!;
    expect(moveTo(l, dot.layer, 10)).toEqual([1, 3, 2]);
    expect(moveBy(l.order, dot.layer, 1)).toEqual([1, 3, 2]);
    expect(moveBy(l.order, dot.layer, -5)).toEqual([2, 1, 3]);
    const [moved] = await layout('nilhecke nu=3', 'psi_1 psi_1 x_3', { 1: [1, 3, 2] });
    expect(piece(moved, dot.layer)!.box[1]).toBeGreaterThan(2);
    // a crossing dropped near the top, between strands 2 and 3
    expect(dropTarget(l, 1.6, 5, 2)).toMatchObject({ below: 3, position: 2 });
    expect(dropTarget(l, -3, 0.1, 1)).toMatchObject({ below: 0, position: 1 });
    expect(insertAbove(l, dot, 1)).toMatchObject({ position: 3 });
  });
});
