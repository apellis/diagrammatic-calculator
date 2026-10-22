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

/**
 * The drawn run of a dot's strand through the dot: the strand's segments joined end to end, from
 * the dot's own segment up and down until a turn or the edge. A dragged dot follows this run.
 */
export function track(l: Layout, dot: Piece): Line[] {
  const s = dotStrand(dot);
  const segs = l.lines.filter((x) => x.kind === 'strand' && x.strand === s);
  const own = segs.find((x) => x.layer === dot.layer);
  if (!own) return [];
  const near = (a: number, b: number) => Math.abs(a - b) < 1e-6;
  const run = [own];
  for (let cur = own, next; (next = segs.find((x) => !run.includes(x) && near(x.y0!, cur.y1!) && near(x.x0!, cur.x1!))); cur = next) run.push(next);
  for (let cur = own, next; (next = segs.find((x) => !run.includes(x) && near(x.y1!, cur.y0!) && near(x.x1!, cur.x0!))); cur = next) run.unshift(next);
  return run;
}

/** The lowest and highest heights a dot may be dragged to on its run, kept off the ends. */
export function trackRange(run: Line[]): [number, number] {
  if (!run.length) return [0, 0];
  return [run[0].y0! + 0.12, run[run.length - 1].y1! - 0.12];
}

/**
 * The point of the run at height `y` (clamped to the run), on the drawn curve: each segment is the
 * cubic of `path`, whose height is monotone in its parameter, so a few bisection steps find it.
 */
export function along(run: Line[], y: number): [number, number] {
  if (!run.length) return [0, y];
  const [lo, hi] = trackRange(run);
  const h = Math.max(lo, Math.min(hi, y));
  const seg = run.find((x) => h <= x.y1!) ?? run[run.length - 1];
  const [x0, y0, x1, y1] = [seg.x0!, seg.y0!, seg.x1!, seg.y1!];
  const target = y1 === y0 ? 0 : (h - y0) / (y1 - y0);
  // height fraction 3t(1-t)/2 + t^3 and width fraction 3t^2 - 2t^3 of the cubic in `path`
  let [a, b] = [0, 1];
  for (let i = 0; i < 24; i++) {
    const t = (a + b) / 2;
    if (1.5 * t * (1 - t) + t * t * t < target) a = t;
    else b = t;
  }
  const t = (a + b) / 2;
  return [x0 + (x1 - x0) * (3 * t * t - 2 * t * t * t), h];
}

/** The crossings a dot's strand passes through, bottom to top. */
export function crossingsOn(l: Layout, dot: Piece): Piece[] {
  const s = dotStrand(dot);
  return l.pieces.filter((p) => p.draw === 'crossing' && s !== undefined && p.inputs.includes(s)).sort((a, b) => centre(a)[1] - centre(b)[1]);
}

/**
 * The Reidemeister-like move a sideways drag of a crossing can make: among the relation matches of
 * a summand, one whose matched pieces are all crossings and include this one. A triangle of three
 * crossings with this one in the middle (a braid relation) is preferred, and is made by dragging
 * the middle crossing towards the other two (`side`, −1 for left); a stacked pair (a square
 * relation) is pulled apart in either direction (`side` 0).
 */
export function pullMove(l: Layout, matches: { occurrence: number[] }[], layer: number): { index: number; side: -1 | 0 | 1; layers: number[] } | undefined {
  const crossing = (k: number) => piece(l, k)?.draw === 'crossing';
  const found = matches
    .map((m, index) => ({ m, index }))
    .filter(({ m }) => m.occurrence.includes(layer) && m.occurrence.length >= 2 && m.occurrence.length <= 3 && m.occurrence.every(crossing));
  const triangle = found.find(({ m }) => m.occurrence.length === 3 && m.occurrence[1] === layer);
  if (triangle) {
    const x = centre(piece(l, layer)!)[0];
    const others = triangle.m.occurrence.filter((k) => k !== layer).map((k) => centre(piece(l, k)!)[0]);
    const side = Math.sign(others.reduce((a, b) => a + b, 0) / others.length - x) as -1 | 0 | 1;
    return { index: triangle.index, side, layers: triangle.m.occurrence };
  }
  const pair = found.find(({ m }) => m.occurrence.length === 2);
  return pair && { index: pair.index, side: 0, layers: pair.m.occurrence };
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
    // a moved piece moves the middle of its curve: both control points shift, so the ends stay put
    // and the curve stays smooth (its middle moves by three quarters of the offset)
    const [ox, oy] = off;
    return `M ${n(x0)} ${f(y0)} C ${n(x0 + ox)} ${f(ym + oy)} ${n(x1 + ox)} ${f(ym + oy)} ${n(x1)} ${f(y1)}`;
  }
  if (line.kind === 'turn') {
    const d = line.up ? -0.6 : 0.6;
    return `M ${n(line.x0!)} ${f(line.y!)} C ${n(line.x0!)} ${f(line.y! + d)} ${n(line.x1!)} ${f(line.y! + d)} ${n(line.x1!)} ${f(line.y!)}`;
  }
  return '';
}

/** SVG points of an arrowhead line (y flipped), as the core draws it; `scale` enlarges it. */
export function arrowPoints(line: Line, scale = 1): string {
  const [x, y, dx, dy] = [line.x!, line.y!, line.dx ?? 0, line.dy ?? 1];
  const [l, h] = [0.13 * scale, 0.1 * scale];
  const pts = [
    [x + dx * l, y + dy * l],
    [x - dx * l - dy * h, y - dy * l + dx * h],
    [x - dx * l + dy * h, y - dy * l - dx * h],
  ];
  return pts.map(([a, b]) => `${+a.toFixed(3)},${+(-b).toFixed(3)}`).join(' ');
}
