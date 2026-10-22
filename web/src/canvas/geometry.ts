// Geometry of a drawn summand (from `term.layout`): hit tests, the targets of drags and drops,
// and the requests gestures turn into. Coordinates are diagram units, y upwards.
import type { TermLayoutLayout } from '../engine/ops.gen';

export type Layout = TermLayoutLayout;
export type Piece = Layout['pieces'][number];
export type Line = Layout['lines'][number];

/** A rectangle of a summand's canonical layering, as the rewriting operations take it. */
export interface Rect {
  layers: [number, number];
  strands: [number, number];
}

export const centre = (p: Piece): [number, number] => [(p.box[0] + p.box[2]) / 2, (p.box[1] + p.box[3]) / 2];

export const piece = (l: Layout, layer: number) => l.pieces.find((p) => p.layer === layer);

/** The pieces whose centres lie in the box spanned by two corners. */
export function piecesIn(l: Layout, a: [number, number], b: [number, number]): number[] {
  const [x0, x1] = [Math.min(a[0], b[0]), Math.max(a[0], b[0])];
  const [y0, y1] = [Math.min(a[1], b[1]), Math.max(a[1], b[1])];
  return l.pieces
    .filter((p) => {
      const [x, y] = centre(p);
      return x >= x0 && x <= x1 && y >= y0 && y <= y1;
    })
    .map((p) => p.layer);
}

/** The rectangle of layers × strands that holds the given pieces (canonical numbering, from 1). */
export function rectangle(l: Layout, layers: number[]): Rect | undefined {
  const ps = layers.map((k) => piece(l, k)).filter((p): p is Piece => !!p);
  if (!ps.length) return undefined;
  return {
    layers: [Math.min(...ps.map((p) => p.layer)), Math.max(...ps.map((p) => p.layer))],
    strands: [Math.min(...ps.map((p) => p.pos + 1)), Math.max(...ps.map((p) => p.pos + Math.max(1, p.width)))],
  };
}

/** The drawn bounding box of some pieces, for showing a selection snapped to them. */
export function bounds(l: Layout, layers: number[]): [number, number, number, number] | undefined {
  const ps = layers.map((k) => piece(l, k)).filter((p): p is Piece => !!p);
  if (!ps.length) return undefined;
  return [Math.min(...ps.map((p) => p.box[0])), Math.min(...ps.map((p) => p.box[1])), Math.max(...ps.map((p) => p.box[2])), Math.max(...ps.map((p) => p.box[3]))];
}

/** The pieces a strand enters or leaves. */
export function onStrand(l: Layout, strand: number): number[] {
  return l.pieces.filter((p) => p.inputs.includes(strand) || p.outputs.includes(strand)).map((p) => p.layer);
}

/** The strand a dot sits on. */
export const dotStrand = (p: Piece): number | undefined => (p.draw === 'dot' ? p.inputs[0] : undefined);

/**
 * How many crossings a dot passes on its own strand when dragged from its height to `y`, and in
 * which direction: the request a dot drag becomes.
 */
export function crossingsPassed(l: Layout, dot: Piece, y: number): { direction: 'up' | 'down'; steps: number } {
  const s = dotStrand(dot);
  const y0 = centre(dot)[1];
  const up = y > y0;
  const steps = l.pieces.filter((p) => {
    if (p.draw !== 'crossing' || s === undefined || !p.inputs.includes(s)) return false;
    const c = centre(p)[1];
    return up ? c > y0 && c < y : c < y0 && c > y;
  }).length;
  return { direction: up ? 'up' : 'down', steps };
}

/** The next piece on a dot's strand above or below it, if any. */
export function nextOnStrand(l: Layout, dot: Piece, up: boolean): Piece | undefined {
  const s = dotStrand(dot);
  if (s === undefined) return undefined;
  const y0 = centre(dot)[1];
  const along = l.pieces
    .filter((p) => p !== dot && (p.inputs.includes(s) || p.outputs.includes(s)))
    .filter((p) => (up ? centre(p)[1] > y0 : centre(p)[1] < y0))
    .sort((a, b) => (up ? centre(a)[1] - centre(b)[1] : centre(b)[1] - centre(a)[1]));
  return along[0];
}

/** The drawn order after moving a piece so that it sits just above the pieces drawn below `y`. */
export function moveTo(l: Layout, layer: number, y: number): number[] {
  const rest = l.order.filter((k) => k !== layer);
  const below = rest.filter((k) => centre(piece(l, k)!)[1] < y).length;
  return [...rest.slice(0, below), layer, ...rest.slice(below)];
}

/** The drawn order after moving a piece `by` places up (positive) or down. */
export function moveBy(order: number[], layer: number, by: number): number[] {
  const i = order.indexOf(layer);
  const j = Math.max(0, Math.min(order.length - 1, i + by));
  const rest = order.filter((k) => k !== layer);
  return [...rest.slice(0, j), layer, ...rest.slice(j)];
}

export const sameOrder = (a: number[], b: number[]) => a.length === b.length && a.every((x, i) => x === b[i]);

/**
 * Where a generator of the given width dropped at (x, y) goes: the nearest height between rows
 * and the strand position (from 1) of its leftmost input there.
 */
export function dropTarget(l: Layout, x: number, y: number, width: number): { below: number; position: number; y: number; x: number } | undefined {
  if (!l.boundaries.length) return undefined;
  const b = l.boundaries.reduce((best, c) => (Math.abs(c.y - y) < Math.abs(best.y - y) ? c : best));
  const n = b.object.length;
  if (n < width) return undefined;
  const left = Math.max(0, Math.min(n - width, Math.round(x - (width - 1) / 2)));
  return { below: b.below, position: left + 1, y: b.y, x: left };
}

/** Where a generator goes when inserted from the keyboard: just above a piece, on its first output. */
export function insertAbove(l: Layout, p: Piece | undefined, width: number): { below: number; position: number } | undefined {
  if (!p) {
    const top = l.boundaries.at(-1);
    return top && top.object.length >= width ? { below: top.below, position: 1 } : undefined;
  }
  const at = l.order.indexOf(p.layer) + 1;
  const b = l.boundaries.find((c) => c.below >= at) ?? l.boundaries.at(-1)!;
  const n = b.object.length;
  if (n < width) return undefined;
  return { below: b.below, position: Math.max(0, Math.min(n - width, p.top)) + 1 };
}

/** A piece's look-only offset, in units: dots move only along their strand. */
export function offsetFor(p: Piece | undefined, hint: [number, number] | undefined): [number, number] {
  if (!p || !hint) return [0, 0];
  if (p.draw === 'dot') return [0, Math.max(-0.4, Math.min(0.4, hint[1]))];
  return [Math.max(-0.45, Math.min(0.45, hint[0])), Math.max(-0.4, Math.min(0.4, hint[1]))];
}

/** SVG path of a line, with y flipped; a piece moved for looks keeps its ends on its neighbours. */
export function path(line: Line, off: [number, number] = [0, 0]): string {
  const f = (v: number) => +(-v).toFixed(3);
  const n = (v: number) => +v.toFixed(3);
  if (line.kind === 'strand') {
    const [x0, y0, x1, y1] = [line.x0!, line.y0!, line.x1!, line.y1!];
    const ym = (y0 + y1) / 2;
    if (!off[0] && !off[1]) return `M ${n(x0)} ${f(y0)} C ${n(x0)} ${f(ym)} ${n(x1)} ${f(ym)} ${n(x1)} ${f(y1)}`;
    // through the moved middle of the piece
    const mx = (x0 + x1) / 2 + off[0];
    const my = ym + off[1];
    const dx = (x1 - x0) / 4;
    return `M ${n(x0)} ${f(y0)} C ${n(x0)} ${f((y0 + my) / 2)} ${n(mx - dx)} ${f(my - 0.15)} ${n(mx)} ${f(my)} C ${n(mx + dx)} ${f(my + 0.15)} ${n(x1)} ${f((my + y1) / 2)} ${n(x1)} ${f(y1)}`;
  }
  if (line.kind === 'turn') {
    const d = line.up ? -0.6 : 0.6;
    return `M ${n(line.x0!)} ${f(line.y!)} C ${n(line.x0!)} ${f(line.y! + d)} ${n(line.x1!)} ${f(line.y! + d)} ${n(line.x1!)} ${f(line.y!)}`;
  }
  return '';
}
