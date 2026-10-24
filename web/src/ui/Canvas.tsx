// The focus canvas: one cell's term drawn full size, edited by gestures.
//
// The canvas edits a definition as written; watches and viewer canvases are inspection-only.
// Rewrites by relations, slides, and interchanges are equalities, so the cell's value stays
// the same while its text changes;
// inserting or removing a piece changes the value. Every gesture is one undoable step of the
// document, and positions moved only for looks are layout hints, never part of the term.
import { batch, createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import type { Doc, Hint } from '../doc/model';
import type { Calls } from '../eval/evaluator';
import type { RelationsMatchResponse } from '../engine/ops.gen';
import { isMac, mod } from '../app';
import { Typeset } from './Typeset';
import { isAction, keysOf } from './shortcuts';
import { ValueBadge } from './Results';
import { Dialog } from './Dialog';
import { patterns } from './theme';
import type { CellValue } from '../eval/evaluator';
import {
  along, arrowPoints, bounds, centre, crossingsOn, crossingsPassed, dropTarget, insertAbove, moveBy, moveTo, nextOnStrand,
  offsetFor, onStrand, path, piece, piecesIn, pullMove, rectangle, sameOrder, track, type Layout, type Line, type Piece, type Rect,
} from '../canvas/geometry';
import { carry, fits, idMap, lettersOf, strandAnchors, type ProvenanceSummand } from '../canvas/occurrences';
import { strandKey } from '../doc/model';

export interface Generator {
  name: string;
  draw: string;
  inputs: number;
  outputs: number;
  /** The name typed and printed, and the strands entering and leaving it (`a* b`), as the presentation writes them. */
  written?: string;
  source?: string;
  target?: string;
}

/** A source or target pattern's strands, left to right: colour and orientation. */
const strandsOf = (p = '') => p.split(/\s+/).filter(Boolean).map((w) => ({ colour: w.replace(/\*$/, ''), up: !w.endsWith('*') }));

/**
 * Which way a generator's strands run, in words: a cup or cap is rightward when its strand runs
 * left to right along it.
 */
export function orientation(g: Generator): string {
  const [src, tgt] = [strandsOf(g.source), strandsOf(g.target)];
  if (g.draw === 'cup' && tgt.length === 2) return tgt[0].up ? 'leftward' : 'rightward';
  if (g.draw === 'cap' && src.length === 2) return src[0].up ? 'rightward' : 'leftward';
  if (!src.length) return '';
  if (src.every((l) => l.up)) return src.length > 1 ? 'upward strands' : 'upward strand';
  if (src.every((l) => !l.up)) return src.length > 1 ? 'downward strands' : 'downward strand';
  return src.map((l) => (l.up ? 'up' : 'down')).join(' and ');
}

/** A generator's tooltip line: "cup, rightward: 1 → a* a". */
export function describeGenerator(g: Generator): string {
  const o = orientation(g);
  const kind = g.draw === 'dot' ? `dot on ${o.startsWith('up') ? 'an' : 'a'} ${o}` : g.draw === 'crossing' ? `crossing of ${o}` : o ? `${g.draw}, ${o}` : g.draw;
  const name = g.written && g.written !== g.name ? `${g.written} (${g.name})` : g.name;
  return g.source === undefined ? name : `${name}: ${kind}, ${g.source || '1'} → ${g.target || '1'}`;
}

/** What is selected, as the AI receives it: references resolved when a message is sent. */
export interface Selection {
  cellId: string;
  summand: number;
  word: string;
  text: string;
  layers: number[];
  rectangle?: Rect;
}

export interface CanvasProps {
  doc: Doc;
  readOnly?: boolean;
  remoteSelections?: { name: string; colour: string; selection: Pick<Selection, 'cellId' | 'word' | 'layers'> }[];
  engine: Calls;
  cellId: string;
  title: string;
  /** The context, with the workspace's scalars. */
  context: string;
  /** The term drawn; null while it is being computed. */
  term: string | null;
  /** The term uses definitions: a gesture writes it out in full. */
  expanded: boolean;
  generators: Generator[];
  colours: string[];
  hintsVersion: number;
  initialWord?: string;
  /** Writes the term; `ids` gives the occurrence IDs of its summands, stored in the same step. */
  onWrite: (term: string, note: string, ids?: (fresh: (letter: string) => string) => Record<string, string[]>) => void;
  onClose: () => void;
  onSelection?: (s: Selection | null) => void;
  onAsk?: () => void;
  /** The cell's computed value, whose provenance the canvas shows. */
  value?: CellValue;
}

type Match = Extract<RelationsMatchResponse, { status: 'ok' }>['matches'][number];

const U = 44; // pixels per unit
const SHOWN = 60;
/** Strand colours, shared with the drawings in cells (styles.css, `--strand-0` …). */
const STRANDS = 8;
const DASHES = ['', '0.18 0.1', '0.05 0.1', '0.3 0.08 0.06 0.08', '0.12 0.12', '0.4 0.1'];
const K = isMac ? '⌘' : 'Ctrl+';


export function Canvas(props: CanvasProps) {
  const [layouts, setLayouts] = createSignal<Layout[]>([]);
  const [total, setTotal] = createSignal(0);
  const [error, setError] = createSignal('');
  const [message, setMessage] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  /**
   * The selection. `kept`: built up piece by piece (X, ⌘- or Ctrl-click, a box, S, ⌘A or Ctrl+A), so Tab moves the
   * keyboard ring (`cursor`) without replacing it; otherwise the selection follows the ring.
   */
  const [sel, setSel] = createSignal<{ summand: number; layers: number[]; strand?: number; kept?: boolean }>({ summand: 0, layers: [] });
  /** The piece the keyboard is on (Tab, ⇧Tab): drawn as a dashed ring while it is not simply the selection. */
  const [cursor, setCursor] = createSignal<{ k: number; layer: number } | null>(null);
  /** Occurrence IDs (d1, c2, s1, …) written beside the pieces and strands. */
  const [labels, setLabels] = createSignal(false);
  /** Bumped when the document's occurrence IDs change, so the IDs drawn follow them. */
  const [occVersion, setOccVersion] = createSignal(0);
  const onOccurrences = () => setOccVersion(occVersion() + 1);
  props.doc.occurrences.observe(onOccurrences);
  onCleanup(() => props.doc.occurrences.unobserve(onOccurrences));
  /** The occurrence IDs of a drawn summand, by canonical layer. */
  const idsOf = (l?: Layout): Map<number, string> => {
    occVersion();
    if (!l) return new Map();
    const ids = props.doc.occurrenceIds(props.cellId, l.word);
    return fits(ids, lettersOf(l)) ? idMap(ids) : new Map();
  };
  /** The IDs of a drawn summand's strands, by strand number. */
  const strandIdsOf = (l?: Layout): Map<number, string> => {
    const out = new Map<number, string>();
    if (!l) return out;
    strandAnchors(l, idsOf(l)).forEach((a, s) => {
      const id = a && props.doc.strandId(props.cellId, a);
      if (id) out.set(s, id);
    });
    return out;
  };
  const drawOf = (name: string) => props.generators.find((g) => g.name === name)?.draw;
  /**
   * Gives the summands drawn for the first time their occurrence IDs, and records the words drawn.
   * A summand written in place of others (text typed by hand, a term the AI wrote, a definition it
   * uses changed) takes them from the summands drawn before, by `term.diff`; one with nothing to
   * take from gets fresh ones. Then each strand without an ID gets one, by where it starts.
   */
  const giveIds = async (gen: number, term: string, context: string, summands: Layout[]) => {
    if (props.readOnly) return;
    const doc = props.doc;
    const cell = props.cellId;
    const words = summands.map((l) => l.word);
    const missing = summands.filter((l, i) => words.indexOf(l.word) === i && !fits(doc.occurrenceIds(cell, l.word), lettersOf(l)));
    const before = (doc.drawnWords(cell) ?? []).filter((w) => doc.occurrenceIds(cell, w));
    let diff: { before: string[]; provenance: ProvenanceSummand[] } | undefined;
    if (missing.length && before.some((w) => !words.includes(w))) {
      const o = await props.engine.call('term.diff', { context, term, before: before.join(' + ') }, { lane: 'interactive' });
      if (gen !== generation) return;
      if (o.status === 'done' && o.response.status === 'ok') diff = o.response;
    }
    if (props.readOnly) return;
    const d = diff;
    doc.recordDrawn(cell, (fresh) => {
      const want = new Set(missing.map((l) => l.word));
      const out: Record<string, string[]> = d
        ? carry(d.provenance.filter((p) => want.has(p.word)), (k) => doc.occurrenceIds(cell, d.before[k - 1]), drawOf, fresh)
        : {};
      for (const l of missing) if (!fits(out[l.word], lettersOf(l))) out[l.word] = lettersOf(l).map(fresh);
      for (const l of summands) {
        const ids = idMap(out[l.word] ?? doc.occurrenceIds(cell, l.word));
        for (const a of strandAnchors(l, ids)) {
          if (a && !doc.strandId(cell, a) && !out[strandKey(a)]) out[strandKey(a)] = [fresh('s')];
        }
      }
      return out;
    }, words);
  };
  const [fresh, setFresh] = createSignal(new Set<string>());
  const [menu, setMenu] = createSignal<{ summand: number; rect?: Rect; matches: Match[]; previews: (Layout[] | null)[] } | null>(null);
  const [inserting, setInserting] = createSignal(false);
  const [help, setHelp] = createSignal(false);
  /** The board's pan (pixels) and zoom; every change is clamped by `keepInView`. */
  const [view, setViewRaw] = createSignal({ x: 24, y: 16, z: 1 });
  const setView = (v: { x: number; y: number; z: number }) => setViewRaw(keepInView(v));
  const [ghost, setGhost] = createSignal<{ summand: number; layer: number; dx: number; dy: number } | null>(null);
  const [boxDrag, setBoxDrag] = createSignal<{ summand: number; a: [number, number]; b: [number, number] } | null>(null);
  const [drop, setDrop] = createSignal<{ summand: number; x: number; y: number; width: number } | null>(null);
  /** Where a click on a strand put the insertion point: the next palette click or I-digit inserts there. */
  const [mark, setMark] = createSignal<{ summand: number; x: number; y: number } | null>(null);
  /**
   * A dot being dragged, drawn where it is on its strand: the crossings it has passed (which fire
   * on release) and the one it is nearing, with how near (0 to 1). After release it stays where it
   * was let go (`held`) until the rewritten term is drawn, or eases back when nothing changes.
   */
  /** A crossing being dragged up or down: its summand drawn with the order it would get. */
  const [preview, setPreview] = createSignal<{ summand: number; order: number[]; layout: Layout; held?: boolean } | null>(null);
  /** A crossing being dragged sideways: the crossings of the move it would make, and that move's result. */
  const [pull, setPull] = createSignal<{ summand: number; layer: number; dx: number; dy?: number; layers: number[]; armed: boolean; result?: Layout[] } | null>(null);
  const [live, setLive] = createSignal<{ summand: number; layer: number; x: number; y: number; passed: number[]; near?: { layer: number; w: number }; held?: boolean } | null>(null);
  let root!: HTMLDivElement;
  let board!: HTMLDivElement;
  let viewport!: HTMLDivElement;
  // Summands fill rows as wide as the visible canvas (at zoom 1), so new ones go below the
  // others once a row is full instead of always to the right.
  const [rowWidth, setRowWidth] = createSignal(2400);
  let space = false;
  /** The summands before a gesture's write, to outline the ones it adds once the new term arrives. */
  let pending: { term: string | null; words: Set<string> } | null = null;
  let firstSelect = props.initialWord;

  // laying out the term, with the stored orders of its summands
  let generation = 0;
  const term = createMemo(() => props.term);
  const context = createMemo(() => props.context);
  const hintsVersion = createMemo(() => props.hintsVersion);
  createEffect(on(() => [term(), context(), hintsVersion()] as const, async ([term, context]) => {
    const gen = ++generation;
    if (term === null) return;
    const first = await props.engine.call('term.layout', { context, term, summands: SHOWN }, { lane: 'interactive' });
    if (gen !== generation || first.status !== 'done') return;
    const r = first.response;
    if (r.status !== 'ok') {
      setError(r.message);
      setLayouts([]);
      return;
    }
    let summands = r.summands;
    const orders: Record<string, number[]> = {};
    summands.forEach((l, k) => {
      const o = props.doc.hint(props.cellId, l.word)?.order;
      if (o && o.length === l.order.length) orders[k + 1] = o;
    });
    if (Object.keys(orders).length) {
      const again = await props.engine.call('term.layout', { context, term, summands: SHOWN, orders }, { lane: 'interactive' });
      if (gen !== generation) return;
      if (again.status === 'done' && again.response.status === 'ok') summands = again.response.summands;
    }
    batch(() => {
      setError('');
      setTotal(r.terms);
      setLayouts(summands);
      if (pending && pending.term !== term) {
        const before = pending.words;
        const added = new Set(summands.map((l) => l.word).filter((w) => !before.has(w)));
        pending = null;
        setFresh(added);
        setTimeout(() => setFresh(new Set<string>()), 2000);
      }
      if (live()?.held) setLive(null);
      if (preview()?.held) setPreview(null);
      setMark(null);
      if (firstSelect) {
        const k = summands.findIndex((l) => l.word === firstSelect);
        if (k >= 0) setSel({ summand: k, layers: [] });
        firstSelect = undefined;
      }
      if (sel().summand >= summands.length) setSel({ summand: 0, layers: [] });
    });
    await giveIds(gen, term, context, summands);
  }));

  createEffect(() => {
    const s = sel();
    const l = layouts()[s.summand];
    props.onSelection?.(l ? { cellId: props.cellId, summand: s.summand + 1, word: l.word, text: l.text, layers: s.layers, rectangle: rectangle(l, s.layers) } : null);
  });
  onCleanup(() => props.onSelection?.(null));

  const colourOf = (name: string) => {
    const i = props.colours.indexOf(name.replace(/\*$/, ''));
    return `var(--strand-${(i < 0 ? 0 : i) % STRANDS})`;
  };
  const dashOf = (name: string) => {
    if (!patterns()) return undefined;
    const i = props.colours.indexOf(name.replace(/\*$/, ''));
    return DASHES[(i < 0 ? 0 : i) % DASHES.length] || undefined;
  };
  const hintOf = (l: Layout): Hint | undefined => props.doc.hint(props.cellId, l.word);

  // gestures --------------------------------------------------------------------------------

  const say = (m: string) => setMessage(m);
  const run = async <T,>(f: () => Promise<T>) => {
    if (props.readOnly) return;
    setBusy(true);
    try {
      return await f();
    } finally {
      setBusy(false);
    }
  };
  /**
   * Writes a new term to the cell as one step, outlining the summands it adds. Its provenance map
   * carries the occurrence IDs of the summands drawn now to the new summands.
   */
  const write = (term: string, note: string, provenance?: ProvenanceSummand[]) => {
    if (props.readOnly) return;
    changed = true;
    const before = layouts();
    pending = { term: props.term, words: new Set(before.map((l) => l.word)) };
    setCursor(null);
    setSel({ summand: sel().summand, layers: [] });
    const old = (k: number) => {
      const l = before[k - 1];
      const ids = l && props.doc.occurrenceIds(props.cellId, l.word);
      return l && fits(ids, lettersOf(l)) ? ids : undefined;
    };
    props.onWrite(term, note, provenance && ((fresh) => carry(provenance, old, drawOf, fresh)));
  };
  const terms = (t: string) => {
    const n = t === '0' ? 0 : t.split(/ [+-] /).length;
    return `${n} ${n === 1 ? 'summand' : 'summands'}`;
  };

  const slide = (k: number, dot: Piece, direction: 'up' | 'down', steps: number) =>
    run(async () => {
      const o = await props.engine.call('slide', { context: props.context, term: props.term!, summand: k + 1, layer: dot.layer, direction, steps }, { lane: 'interactive' });
      if (props.readOnly || o.status !== 'done') return;
      const r = o.response;
      if (r.status !== 'ok') return say(r.message);
      if (!r.steps.length) return say('Nothing to slide through there.');
      write(r.term, `slide a dot ${direction} through ${r.steps.length} crossing${r.steps.length > 1 ? 's' : ''}`, r.provenance);
      say(`Slid the dot ${direction} through ${r.steps.length} crossing${r.steps.length > 1 ? 's' : ''} by ${[...new Set(r.steps.map((s) => s.relation))].join(', ')}; the term now has ${terms(r.term)}.`);
    });

  const reorder = (k: number, order: number[], note: string) =>
    run(async () => {
      const l = layouts()[k];
      if (!l || sameOrder(order, l.order)) return;
      const o = await props.engine.call('term.layout', { context: props.context, term: props.term!, summands: k + 1, orders: { [k + 1]: order } }, { lane: 'interactive' });
      if (props.readOnly || o.status !== 'done') return;
      const r = o.response;
      if (r.status !== 'ok') return say(r.message);
      const moved = r.summands[k];
      const h = hintOf(l) ?? {};
      const canonical = order.every((x, i) => x === i + 1);
      changed = true;
      if (!props.readOnly) props.doc.setHint(props.cellId, l.word, { ...h, order: canonical ? undefined : order }, note);
      say(moved.sign !== l.sign ? 'Interchange of two odd pieces: the coefficient of this summand changes sign (−1).' : 'Interchanged; the coefficient is unchanged.');
    });

  const lookMove = (k: number, layer: number, d: [number, number]) => {
    if (props.readOnly) return;
    const l = layouts()[k];
    const p = l && piece(l, layer);
    if (!p) return;
    const h = hintOf(l) ?? {};
    const old = h.offsets?.[layer] ?? [0, 0];
    const next = offsetFor(p, [old[0] + d[0], old[1] + d[1]]);
    const offsets = { ...(h.offsets ?? {}) };
    if (Math.abs(next[0]) < 0.02 && Math.abs(next[1]) < 0.02) delete offsets[layer];
    else offsets[layer] = next;
    if (!props.readOnly) props.doc.setHint(props.cellId, l.word, { ...h, offsets }, 'move a piece for looks');
  };

  const openMenu = (k: number) =>
    run(async () => {
      const l = layouts()[k];
      if (!l) return;
      const rect = sel().summand === k ? rectangle(l, sel().layers) : undefined;
      const o = await props.engine.call('relations.match', { context: props.context, term: props.term!, summand: k + 1, ...(rect ? { rectangle: rect } : {}) }, { lane: 'interactive' });
      if (props.readOnly || o.status !== 'done') return;
      const r = o.response;
      if (r.status !== 'ok') return say(r.message);
      if (!r.matches.length) return say(rect ? 'No relation applies to the selection.' : 'No relation applies to this summand.');
      setMenu({ summand: k, rect, matches: r.matches, previews: r.matches.map(() => null) });
      // thumbnails of the results, nine at most
      r.matches.slice(0, 9).forEach(async (m, i) => {
        if (!m.preview) return;
        const p = await props.engine.call('term.layout', { context: props.context, term: m.preview, summands: 4 }, { lane: 'interactive' });
        const cur = menu();
        if (!cur || cur.matches !== r.matches || p.status !== 'done' || p.response.status !== 'ok') return;
        const previews = [...cur.previews];
        previews[i] = p.response.summands;
        setMenu({ ...cur, previews });
      });
    });

  const apply = (i: number) => {
    const m = menu();
    if (!m || !m.matches[i]) return;
    setMenu(null);
    return applyMatch(m.summand, m.matches[i], i, m.rect);
  };
  /** Applies match `i` of the summand (in its rectangle, if one was given) and writes the result. */
  const applyMatch = (k: number, match: Match, i: number, rect?: Rect) =>
    run(async () => {
      const o = await props.engine.call('relation.apply', { context: props.context, term: props.term!, summand: k + 1, ...(rect ? { rectangle: rect } : {}), match: i }, { lane: 'interactive' });
      if (props.readOnly || o.status !== 'done') return;
      const r = o.response;
      if (r.status !== 'ok') return say(r.message);
      write(r.term, `apply ${match.relation}`, r.provenance);
      say(`Applied ${match.relation} (${match.direction})${match.sign === -1 ? ', with sign −1 from interchanges' : ''}; the term now has ${terms(r.term)}.`);
    });

  const edit = (k: number, req: { action: 'insert'; generator: string; below: number; position: number } | { action: 'remove'; layers: number[] }, note: string) =>
    run(async () => {
      const l = layouts()[k];
      if (!l) return;
      const order = hintOf(l)?.order;
      const o = await props.engine.call('term.edit', { context: props.context, term: props.term!, summand: k + 1, ...(order ? { order } : {}), ...req }, { lane: 'interactive' });
      if (props.readOnly || o.status !== 'done') return;
      const r = o.response;
      if (r.status !== 'ok') return say(r.message);
      write(r.term, note, r.provenance);
      say(`${note[0].toUpperCase()}${note.slice(1)}: this changes the element, unlike a rewrite.`);
    });

  const removeSelected = () => {
    const s = sel();
    const l = layouts()[s.summand];
    if (!l || !s.layers.length) return say('Select pieces to remove them: click one, or drag a box on empty space.');
    const n = s.layers.length;
    const note = n === 1 ? `remove ${piece(l, s.layers[0])?.generator ?? 'a piece'}` : `remove ${n} pieces`;
    void edit(s.summand, { action: 'remove', layers: s.layers }, note);
  };
  const insertGenerator = (g: Generator, k = sel().summand, at?: { below: number; position: number }) => {
    const l = layouts()[k];
    if (!l) return;
    const width = Math.max(g.inputs, 1);
    const m = mark();
    const one = sel().summand === k && sel().layers.length === 1 ? piece(l, sel().layers[0]) : undefined;
    const target = at ?? (m && m.summand === k && !one ? dropTarget(l, m.x, m.y, width) : insertAbove(l, one, width));
    if (!target) return say(`${g.name} does not fit there.`);
    void edit(k, { action: 'insert', generator: g.name, ...target }, `insert ${g.name}`);
  };

  // pointer input ---------------------------------------------------------------------------

  const units = (svg: SVGSVGElement, e: { clientX: number; clientY: number }): [number, number] => {
    const m = svg.getScreenCTM();
    if (!m) return [0, 0];
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    return [p.x, -p.y];
  };

  let drag: {
    k: number; layer?: number; start: [number, number]; alt: boolean; moved: boolean; svg: SVGSVGElement; additive: boolean;
    /** Pressed on a strand (not on a piece): a click there sets the insertion point. */
    strand?: boolean;
    /** For a dot: the run of its strand and the crossings on it. */
    run?: Line[]; crossings?: Piece[];
    /** For another piece: up and down ('v') or sideways ('h'), fixed by the first movement. */
    mode?: 'v' | 'h';
    /** The summand's relation matches, and the sideways move this piece can make. */
    matches?: Match[]; move?: ReturnType<typeof pullMove>; resultAsked?: boolean;
    /** The drawn order last asked for while dragging up or down. */
    asked?: number[];
    /** The pointer's last offset from the start, kept while a sideways drag waits for the matches. */
    last?: [number, number];
  } | null = null;
  let pan: { x: number; y: number; vx: number; vy: number } | null = null;

  // Permission or cell-kind changes also cancel an in-flight editing gesture.
  createEffect(on(() => props.readOnly, (readOnly) => {
    if (!readOnly) return;
    drag = null;
    setGhost(null);
    setLive(null);
    setPreview(null);
    setPull(null);
    setDrop(null);
    setMark(null);
    setBoxDrag(null);
    setMenu(null);
    setInserting(false);
    say('');
  }));

  const onDown = (k: number, e: PointerEvent) => {
    root.focus({ preventScroll: true });
    const svg = e.currentTarget as SVGSVGElement;
    if (e.button === 1 || space) {
      pan = { x: e.clientX, y: e.clientY, vx: view().x, vy: view().y };
      svg.setPointerCapture(e.pointerId);
      return;
    }
    if (e.button !== 0) return;
    const t = (e.target as Element).closest('[data-layer]');
    drag = { k, layer: t ? Number(t.getAttribute('data-layer')) : undefined, start: units(svg, e), alt: e.altKey, moved: false, svg, additive: mod(e as unknown as KeyboardEvent),
      strand: !t && !!(e.target as Element).closest('[data-strand]') };
    const l = layouts()[k];
    const pc = drag.layer !== undefined && l ? piece(l, drag.layer) : undefined;
    if (!props.readOnly && pc?.draw === 'dot' && !e.altKey) {
      drag.run = track(l, pc);
      drag.crossings = crossingsOn(l, pc);
    } else if (!props.readOnly && pc && !e.altKey) {
      const d = drag;
      void props.engine.call('relations.match', { context: props.context, term: props.term!, summand: k + 1 }, { lane: 'interactive' }).then((o) => {
        if (o.status !== 'done' || o.response.status !== 'ok') return;
        d.matches = o.response.matches;
        d.move = pullMove(l, d.matches, pc.layer);
        // a sideways drag that got ahead of the answer goes on from where the pointer is now,
        // without waiting for it to move again
        if (drag === d && !d.mode && d.last) {
          const [dx, dy] = d.last;
          d.mode = Math.abs(dx) > Math.abs(dy) ? 'h' : 'v';
          if (d.mode === 'h') pullSideways(d, dx);
          else moveUpDown(d, dy);
        }
      });
    }
    svg.setPointerCapture(e.pointerId);
    setMenu(null);
  };
  const onMove = (e: PointerEvent) => {
    if (pan) {
      setView({ ...view(), x: pan.vx + e.clientX - pan.x, y: pan.vy + e.clientY - pan.y });
      return;
    }
    if (!drag) return;
    const p = units(drag.svg, e);
    const [dx, dy] = [p[0] - drag.start[0], p[1] - drag.start[1]];
    if (!drag.moved && Math.hypot(dx, dy) < 0.12) return;
    drag.moved = true;
    if (props.readOnly && drag.layer !== undefined) return;
    if (drag.layer !== undefined && drag.run?.length) {
      const l = layouts()[drag.k];
      const pc = piece(l, drag.layer)!;
      const y0 = centre(pc)[1];
      const [x, y] = along(drag.run, y0 + dy);
      const up = y > y0;
      const passed = drag.crossings!.filter((c) => (up ? centre(c)[1] > y0 && centre(c)[1] < y : centre(c)[1] < y0 && centre(c)[1] > y)).map((c) => c.layer);
      const ahead = drag.crossings!
        .filter((c) => !passed.includes(c.layer))
        .map((c) => ({ layer: c.layer, d: Math.abs(centre(c)[1] - y) }))
        .sort((a, b) => a.d - b.d)[0];
      const near = ahead && ahead.d < REACH ? { layer: ahead.layer, w: 1 - ahead.d / REACH } : undefined;
      setLive({ summand: drag.k, layer: drag.layer, x, y, passed, near });
      const n = passed.length;
      say(n ? `Let go to slide the dot ${up ? 'up' : 'down'} through ${n} crossing${n > 1 ? 's' : ''}.` : near ? 'Keep going to pass the crossing.' : 'Sliding the dot along its strand; it settles where you let go.');
    } else if (drag.layer !== undefined && drag.alt) {
      const pc = piece(layouts()[drag.k], drag.layer);
      setGhost({ summand: drag.k, layer: drag.layer, dx: pc?.draw !== 'dot' ? dx : 0, dy });
    } else if (drag.layer !== undefined) {
      if (!drag.mode) {
        // sideways makes a move of crossings; wait for the matches before deciding
        drag.last = [dx, dy];
        if (Math.abs(dx) > Math.abs(dy) && drag.matches === undefined) return;
        drag.mode = Math.abs(dx) > Math.abs(dy) ? 'h' : 'v';
      }
      if (drag.mode === 'h') pullSideways(drag, dx);
      else moveUpDown(drag, dy);
    } else {
      setBoxDrag({ summand: drag.k, a: drag.start, b: p });
    }
  };
  const onUp = (e: PointerEvent) => {
    if (pan) {
      pan = null;
      return;
    }
    const d = drag;
    drag = null;
    setGhost(null);
    setBoxDrag(null);
    if (!d) return;
    if (props.readOnly && d.layer !== undefined && d.moved) return;
    if (!d.moved && live()) setLive(null);
    const pulled = pull();
    setPull(null);
    if (d.mode === 'h') {
      // a drag with no move keeps its explanation up
      if (pulled?.armed && d.move && d.matches) void applyMatch(d.k, d.matches[d.move.index], d.move.index);
      else if (d.move) say('');
      return;
    }
    if (d.mode === 'v') {
      const pv = preview();
      const l0 = layouts()[d.k];
      if (!pv || pv.summand !== d.k || !l0 || sameOrder(pv.order, l0.order)) {
        setPreview(null);
        return;
      }
      setPreview({ ...pv, held: true });
      return void reorder(d.k, pv.order, 'interchange').finally(() => setTimeout(() => preview()?.held && setPreview(null), 1500));
    }
    const l = layouts()[d.k];
    if (!l) return;
    const p = units(d.svg, e);
    if (d.layer === undefined) {
      if (!d.moved) {
        if (!d.additive) setSel({ summand: d.k, layers: [] });
        if (!props.readOnly && d.strand && !d.additive) {
          setMark({ summand: d.k, x: d.start[0], y: d.start[1] });
          return say(`Insertion point set on this strand: click a generator in the palette, or press I and its number, to insert it here${props.generators.some((g) => g.inputs > 1) ? ' (a crossing takes this strand and the next one to the right)' : ''}.`);
        }
        setMark(null);
        return;
      }
      setMark(null);
      const inside = piecesIn(l, d.start, p);
      const base = d.additive && sel().summand === d.k ? sel().layers : [];
      setSel({ summand: d.k, layers: [...new Set([...base, ...inside])], kept: true });
      const n = sel().layers.length;
      say(n ? `Selected ${n} piece${n > 1 ? 's' : ''}. ${props.readOnly ? '' : `R: the relations that apply inside the selection; Delete: remove ${n > 1 ? 'them' : 'it'}; `}${keysOf('ask')}: ask the AI about ${n > 1 ? 'them' : 'it'}.` : 'Nothing inside the box.');
      return;
    }
    const pc = piece(l, d.layer);
    if (!pc) return;
    if (!d.moved) {
      setMark(null);
      const s = sel();
      setCursor({ k: d.k, layer: d.layer });
      if (d.additive && s.summand === d.k) {
        const has = s.layers.includes(d.layer);
        setSel({ summand: d.k, layers: has ? s.layers.filter((x) => x !== d.layer) : [...s.layers, d.layer], kept: true });
      } else setSel({ summand: d.k, layers: [d.layer] });
      return;
    }
    const [dx, dy] = [p[0] - d.start[0], p[1] - d.start[1]];
    if (d.alt) return lookMove(d.k, d.layer, [pc.draw === 'dot' ? 0 : dx, dy]);
    const lv = live();
    const y = lv && lv.layer === d.layer ? lv.y : centre(pc)[1] + dy;
    if (pc.draw === 'dot') {
      const { direction, steps } = crossingsPassed(l, pc, y);
      if (steps > 0) {
        hold();
        return void slide(d.k, pc, direction, steps).finally(release);
      }
      const order = moveTo(l, d.layer, y);
      if (sameOrder(order, l.order)) return settleBack(d.run ?? [], centre(pc)[1]);
      hold();
      return void reorder(d.k, order, 'interchange').finally(release);
    }
    void reorder(d.k, moveTo(l, d.layer, y), 'interchange');
  };

  /** A piece dragged up or down: the summand is drawn with the order the piece would get. */
  const moveUpDown = (d: NonNullable<typeof drag>, dy: number) => {
    const l = layouts()[d.k];
    const pc = l && piece(l, d.layer!);
    if (!pc) return;
    const order = moveTo(l, pc.layer, centre(pc)[1] + dy);
    if (sameOrder(order, l.order)) {
      d.asked = undefined;
      setPreview(null);
      // nothing to pass: the piece gives a little and springs back on release
      setPull({ summand: d.k, layer: pc.layer, dx: 0, dy: rubber(dy, 0.3), layers: [], armed: false });
      const others = l.pieces.some((q) => q !== pc && !q.inputs.some((x) => pc.inputs.includes(x) || pc.outputs.includes(x)) && !q.outputs.some((x) => pc.inputs.includes(x) || pc.outputs.includes(x)));
      return say(others
        ? 'Drag past a piece on other strands to interchange heights.'
        : `Every other piece shares a strand with this one, so it cannot change height${pc.draw === 'crossing' ? '; drag it sideways for a braid or square move' : ''}.`);
    }
    setPull(null);
    if (d.asked && sameOrder(d.asked, order)) return;
    d.asked = order;
    void props.engine.call('term.layout', { context: props.context, term: props.term!, summands: d.k + 1, orders: { [d.k + 1]: order } }, { lane: 'interactive' }).then((o) => {
      if (drag !== d || !d.asked || !sameOrder(d.asked, order) || o.status !== 'done') return;
      const r = o.response;
      if (r.status !== 'ok') return say(`It cannot go there: ${r.message}`);
      const next = r.summands[d.k];
      setPreview({ summand: d.k, order, layout: next });
      say(next.sign !== l.sign ? 'Let go to interchange: two odd pieces swap, so this summand changes sign (−1).' : 'Let go to interchange: an equality, so only the drawing changes.');
    });
  };

  /** A crossing dragged sideways: arms its braid or square move once pulled far enough the right way. */
  const pullSideways = (d: NonNullable<typeof drag>, dx: number) => {
    const mv = d.move;
    if (!mv) {
      // no move to make: the piece gives a little, says why, and springs back on release
      const pc = piece(layouts()[d.k], d.layer!);
      setPull({ summand: d.k, layer: d.layer!, dx: rubber(dx, 0.25), layers: [], armed: false });
      return say(pc?.draw === 'crossing'
        ? 'No braid or square move here: pull the middle crossing of a triangle of crossings towards the other two, or either crossing of a stacked pair apart.'
        : `Sideways drags move crossings. Drag this piece up or down to interchange heights, or ${ALT}drag to move it for looks.`);
    }
    const m = d.matches![mv.index];
    const right = mv.side === 0 || Math.sign(dx) === mv.side;
    const shift = Math.max(-0.8, Math.min(0.8, dx)) * (right ? 1 : 0.25);
    const armed = right && Math.abs(dx) > PULL;
    setPull({ summand: d.k, layer: d.layer!, dx: shift, layers: mv.layers, armed, result: pull()?.result });
    const way = mv.side < 0 ? 'left' : mv.side > 0 ? 'right' : 'sideways';
    say(armed ? `Let go to apply ${m.relation}${m.sign === -1 ? ' (sign −1 from interchanges)' : ''}.` : `Pull ${way} to apply ${m.relation}.`);
    if (armed && !d.resultAsked && m.preview) {
      d.resultAsked = true;
      void props.engine.call('term.layout', { context: props.context, term: m.preview, summands: 4 }, { lane: 'interactive' }).then((o) => {
        const cur = pull();
        if (drag === d && cur && o.status === 'done' && o.response.status === 'ok') setPull({ ...cur, result: o.response.summands });
      });
    }
  };

  /** Keeps a released dot where it was let go until the rewritten term is drawn. */
  let holdTimer: ReturnType<typeof setTimeout> | undefined;
  let changed = false;
  const hold = () => {
    changed = false;
    const lv = live();
    if (lv) setLive({ ...lv, passed: [], near: undefined, held: true });
  };
  /** Lets go of a held dot at once if the gesture changed nothing, else when the new drawing is late. */
  const release = () => {
    clearTimeout(holdTimer);
    if (!changed) return void (live()?.held && setLive(null));
    holdTimer = setTimeout(() => live()?.held && setLive(null), 1500);
  };
  /** Eases a released dot back along its strand to where it is drawn. */
  const settleBack = (run: Line[], to: number) => {
    const lv = live();
    if (!lv || !run.length) return setLive(null);
    say('');
    const from = lv.y;
    const t0 = performance.now();
    const step = (now: number) => {
      const cur = live();
      if (!cur || cur.layer !== lv.layer) return;
      const t = Math.min(1, (now - t0) / 180);
      const [x, y] = along(run, from + (to - from) * (1 - (1 - t) ** 3));
      if (t >= 1) return setLive(null);
      setLive({ ...cur, x, y, passed: [], near: undefined });
      requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  };
  onCleanup(() => clearTimeout(holdTimer));

  /** A click on the canvas outside the diagrams clears the selection (on a frame, it picks that summand). */
  const onBackground = (e: PointerEvent) => {
    const t = e.target as Element;
    if (e.button !== 0 || t.closest('svg.diagram, .relmenu, .pullresult, button, input, label, a')) return;
    root.focus({ preventScroll: true });
    const frame = t.closest<HTMLElement>('.frame[data-summand]');
    setMenu(null);
    setCursor(null);
    setSel({ summand: frame ? Number(frame.dataset.summand) - 1 : sel().summand, layers: [] });
  };
  const onDblClick = (k: number, e: MouseEvent) => {
    const t = (e.target as Element).closest('[data-strand]');
    if (!t) return;
    const s = Number(t.getAttribute('data-strand'));
    const l = layouts()[k];
    setSel({ summand: k, layers: onStrand(l, s), strand: s });
  };
  const onContext = (k: number, e: MouseEvent) => {
    if (props.readOnly) return;
    e.preventDefault();
    const t = (e.target as Element).closest('[data-layer]');
    if (t) {
      const layer = Number(t.getAttribute('data-layer'));
      if (sel().summand !== k || !sel().layers.includes(layer)) setSel({ summand: k, layers: [layer] });
    } else if (sel().summand !== k) setSel({ summand: k, layers: [] });
    void openMenu(k);
  };

  // drops from the palette
  const dragged = (e: DragEvent): Generator | undefined => {
    const name = e.dataTransfer?.getData('text/x-diagcalc-generator') || currentDrag;
    return props.generators.find((g) => g.name === name);
  };
  let currentDrag = '';
  const onDragOver = (k: number, e: DragEvent) => {
    if (props.readOnly) return;
    const g = dragged(e);
    if (!g) return;
    e.preventDefault();
    const [x, y] = units(e.currentTarget as SVGSVGElement, e);
    const width = Math.max(g.inputs, 1);
    const t = dropTarget(layouts()[k], x, y, width);
    setDrop(t ? { summand: k, x: t.x, y: t.y, width } : null);
  };
  const onDrop = (k: number, e: DragEvent) => {
    if (props.readOnly) return;
    const g = dragged(e);
    setDrop(null);
    if (!g) return;
    e.preventDefault();
    const [x, y] = units(e.currentTarget as SVGSVGElement, e);
    const t = dropTarget(layouts()[k], x, y, Math.max(g.inputs, 1));
    if (!t) return say(`${g.name} does not fit there.`);
    insertGenerator(g, k, t);
  };

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const v = view();
    if (e.ctrlKey || e.metaKey) {
      const r = viewport.getBoundingClientRect();
      const [cx, cy] = [e.clientX - r.left, e.clientY - r.top];
      const z = Math.max(0.3, Math.min(4, v.z * Math.exp(-e.deltaY * 0.01)));
      setView({ x: cx - ((cx - v.x) * z) / v.z, y: cy - ((cy - v.y) * z) / v.z, z });
    } else setView({ ...v, x: v.x - e.deltaX, y: v.y - e.deltaY });
  };

  // keyboard paths --------------------------------------------------------------------------

  const allPieces = () => layouts().flatMap((l, k) => l.order.map((layer) => ({ k, layer })));
  const focusMove = (by: number) => {
    const list = allPieces();
    if (!list.length) return;
    const s = sel();
    const c = cursor();
    const at = c ? list.findIndex((x) => x.k === c.k && x.layer === c.layer) : -1;
    const cur = at >= 0 ? at : s.layers.length ? list.findIndex((x) => x.k === s.summand && x.layer === s.layers[s.layers.length - 1]) : -1;
    const next = list[(cur + by + list.length) % list.length];
    setCursor({ k: next.k, layer: next.layer });
    // a kept selection stays as it is while the ring moves on
    if (!s.kept) setSel({ summand: next.k, layers: [next.layer] });
    else {
      const id = idsOf(layouts()[next.k]).get(next.layer);
      say(`On ${id ?? 'a piece'}${s.summand === next.k && s.layers.includes(next.layer) ? ' (selected)' : ''}: X adds or removes it.`);
    }
  };
  /** X: adds the piece at the ring to the selection, or takes it out; the selection is then kept while Tab moves on. */
  const toggleAtCursor = () => {
    const s = sel();
    const c = cursor() ?? (s.layers.length ? { k: s.summand, layer: s.layers[s.layers.length - 1] } : null);
    if (!c) return say('Tab to a piece first; X then keeps it selected while Tab moves on.');
    setCursor(c);
    let layers: number[];
    if (s.summand !== c.k) layers = [c.layer];
    else if (!s.kept) layers = s.layers.includes(c.layer) ? s.layers : [...s.layers, c.layer];
    else layers = s.layers.includes(c.layer) ? s.layers.filter((x) => x !== c.layer) : [...s.layers, c.layer];
    setSel({ summand: c.k, layers, kept: true });
    const n = layers.length;
    say(n ? `${n} piece${n > 1 ? 's' : ''} selected. Tab moves on without changing the selection; X adds or removes the piece at the ring; Esc clears.` : 'Nothing selected.');
  };
  /**
   * A pan and zoom with the drawing still in view: at least `KEEP` pixels of the board (or all
   * of it, when it is smaller) stay inside the visible canvas on each axis, however far one pans
   * or zooms, so the diagram can never drift out of sight.
   */
  function keepInView(v: { x: number; y: number; z: number }) {
    if (!board?.isConnected || !viewport) return v;
    const [bw, bh] = [board.offsetWidth * v.z, board.offsetHeight * v.z];
    const [vw, vh] = [viewport.clientWidth, viewport.clientHeight];
    if (!vw || !vh) return v;
    const clamp = (p: number, size: number, room: number) => {
      const keep = Math.min(KEEP, size, room);
      return Math.max(keep - size, Math.min(room - keep, p));
    };
    return { x: clamp(v.x, bw, vw), y: clamp(v.y, bh, vh), z: v.z };
  }
  /** The summand `k`'s frame, panned (not zoomed) just enough to be wholly in view where it fits. */
  const reveal = (k: number) => {
    const frame = board?.querySelector<HTMLElement>(`[data-summand="${k + 1}"]`);
    if (!frame || !viewport) return;
    const v = view();
    const margin = 12;
    const shift = (start: number, size: number, room: number) => {
      const end = start + size;
      if (size + 2 * margin > room) return margin - start; // larger than the view: show its start
      if (start < margin) return margin - start;
      if (end > room - margin) return room - margin - end;
      return 0;
    };
    const dx = shift(v.x + frame.offsetLeft * v.z, frame.offsetWidth * v.z, viewport.clientWidth);
    const dy = shift(v.y + frame.offsetTop * v.z, frame.offsetHeight * v.z, viewport.clientHeight);
    if (dx || dy) setView({ ...v, x: v.x + dx, y: v.y + dy });
  };
  // a new drawing can be smaller than the last (a summand gone): clamp the pan again; and the
  // summand moved to with the keyboard is panned into view
  createEffect(on(layouts, () => requestAnimationFrame(() => setView(view())), { defer: true }));
  createEffect(on(() => sel().summand, (k) => requestAnimationFrame(() => reveal(k)), { defer: true }));
  const fit = () => {
    const frame = board.querySelector<HTMLElement>(`[data-summand="${sel().summand + 1}"]`);
    if (!frame) return setView({ x: 24, y: 16, z: 1 });
    const [w, h] = [viewport.clientWidth, viewport.clientHeight];
    const z = Math.max(0.5, Math.min(3, Math.min((w - 48) / frame.offsetWidth, (h - 48) / frame.offsetHeight)));
    setView({ x: (w - frame.offsetWidth * z) / 2 - frame.offsetLeft * z, y: (h - frame.offsetHeight * z) / 2 - frame.offsetTop * z, z });
  };

  /** Zooms about the middle of the view. */
  const zoomBy = (f: number) => {
    const v = view();
    const [cx, cy] = [viewport.clientWidth / 2, viewport.clientHeight / 2];
    const z = Math.max(0.3, Math.min(4, v.z * f));
    setView({ x: cx - ((cx - v.x) * z) / v.z, y: cy - ((cy - v.y) * z) / v.z, z });
  };
  /** Moves the insertion point by an arrow key: along a height, or to the next height up or down. */
  const moveMark = (k: number, key: string) => {
    const l = layouts()[k];
    if (!l?.boundaries.length) return;
    const heights = [...l.boundaries].sort((a, b) => a.y - b.y);
    const m = mark()?.summand === k ? mark()! : null;
    let i = m ? heights.reduce((best, c, j) => (Math.abs(c.y - m.y) < Math.abs(heights[best].y - m.y) ? j : best), 0) : heights.length - 1;
    let x = m ? Math.round(m.x) : 0;
    if (m) {
      if (key === 'ArrowUp') i = Math.min(heights.length - 1, i + 1);
      if (key === 'ArrowDown') i = Math.max(0, i - 1);
      if (key === 'ArrowLeft') x -= 1;
      if (key === 'ArrowRight') x += 1;
    }
    const b = heights[i];
    x = Math.max(0, Math.min(b.object.length - 1, x));
    setMark({ summand: k, x, y: b.y });
    say(b.object.length ? `Insertion point: strand ${x + 1} of ${b.object.length}, height ${i + 1} of ${heights.length}. Press 1–9 to insert a generator there.` : 'No strand at this height.');
  };

  const onKey = (e: KeyboardEvent) => {
    if ((e.target as HTMLElement).closest('.modal')) { e.stopPropagation(); return; }
    if ((e.target as HTMLElement).closest('input, textarea, .cm-editor')) return;
    const s = sel();
    const l = layouts()[s.summand];
    const one = l && s.layers.length === 1 ? piece(l, s.layers[0]) : undefined;
    const handled = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    if (props.readOnly && !mod(e) && (
      ['r', 'i', 'delete', 'backspace'].includes(e.key.toLowerCase()) ||
      (e.key.startsWith('Arrow') && (e.altKey || !e.shiftKey && ['ArrowUp', 'ArrowDown'].includes(e.key)))
    )) {
      say('This diagram is read only: it can be selected and inspected, not changed.');
      return handled();
    }
    if (e.key === 'Tab' && (e.target as HTMLElement).closest('.canvas-bar, .canvas-actions, .palette-bar')) {
      // the toolbar and the palette take Tab as usual; forward from the last generator comes back to the pieces
      const gens = [...root.querySelectorAll<HTMLElement>('.palette-bar .gen')];
      if (!e.shiftKey && e.target === gens.at(-1)) {
        handled();
        root.focus({ preventScroll: true });
      }
      return;
    }
    if ((e.target as HTMLElement).closest('.canvas-bar, .canvas-actions, .palette-bar, .relmenu') && (e.key === 'Enter' || e.key === ' ')) return;
    if (e.key === ' ') {
      space = true;
      return handled();
    }
    if (e.key === 'Escape') {
      handled();
      if (help()) return setHelp(false);
      if (menu()) return setMenu(null);
      if (inserting()) return setInserting(false);
      if (mark()) return setMark(null);
      if (s.layers.length || cursor()) {
        setCursor(null);
        return setSel({ summand: s.summand, layers: [] });
      }
      return props.onClose();
    }
    if (menu() && /^[1-9]$/.test(e.key)) {
      handled();
      return void apply(Number(e.key) - 1);
    }
    if (inserting() && /^[1-9]$/.test(e.key)) {
      handled();
      setInserting(false);
      const g = props.generators[Number(e.key) - 1];
      if (g) insertGenerator(g);
      return;
    }
    // while inserting, the arrows move the insertion point (the green ring)
    if (inserting() && l && !mod(e) && !e.altKey && !e.shiftKey && e.key.startsWith('Arrow')) {
      handled();
      return moveMark(s.summand, e.key);
    }
    if (e.shiftKey && !mod(e) && e.key.startsWith('Arrow')) {
      handled();
      if (e.altKey) {
        if (!one) return say('Select one piece first (click it, or Tab to it).');
        const d: Record<string, [number, number]> = { ArrowLeft: [-0.1, 0], ArrowRight: [0.1, 0], ArrowUp: [0, 0.1], ArrowDown: [0, -0.1] };
        return lookMove(s.summand, one.layer, d[e.key]);
      }
      const v = view();
      const d: Record<string, [number, number]> = { ArrowLeft: [40, 0], ArrowRight: [-40, 0], ArrowUp: [0, 40], ArrowDown: [0, -40] };
      return setView({ ...v, x: v.x + d[e.key][0], y: v.y + d[e.key][1] });
    }
    if (mod(e) && e.key.toLowerCase() === 'a') {
      handled();
      if (l) setSel({ summand: s.summand, layers: [...l.order], kept: true });
      return;
    }
    if (isAction(e, 'ask')) {
      handled();
      return props.onAsk?.();
    }
    if (mod(e)) return;
    switch (e.key) {
      case 'Tab': {
        // Shift+Tab before the first piece leaves for the palette and the toolbar
        const list = allPieces();
        const first = !s.layers.length || (list[0] && list[0].k === s.summand && list[0].layer === s.layers[s.layers.length - 1]);
        if (e.shiftKey && first) {
          const out = root.querySelector<HTMLElement>('.palette-bar .gen:last-child') ?? root.querySelector<HTMLElement>('.canvas-actions button:last-of-type');
          if (out) {
            handled();
            setSel({ summand: s.summand, layers: [] });
            return out.focus();
          }
        }
        handled();
        return focusMove(e.shiftKey ? -1 : 1);
      }
      case 's':
      case 'S': {
        handled();
        if (!l || !l.strands.length) return;
        const n = l.strands.length;
        const back = e.key === 'S';
        const start = s.strand !== undefined ? (s.strand + (back ? -1 : 1) + n) % n : (one?.inputs[0] ?? one?.outputs[0] ?? 0);
        setSel({ summand: s.summand, layers: onStrand(l, start), strand: start, kept: true });
        const c = l.strands[start]?.colour;
        const tag = [strandIdsOf(l).get(start), c].filter(Boolean).join(', ');
        return say(`Strand ${start + 1} of ${n}${tag ? ` (${tag})` : ''} selected; S and Shift+S move to the next and previous strand, Tab and X add or remove single pieces.`);
      }
      case 'x':
      case 'X':
        handled();
        return toggleAtCursor();
      case 'l':
      case 'L':
        handled();
        setLabels(!labels());
        return say(labels() ? 'Occurrence IDs shown: d dots, c crossings, u cups, n caps, b bubbles, g other pieces, s strands, numbered through the document in the order they were made.' : 'Occurrence IDs hidden; hovering a piece still shows its ID.');
      case '+':
      case '=':
        handled();
        return zoomBy(1.25);
      case '-':
      case '_':
        handled();
        return zoomBy(0.8);
      case 'r':
      case 'R':
        handled();
        return void openMenu(s.summand);
      case 'i':
      case 'I':
        handled();
        if (!inserting()) say('Inserting: press 1–9 for a generator; the arrows move the insertion point first. Esc cancels.');
        return setInserting(!inserting());
      case 'f':
      case 'F':
        handled();
        return fit();
      case '0':
        handled();
        return setView({ x: 24, y: 16, z: 1 });
      case '?':
        handled();
        return setHelp(!help());
      case 'Delete':
      case 'Backspace':
        handled();
        return removeSelected();
      case 'ArrowLeft':
      case 'ArrowRight':
        handled();
        setCursor(null);
        return setSel({ summand: Math.max(0, Math.min(layouts().length - 1, s.summand + (e.key === 'ArrowLeft' ? -1 : 1))), layers: [] });
      case 'ArrowUp':
      case 'ArrowDown': {
        handled();
        if (!one) return say('Select one piece first (click it, or Tab to it).');
        const up = e.key === 'ArrowUp';
        if (e.altKey) return void reorder(s.summand, moveBy(l.order, one.layer, up ? 1 : -1), 'interchange');
        if (one.draw !== 'dot') return say(`Arrow keys slide a dot; ${e.altKey ? '' : `${isMac ? '⌥' : 'Alt+'}arrows interchange other pieces.`}`);
        const next = nextOnStrand(l, one, up);
        if (next?.draw !== 'crossing') return say(`No crossing ${up ? 'above' : 'below'} this dot on its strand.`);
        return void slide(s.summand, one, up ? 'up' : 'down', 1);
      }
    }
  };
  const onKeyUp = (e: KeyboardEvent) => {
    if (e.key === ' ') space = false;
  };

  onMount(() => {
    root.focus({ preventScroll: true });
    const fit = () => {
      setRowWidth(Math.max(320, viewport.clientWidth - 48));
      setView(view());
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(viewport);
    onCleanup(() => ro.disconnect());
  });

  // drawing ---------------------------------------------------------------------------------

  /** A summand's sign and coefficient, typeset. */
  const Lead = (q: { l: Layout; k: number }) => (
    <>
      {q.l.negative ? '−' : q.k === 0 ? '' : '+'}
      <Show when={q.l.coefficient}>{(c) => <> <Typeset text={c()} /></>}</Show>
    </>
  );

  const Diagram = (p: { l: Layout; k: number }) => {
    const l = () => p.l;
    const pad = 0.7;
    const selected = () => (sel().summand === p.k ? new Set(sel().layers) : new Set<number>());
    const offsets = () => hintOf(l())?.offsets ?? {};
    const off = (layer?: number): [number, number] => {
      if (layer === undefined) return [0, 0];
      const pc = piece(l(), layer);
      const base = offsetFor(pc, offsets()[layer]);
      const g = ghost();
      if (g && g.summand === p.k && g.layer === layer) return [base[0] + g.dx, base[1] + g.dy];
      return base;
    };
    const strandColour = (s?: number) => (s === undefined ? 'currentColor' : colourOf(l().strands[s]?.colour ?? ''));
    const strandDash = (s?: number) => (s === undefined ? undefined : dashOf(l().strands[s]?.colour ?? ''));
    const vb = () => `${-pad} ${-(l().height + pad)} ${l().width + 2 * pad} ${l().height + 2 * pad + 0.5}`;
    const ids = createMemo(() => idsOf(l()));
    const strandIds = createMemo(() => strandIdsOf(l()));
    /** The piece the keyboard ring is on, when it is in this summand and not just the selection. */
    const ring = () => {
      const c = cursor();
      if (!c || c.k !== p.k) return undefined;
      if (!sel().kept && selected().has(c.layer) && selected().size === 1) return undefined;
      return piece(l(), c.layer);
    };
    /** Where each strand's ID goes: beside the lower end of its lowest stretch, where it starts. */
    const strandStarts = () => {
      const low = new Map<number, { x: number; y: number }>();
      for (const line of l().lines) {
        if (line.kind !== 'strand' || line.strand === undefined) continue;
        const [x, y] = line.y0! <= line.y1! ? [line.x0!, line.y0!] : [line.x1!, line.y1!];
        const t = low.get(line.strand);
        if (!t || y < t.y) low.set(line.strand, { x, y });
      }
      return [...low].map(([strand, t]) => ({ strand, ...t }));
    };
    const snapped = () => (selected().size ? bounds(l(), [...selected()]) : undefined);
    const isLive = (layer?: number) => {
      const lv = live();
      return !!lv && lv.summand === p.k && lv.layer === layer;
    };
    /** The piece whose copy a sideways or blocked drag is carrying: drawn faintly where it was. */
    const pulled = (layer?: number) => {
      const pl = pull();
      return !!pl && pl.summand === p.k && pl.layer === layer;
    };
    const lineEl = (line: Line) => {
      const o = () => (isLive(line.layer) ? [0, 0] as [number, number] : off(line.layer));
      const hot = () => line.layer !== undefined && selected().has(line.layer);
      const strandHot = () => sel().summand === p.k && sel().strand !== undefined && line.strand === sel().strand;
      switch (line.kind) {
        case 'strand':
        case 'turn':
          return <path d={path(line, o())} stroke={strandColour(line.strand)} stroke-dasharray={strandDash(line.strand)} stroke-width={hot() || strandHot() ? 0.09 : 0.055} fill="none" opacity={pulled(line.layer) ? 0.3 : undefined} />;
        case 'dot':
          return (
            <>
              <circle cx={line.x! + o()[0]} cy={-(line.y! + o()[1])} r={hot() ? 0.15 : 0.11} fill={hot() ? 'var(--accent)' : strandColour(line.strand)} visibility={isLive(line.layer) ? 'hidden' : undefined} />
              {/* a dot power above 3: one dot labelled with its exponent (docs/conventions.md, Drawing) */}
              <Show when={line.power}>{(n) => <text x={line.x! + o()[0] + 0.17} y={-(line.y! + o()[1]) + 0.12} font-size="0.32" fill={strandColour(line.strand)} data-power={n()}>{n()}</text>}</Show>
            </>
          );
        case 'box':
          return (
            <g transform={`translate(${o()[0]} ${-o()[1]})`}>
              <rect x={line.x0} y={-line.y1!} width={line.x1! - line.x0!} height={line.y1! - line.y0!} fill="var(--panel)" stroke={hot() ? 'var(--accent)' : 'currentColor'} stroke-width={0.04} />
              <text x={(line.x0! + line.x1!) / 2} y={-(line.y0! + line.y1!) / 2 + 0.13} font-size="0.36" text-anchor="middle" fill="currentColor">{line.label}</text>
            </g>
          );
        case 'label':
          return <text x={line.x} y={0.55} font-size="0.36" font-style="italic" text-anchor="middle" fill={strandColour(line.strand)}>{minus(line.text)}</text>;
        case 'arrow':
          return <polygon points={arrowPoints(line)} fill={line.strand === undefined ? 'currentColor' : strandColour(line.strand)} />;
        case 'note':
          return <text x={line.x} y={-line.y! + 0.13} font-size="0.36" font-style="italic" text-anchor="middle" fill={line.strand === undefined ? 'currentColor' : strandColour(line.strand)}>{minus(line.text)}</text>;
      }
    };
    return (
      <svg class="diagram" viewBox={vb()} width={(l().width + 2 * pad) * U} height={(l().height + 2 * pad + 0.5) * U}
        role="img" aria-label={l().text} data-summand={p.k + 1}
        onPointerDown={(e) => onDown(p.k, e)} onPointerMove={onMove} onPointerUp={onUp}
        onDblClick={(e) => onDblClick(p.k, e)} onContextMenu={(e) => onContext(p.k, e)}
        onDragOver={(e) => onDragOver(p.k, e)} onDragLeave={() => setDrop(null)} onDrop={(e) => onDrop(p.k, e)}>
        <rect x={-pad} y={-(l().height + pad)} width={l().width + 2 * pad} height={l().height + 2 * pad + 0.5} fill="transparent" />
        <For each={l().lines}>{(line) => lineEl(line)}</For>
        {/* hit targets: strands under pieces */}
        <For each={l().lines.filter((x) => x.kind === 'strand' || x.kind === 'turn')}>
          {(line) => <path d={path(line, off(line.layer))} stroke="transparent" stroke-width={0.3} fill="none" data-strand={line.strand} class="hit-strand" />}
        </For>
        <For each={l().pieces}>
          {(pc) => {
            const o = () => off(pc.layer);
            return (
              <rect class="hit-piece" data-layer={pc.layer} data-draw={pc.draw}
                x={pc.box[0] - 0.08 + o()[0]} y={-(pc.box[3] + 0.08 + o()[1])} width={pc.box[2] - pc.box[0] + 0.16} height={pc.box[3] - pc.box[1] + 0.16}
                rx={0.08} fill={selected().has(pc.layer) ? 'var(--accent-soft)' : 'transparent'} fill-opacity={0.55}
                stroke={selected().has(pc.layer) ? 'var(--accent)' : 'none'} stroke-width={0.03}>
                <title>{ids().get(pc.layer)}: {pc.generator} ({pc.draw}{pc.odd ? ', odd' : ''}, degree {pc.degree})</title>
              </rect>
            );
          }}
        </For>
        <Show when={ring()}>
          {(pc) => <rect class="ring" data-testid="ring" x={pc().box[0] - 0.16 + off(pc().layer)[0]} y={-(pc().box[3] + 0.16 + off(pc().layer)[1])}
            width={pc().box[2] - pc().box[0] + 0.32} height={pc().box[3] - pc().box[1] + 0.32} rx={0.1} fill="none" stroke="var(--accent)" stroke-width={0.035} stroke-dasharray="0.09 0.06" pointer-events="none" />}
        </Show>
        <Show when={labels()}>
          <g class="occ-labels" pointer-events="none" data-testid="occ-labels">
            {/* a dot's ID to its left, another piece's to its right */}
            <For each={l().pieces}>
              {(pc) => {
                const dot = pc.draw === 'dot';
                const [x, y] = dot ? [pc.box[0] - 0.05, (pc.box[1] + pc.box[3]) / 2 - 0.08] : [pc.box[2] + 0.05, (pc.box[1] + pc.box[3]) / 2 - 0.08];
                return <text x={x + off(pc.layer)[0]} y={-(y + off(pc.layer)[1])} font-size="0.24" text-anchor={dot ? 'end' : 'start'} fill="var(--muted)" data-occ={ids().get(pc.layer)}>{ids().get(pc.layer)}</text>;
              }}
            </For>
            <For each={strandStarts()}>
              {(t) => <text x={t.x + 0.08} y={-(t.y + 0.12)} font-size="0.24" fill={strandColour(t.strand)} data-occ={strandIds().get(t.strand)}>{strandIds().get(t.strand)}</text>}
            </For>
          </g>
        </Show>
        <For each={(props.remoteSelections ?? []).filter((s) => s.selection.cellId === props.cellId && s.selection.word === l().word)}>
          {(s) => <Show when={bounds(l(), s.selection.layers)}>{(b) => <g pointer-events="none" data-testid="remote-canvas-selection">
            <rect x={b()[0] - 0.2} y={-(b()[3] + 0.2)} width={b()[2] - b()[0] + 0.4} height={b()[3] - b()[1] + 0.4}
              fill="none" stroke={s.colour} stroke-width={0.035} stroke-dasharray="0.12 0.06" />
            <text x={b()[0] - 0.2} y={-(b()[3] + 0.25)} font-size="0.24" fill={s.colour}>{s.name}</text>
          </g>}</Show>}
        </For>
        <Show when={snapped()}>
          {(b) => <rect x={b()[0] - 0.15} y={-(b()[3] + 0.15)} width={b()[2] - b()[0] + 0.3} height={b()[3] - b()[1] + 0.3} fill="none" stroke="var(--accent)" stroke-width={0.025} stroke-dasharray="0.1 0.07" pointer-events="none" />}
        </Show>
        <Show when={boxDrag()?.summand === p.k && boxDrag()}>
          {(b) => <rect x={Math.min(b().a[0], b().b[0])} y={-Math.max(b().a[1], b().b[1])} width={Math.abs(b().a[0] - b().b[0])} height={Math.abs(b().a[1] - b().b[1])} fill="var(--accent-soft)" fill-opacity={0.4} stroke="var(--accent)" stroke-width={0.02} pointer-events="none" />}
        </Show>
        <Show when={drop()?.summand === p.k && drop()}>
          {(d) => (
            <g pointer-events="none">
              <line x1={-pad / 2} x2={l().width + pad / 2} y1={-d().y} y2={-d().y} stroke="var(--good)" stroke-width={0.03} stroke-dasharray="0.08 0.06" />
              <rect x={d().x - 0.2} y={-d().y - 0.2} width={d().width - 1 + 0.4} height={0.4} rx={0.1} fill="var(--good-soft)" stroke="var(--good)" stroke-width={0.03} />
            </g>
          )}
        </Show>
        <Show when={mark()?.summand === p.k && dropTarget(l(), mark()!.x, mark()!.y, 1)}>
          {(t) => (
            <g pointer-events="none" data-testid="mark">
              <line x1={t().x - 0.35} x2={t().x + 0.35} y1={-t().y} y2={-t().y} stroke="var(--good)" stroke-width={0.03} stroke-dasharray="0.08 0.06" />
              <circle cx={t().x} cy={-t().y} r={0.16} fill="var(--good-soft)" stroke="var(--good)" stroke-width={0.04} />
            </g>
          )}
        </Show>
        <Show when={pull()?.summand === p.k && pull()}>
          {(pl) => (
            <g pointer-events="none" data-testid="pull">
              {/* the crossing being pulled, carried along by the pointer; the drawing itself stays */}
              <g class="pulled" transform={`translate(${pl().dx} ${-(pl().dy ?? 0)})`}>
                <For each={l().lines.filter((x) => x.layer === pl().layer)}>
                  {(line) =>
                    line.kind === 'strand' ? <path d={path(line)} stroke={strandColour(line.strand)} stroke-width={0.07} fill="none" />
                    : line.kind === 'dot' ? <circle cx={line.x} cy={-line.y!} r={0.13} fill={strandColour(line.strand)} />
                    : line.kind === 'box' ? <rect x={line.x0} y={-line.y1!} width={line.x1! - line.x0!} height={line.y1! - line.y0!} fill="var(--panel)" stroke="currentColor" stroke-width={0.04} />
                    : null}
                </For>
              </g>
              <For each={pl().layers}>
                {(layer) => {
                  const c = () => centre(piece(l(), layer) ?? l().pieces[0]);
                  return <circle class={`xing ${pl().armed ? 'passed' : 'near'}`} data-layer={layer} cx={c()[0]} cy={-c()[1]} r={0.34} />;
                }}
              </For>
            </g>
          )}
        </Show>
        <Show when={live()?.summand === p.k && live()}>
          {(lv) => {
            const at = (layer: number) => centre(piece(l(), layer)!);
            return (
              <g pointer-events="none" data-testid="live">
                <Show when={lv().near}>
                  {(n) => <circle class="xing near" data-layer={n().layer} cx={at(n().layer)[0]} cy={-at(n().layer)[1]} r={0.24 + 0.14 * n().w} opacity={0.25 + 0.75 * n().w} />}
                </Show>
                <For each={lv().passed}>
                  {(layer) => <circle class="xing passed" data-layer={layer} cx={at(layer)[0]} cy={-at(layer)[1]} r={0.36} />}
                </For>
                <circle class="live-halo" cx={lv().x} cy={-lv().y} r={0.26} />
                <circle class="live-dot" cx={lv().x} cy={-lv().y} r={0.14} fill={strandColour(piece(l(), lv().layer)?.inputs[0])} />
              </g>
            );
          }}
        </Show>
      </svg>
    );
  };

  const Mini = (p: { ls: Layout[] }) => (
    <span class="mini">
      <For each={p.ls}>
        {(l, k) => (
          <span class="summand">
            <Show when={l.negative || k() > 0 || l.coefficient}><span class="c"><Lead l={l} k={k()} /></span></Show>
            <svg viewBox={`-0.5 ${-(l.height + 0.5)} ${l.width + 1} ${l.height + 1.3}`} width={(l.width + 1) * 16} height={(l.height + 1.3) * 16}>
              <For each={l.lines}>
                {(line) =>
                  line.kind === 'strand' || line.kind === 'turn' ? <path d={path(line)} stroke={colourOf(l.strands[line.strand ?? 0]?.colour ?? '')} stroke-width={0.08} fill="none" />
                  : line.kind === 'dot' ? <><circle cx={line.x} cy={-line.y!} r={0.15} fill="currentColor" /><Show when={line.power}>{(n) => <text x={line.x! + 0.2} y={-line.y! + 0.13} font-size="0.36" fill="currentColor">{n()}</text>}</Show></>
                  : line.kind === 'box' ? <rect x={line.x0} y={-line.y1!} width={line.x1! - line.x0!} height={line.y1! - line.y0!} fill="none" stroke="currentColor" stroke-width={0.06} />
                  : line.kind === 'arrow' ? <polygon points={arrowPoints(line, 1.3)} fill={colourOf(l.strands[line.strand ?? 0]?.colour ?? '')} />
                  : null}
              </For>
            </svg>
          </span>
        )}
      </For>
    </span>
  );

  /** A generator drawn as it sits in a diagram, with arrows for the orientation of its strands. */
  const PaletteIcon = (p: { g: Generator }) => {
    const w = Math.max(p.g.inputs, p.g.outputs, 1);
    const src = strandsOf(p.g.source);
    const tgt = strandsOf(p.g.target);
    const known = p.g.source !== undefined;
    /** An arrowhead at (x, y), y upwards, pointing up or down; or left or right with `side`. */
    const head = (x: number, y: number, dir: 'up' | 'down' | 'left' | 'right') => {
      const s = 0.2;
      const pts = dir === 'up' ? [[x - s, -y + s * 0.7], [x + s, -y + s * 0.7], [x, -y - s * 0.7]]
        : dir === 'down' ? [[x - s, -y - s * 0.7], [x + s, -y - s * 0.7], [x, -y + s * 0.7]]
        : dir === 'right' ? [[x - s * 0.7, -y - s], [x - s * 0.7, -y + s], [x + s * 0.7, -y]]
        : [[x + s * 0.7, -y - s], [x + s * 0.7, -y + s], [x - s * 0.7, -y]];
      return <polygon points={pts.map((q) => q.join(',')).join(' ')} fill="currentColor" />;
    };
    return (
      <svg viewBox={`-0.4 -1.2 ${w - 1 + 0.8} 1.4`} width={(w - 1 + 0.8) * 22} height={1.4 * 22} aria-hidden="true">
        {p.g.draw === 'dot' ? (
          <>
            <path d="M 0 0 L 0 -1" stroke="currentColor" stroke-width={0.07} />
            <circle cx={0} cy={-0.5} r={0.13} fill="currentColor" />
            {known && head(0, 0.85, src[0]?.up === false ? 'down' : 'up')}
          </>
        ) : p.g.draw === 'crossing' ? (
          <>
            <path d={path({ kind: 'strand', x0: 0, y0: 0, x1: 1, y1: 1 })} stroke="currentColor" stroke-width={0.07} fill="none" />
            <path d={path({ kind: 'strand', x0: 1, y0: 0, x1: 0, y1: 1 })} stroke="currentColor" stroke-width={0.07} fill="none" />
            {known && src.length === 2 && <>{head(1, 0.9, src[0].up ? 'up' : 'down')}{head(0, 0.9, src[1].up ? 'up' : 'down')}</>}
          </>
        ) : p.g.draw === 'cup' && w === 2 ? (
          <>
            <path d="M 0 -1 C 0 -0.15, 1 -0.15, 1 -1" stroke="currentColor" stroke-width={0.07} fill="none" />
            {known && tgt.length === 2 && head(0.5, 0.36, tgt[0].up ? 'left' : 'right')}
          </>
        ) : p.g.draw === 'cap' && w === 2 ? (
          <>
            <path d="M 0 0 C 0 -0.85, 1 -0.85, 1 0" stroke="currentColor" stroke-width={0.07} fill="none" />
            {known && src.length === 2 && head(0.5, 0.64, src[0].up ? 'right' : 'left')}
          </>
        ) : (
          <rect x={-0.3} y={-0.8} width={w - 1 + 0.6} height={0.6} fill="none" stroke="currentColor" stroke-width={0.06} />
        )}
      </svg>
    );
  };

  return (
    <div class="canvas" classList={{ readonly: !!props.readOnly }} ref={root} tabindex={0} data-keeps-focus role="application" aria-label={`Canvas for ${props.title}`} onKeyDown={onKey} onKeyUp={onKeyUp}>
      <div class="canvas-bar">
        <button onClick={() => props.onClose()} title="Close the canvas and go back to the document (Esc)" data-testid="canvas-back">← Back to the document</button>
        <strong>{props.title}</strong>
        <Show when={props.readOnly}><span class="hint">Read only</span></Show>
        <span class="hint">{total() > layouts().length ? `${layouts().length} of ${total()} summands` : `${total()} ${total() === 1 ? 'summand' : 'summands'}`}</span>
        <Show when={busy()}><span class="light busy" /></Show>
        <Show when={props.value?.status === 'ok' && props.value}>
          {(v) => <span class="canvas-badge row"><span class="hint">value</span><ValueBadge value={v() as Extract<CellValue, { status: 'ok' }>} compact /></span>}
        </Show>
        <span class="hint grow canvas-msg" data-testid="canvas-message" role="status">{message()}</span>
        <button class="ghost" aria-pressed={labels()} classList={{ on: labels() }} onClick={() => setLabels(!labels())}
          title="Show the occurrence IDs of pieces and strands (L)" data-testid="labels-toggle">IDs</button>
        <button class="ghost" onClick={() => setHelp(!help())} title="Gestures and keys (?)" aria-label="Gestures and keys">?</button>
      </div>
      <Show when={props.expanded}>
        <div class="banner">{props.readOnly ? 'This term uses definitions and is shown written out.' : 'This term uses definitions; it is shown written out, and a gesture writes it out in full in the cell.'}</div>
      </Show>
      <div class="canvas-actions" role="group" aria-label="Diagram actions">
        <Show when={!props.readOnly}>
          <button onClick={() => void openMenu(sel().summand)} disabled={busy() || !layouts().length} title="Rewrite using a relation without changing the element (R)">Relations</button>
          <button onClick={removeSelected} disabled={busy() || !sel().layers.length} aria-label="Remove selected pieces" title="Remove selected pieces; this changes the element (Delete)">Remove</button>
        </Show>
        <div class="grow" />
        <button class="ghost" onClick={() => zoomBy(0.8)} aria-label="Zoom out" title="Zoom out (−)">−</button>
        <button class="ghost" onClick={fit} aria-label="Fit diagram" title="Fit the selected summand (F)">Fit</button>
        <button class="ghost" onClick={() => zoomBy(1.25)} aria-label="Zoom in" title="Zoom in (+)">+</button>
      </div>
      <Show when={!props.readOnly}>
        <div class="canvas-insert-help hint">Select a strand to add a piece; select a piece to remove it. Relations rewrite without changing the element.</div>
      </Show>
      <div class="canvas-body">
        <Show when={!props.readOnly}><div class="palette-bar" aria-label="Generators">
          <For each={props.generators}>
            {(g, i) => (
              <button class="gen" disabled={props.readOnly} draggable={!props.readOnly} data-generator={g.name} aria-label={describeGenerator(g)}
                title={`${describeGenerator(g)}. Drag onto a strand to insert it; or click a strand to set the insertion point, then click here (or press I, then ${i() + 1})`}
                classList={{ on: inserting() }}
                onDragStart={(e) => {
                  currentDrag = g.name;
                  e.dataTransfer?.setData('text/x-diagcalc-generator', g.name);
                  e.dataTransfer && (e.dataTransfer.effectAllowed = 'copy');
                }}
                onDragEnd={() => {
                  currentDrag = '';
                  setDrop(null);
                }}
                onClick={() => insertGenerator(g)}>
                <PaletteIcon g={g} />
                <span class="gen-label">{g.draw === 'dot' ? 'Dot' : g.draw === 'crossing' ? 'Crossing' : g.draw === 'cup' ? 'Cup' : g.draw === 'cap' ? 'Cap' : g.name}</span>
                <span>{inserting() ? `${i() + 1} ` : ''}<Typeset text={g.written ?? g.name} /></span>
              </button>
            )}
          </For>
        </div></Show>
        <div class="canvas-view" ref={viewport} onWheel={onWheel} onPointerDown={onBackground}>
          <Show when={props.term !== null} fallback={<div class="hint" style={{ padding: '20px' }}>Computing the term…</div>}>
            <Show when={!error()} fallback={<div class="error" style={{ margin: '20px' }}>{error()}</div>}>
              <div class="canvas-board" ref={board} style={{ transform: `translate(${view().x}px, ${view().y}px) scale(${view().z})`, 'max-width': `${rowWidth()}px` }}>
                <Show when={layouts().length} fallback={<div class="frame"><span class="c">0</span></div>}>
                  <For each={layouts()}>
                    {(l, k) => (
                      <div class="frame" data-summand={k() + 1} data-word={l.word}
                        classList={{ on: sel().summand === k(), new: fresh().has(l.word) }}>
                        <div class="row frame-head">
                          <span class="c"><Lead l={l} k={k()} /></span>
                          <Show when={l.sign === -1}><span class="badge warn" title="This drawing's layers are in an order that costs a Koszul sign">−1 from interchange</span></Show>
                          <span class="hint">degree {l.degree}</span>
                        </div>
                        <Diagram l={preview()?.summand === k() ? preview()!.layout : l} k={k()} />
                      </div>
                    )}
                  </For>
                </Show>
              </div>
            </Show>
          </Show>
          <Show when={pull()?.armed && pull()?.result}>
            {(ls) => <div class="pullresult" data-testid="pull-result"><span class="hint">Becomes</span> <Mini ls={ls()} /></div>}
          </Show>
          <Show when={!props.readOnly && menu()}>
            {(m) => (
              <div class="relmenu" role="dialog" aria-label="Relations that apply">
                <div class="row"><strong class="grow">Relations{m().rect ? ' in the selection' : ''}</strong><button class="ghost" onClick={() => setMenu(null)} aria-label="Close the relations">×</button></div>
                <For each={m().matches.slice(0, 9)}>
                  {(x, i) => (
                    <button class="relitem" disabled={props.readOnly} onClick={() => void apply(i())} data-relation={x.relation}>
                      <kbd>{i() + 1}</kbd>
                      <span class="grow">
                        <span class="relname">{x.relation}</span> <span class="hint">{x.direction}{x.sign === -1 ? ', sign −1' : ''}</span>
                        <Show when={m().previews[i()]} fallback={<div class="hint"><Typeset text={x.preview ?? ''} /></div>}>
                          {(ls) => <div><Mini ls={ls()} /></div>}
                        </Show>
                      </span>
                    </button>
                  )}
                </For>
                <Show when={m().matches.length > 9}><div class="hint">{m().matches.length - 9} more; select a smaller region to narrow them.</div></Show>
              </div>
            )}
          </Show>
        </div>
      </div>
      <Show when={help()}>
        <Dialog label="Gestures and keys" class="canvas-help" noClose onClose={() => setHelp(false)}>
          <div class="row"><h3 class="grow">Gestures and keys</h3><button class="ghost" onClick={() => setHelp(false)} aria-label="Close the gestures">×</button></div>
          <table class="kv keys gestures">
            <thead><tr><th>Action</th><th>Pointer</th><th>Keys</th></tr></thead>
            <tbody>
              <For each={GESTURES.filter(([, , , edits]) => !props.readOnly || !edits)}>{([a, p, k]) => <tr><td>{a}</td><td>{p || '—'}</td><td>{k}</td></tr>}</For>
            </tbody>
          </table>
        </Dialog>
      </Show>
    </div>
  );
}

/** A label as typeset: a minus sign rather than a hyphen before a number. */
const minus = (t?: string) => t?.replace(/(^|[^A-Za-z0-9_])-(?=[0-9])/g, '$1−');

/** Pixels of the drawing that always stay in view, however far the canvas is panned or zoomed. */
const KEEP = 80;

/** How near (in units) a dragged dot must come to a crossing on its strand before the crossing glows. */
const REACH = 0.75;
/** How far (in units) a crossing must be pulled sideways to arm its move. */
const PULL = 0.45;
/** A drag that can do nothing moves the piece this far at most, easing off: a rubber band. */
const rubber = (v: number, max: number) => max * Math.tanh(v / max);

const ALT = isMac ? '⌥' : 'Alt+';
/** The canvas's actions, each with its pointer gesture and its keys. */
const GESTURES: [action: string, pointer: string, keys: string, edits?: boolean][] = [
  ['Slide a dot through the next crossing on its strand (dot-slide relation)', 'drag the dot; a crossing glows as it nears and rings once passed', '↑ ↓ on a selected dot', true],
  ['Interchange a piece with pieces on other strands; a −1 badge shows when two odd pieces swap', 'drag it up or down', `${ALT}↑ ${ALT}↓`, true],
  ['Braid relation (middle crossing of a triangle) or square relation (a stacked pair)', 'drag the crossing sideways; the result shows before you let go', 'R, then the relation’s number', true],
  ['Relations that apply to the selection, or inside a box', 'Relations, or right-click', 'R, then 1–9 applies one', true],
  ['Select a piece; add or remove one', `click; ${K}click`, 'Tab, ⇧Tab'],
  ['Select a whole strand', 'double-click the strand', 'S, ⇧S (next, previous strand)'],
  ['Select several pieces', `drag a box on empty space; ${K}click adds or removes one`, `Tab to a piece, X keeps it; Tab on, X adds (or removes) the piece at the dashed ring; ${K}A selects all`],
  ['Clear the selection', 'click empty space', 'Esc'],
  ['Set the insertion point (a green ring)', 'click a strand', 'I, then the arrows', true],
  ['Insert a generator', 'drag it from the palette, or click it after setting the insertion point', 'I, then 1–9; or Tab to the palette and Enter', true],
  ['Remove the selected pieces', 'Remove', 'Delete', true],
  ['Move a piece for looks only (a layout hint)', `${ALT}drag`, `${ALT}⇧ arrows`, true],
  ['Move between summands', 'click a summand', '← →'],
  ['Pan', 'space-drag, scroll', '⇧ arrows'],
  ['Zoom', `+ / − buttons, Fit; ${K}scroll, pinch`, '+ −; F fits the summand, 0 resets'],
  ['Show or hide occurrence IDs (d dot, c crossing, u cup, n cap, b bubble, g other piece, s strand: numbered through the document in the order they were made, kept through rewrites and edits)', 'hover a piece; IDs', 'L'],
  ['Ask the AI about the selection', '', keysOf('ask')],
  ['Leave the canvas for its controls', 'click', '⇧Tab from the first piece'],
  ['Back to the document', '← Back', 'Esc (after clearing the selection)'],
];
