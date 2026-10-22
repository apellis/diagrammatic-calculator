import { describe, expect, it } from 'vitest';
import { nodeEngine } from '../eval/testing';
import { along, crossingsOn, crossingsPassed, dropTarget, insertAbove, moveBy, moveTo, nextOnStrand, onStrand, pullMove, piece, piecesIn, rectangle, track, trackRange, type Layout } from './geometry';

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

  it('follows a dragged dot along its strand through the crossings', async () => {
    const [l] = await layout('nilhecke nu=3', 'psi_1 psi_2 psi_1 x_1');
    const dot = l.pieces.find((p) => p.draw === 'dot')!;
    const run = track(l, dot);
    // the run is one strand drawn end to end, from the bottom of the diagram to the top
    expect(run[0].y0).toBe(0);
    expect(run[run.length - 1].y1).toBe(l.height);
    run.slice(1).forEach((seg, i) => expect([seg.x0, seg.y0]).toEqual([run[i].x1, run[i].y1]));
    const [lo, hi] = trackRange(run);
    expect(along(run, -5)[1]).toBe(lo);
    expect(along(run, 99)[1]).toBe(hi);
    // half way up a crossing the strand is half way across it; at its ends it is on the ends
    const c = crossingsOn(l, dot)[0];
    const seg = run.find((x) => x.layer === c.layer)!;
    expect(along(run, (seg.y0! + seg.y1!) / 2)[0]).toBeCloseTo((seg.x0! + seg.x1!) / 2, 3);
    expect(along(run, seg.y0! + 1e-9)[0]).toBeCloseTo(seg.x0!, 3);
    expect(crossingsOn(l, dot).length).toBe(2);
    // a dot on a bare strand has a run of its own strand only, with no crossings
    const [bare] = await layout('klr type=A2 nu=i+j', 'x_1 e(ij)');
    const d = bare.pieces[0];
    expect(track(bare, d).length).toBe(1);
    expect(crossingsOn(bare, d)).toEqual([]);
  });

  it('finds the braid triangle and the stacked pair a sideways crossing drag can resolve', async () => {
    const match = async (context: string, term: string) => {
      const o = await engine.call('relations.match', { context, term, summand: 1 });
      if (o.status !== 'done' || o.response.status !== 'ok') throw new Error(JSON.stringify(o));
      return o.response.matches;
    };
    const ctx = 'klr type=A2 nu=2i+j';
    const [l] = await layout(ctx, 'psi_1 psi_2 psi_1 e(iji)');
    const ms = await match(ctx, 'psi_1 psi_2 psi_1 e(iji)');
    const order = [...l.pieces].sort((a, b) => a.box[1] - b.box[1]);
    const [bottom, middle] = order;
    // the middle crossing is on the right (positions 2, 3): it is dragged left, towards the others
    const move = pullMove(l, ms, middle.layer)!;
    expect(ms[move.index].relation).toMatch(/braid/);
    expect(move.side).toBe(-1);
    expect(move.layers.length).toBe(3);
    // an outer crossing of the triangle alone makes no move
    expect(pullMove(l, ms, bottom.layer)).toBeUndefined();
    // a stacked pair of crossings is pulled apart by the square relation, either way
    const [sq] = await layout(ctx, 'psi_1 psi_1 e(iji)');
    const pair = pullMove(sq, await match(ctx, 'psi_1 psi_1 e(iji)'), sq.pieces[0].layer)!;
    expect(pair.side).toBe(0);
    expect(pair.layers.length).toBe(2);
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
