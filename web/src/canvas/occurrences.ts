// Generator-occurrence IDs (docs/spec.md, IDs; docs/design.md, Occurrence IDs): a letter for how
// a piece is drawn and a number counted through the whole document in creation order (d3, c12).
// The document stores them per summand word; a gesture carries them to its result through the
// provenance map the engine returns, and a summand drawn without them takes them from the
// summands drawn before (`term.diff`) or gets fresh ones. Strands (s1, s2, …) are numbered the
// same way, each by where it starts: a point of the cell's bottom edge, or the piece it starts at.
import type { Layout } from './geometry';

/** The letter of an occurrence ID by how a piece is drawn; g for any other piece. */
const LETTERS: Record<string, string> = { dot: 'd', crossing: 'c', cup: 'u', cap: 'n', bubble: 'b' };
export const letterOf = (draw?: string): string => LETTERS[draw ?? ''] ?? 'g';

/** One summand of a provenance map: its word and, per canonical layer, the occurrences `[summand, layer]` it descends from. */
export interface ProvenanceSummand {
  word: string;
  layers: { generator: string; from: number[][] }[];
}

/** The letters of a drawn summand's layers, by canonical layer from the bottom. */
export function lettersOf(l: Layout): string[] {
  const out = new Array<string>(l.order.length).fill('g');
  for (const pc of l.pieces) out[pc.layer - 1] = letterOf(pc.draw);
  return out;
}

/** Whether stored IDs fit a drawn summand: one per layer, each of the layer's kind. */
export function fits(ids: string[] | undefined, letters: string[]): ids is string[] {
  return !!ids && ids.length === letters.length && ids.every((id, i) => id.startsWith(letters[i]) && /^[a-z][0-9]+$/.test(id));
}

const number = (id: string) => Number(id.slice(1));

/**
 * The IDs of a result's summands (by word), carried by its provenance map: a layer keeps the ID
 * of the occurrence it descends from (the oldest, if several were added together, and never one
 * its summand already uses); a created layer, or one whose origin had no ID, gets a fresh one.
 * `old(k)` gives the IDs of summand k (1-based) of the starting term, if it has any; `draw` the
 * drawing kind of a generator. Only the first `limit` summands are given IDs here; the others get
 * theirs when first drawn.
 */
export function carry(
  provenance: ProvenanceSummand[],
  old: (k: number) => string[] | undefined,
  draw: (generator: string) => string | undefined,
  fresh: (letter: string) => string,
  limit = 200,
): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const s of provenance.slice(0, limit)) {
    const used = new Set<string>();
    const ids = s.layers.map((layer) => {
      const letter = letterOf(draw(layer.generator));
      const inherited = layer.from
        .map(([k, i]) => old(k)?.[i - 1])
        .filter((id): id is string => !!id && id.startsWith(letter) && !used.has(id))
        .sort((a, b) => number(a) - number(b))[0];
      const id = inherited ?? fresh(letter);
      used.add(id);
      return id;
    });
    out[s.word] = ids;
  }
  return out;
}

/**
 * Where each strand of a drawn summand starts, as the key its ID is stored under: `bottom k` for
 * a strand that starts at the k-th point (from 1, left to right) of the bottom edge (the leftmost,
 * if it has several), else `on ID` for the piece with the lowest canonical layer on it (a cup, or
 * a bubble), by that piece's occurrence ID; undefined while that piece has none. Rewrites keep a
 * term's bottom edge, and a piece's ID follows it, so a strand keeps its ID with them.
 */
export function strandAnchors(l: Layout, ids: Map<number, string>): (string | undefined)[] {
  const out = new Map<number, string | undefined>();
  const y0 = Math.min(0, ...l.boundaries.map((b) => b.y));
  const at = (y?: number) => y !== undefined && Math.abs(y - y0) < 1e-9;
  const starts: { x: number; strand: number }[] = [];
  for (const line of l.lines) {
    if (line.kind !== 'strand' || line.strand === undefined) continue;
    if (at(line.y0)) starts.push({ x: line.x0!, strand: line.strand });
    else if (at(line.y1)) starts.push({ x: line.x1!, strand: line.strand });
  }
  starts.sort((a, b) => a.x - b.x);
  starts.forEach(({ strand }, k) => {
    if (!out.has(strand)) out.set(strand, `bottom ${k + 1}`);
  });
  for (const pc of [...l.pieces].sort((a, b) => a.layer - b.layer)) {
    for (const s of [...pc.inputs, ...pc.outputs]) {
      if (!out.has(s)) out.set(s, ids.has(pc.layer) ? `on ${ids.get(pc.layer)}` : undefined);
    }
  }
  return l.strands.map((_, s) => out.get(s));
}

/** The IDs of a drawn summand by canonical layer, as the canvas shows them. */
export function idMap(ids: string[] | undefined): Map<number, string> {
  return new Map((ids ?? []).map((id, i) => [i + 1, id]));
}
