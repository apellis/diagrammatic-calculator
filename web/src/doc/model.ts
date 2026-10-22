// The document: a Yjs CRDT holding the workspace's inputs (context and cells), the pinned
// snapshots and the version history. Results are never stored here (see src/eval/).
//
// Every change goes through `step`, so it is one undoable step with an author and a note, and
// it is recorded as a version in the history.
import * as Y from 'yjs';

export const FORMAT = 'diagcalc-workspace/1';

/** The origin of changes made in this browser by the person using it. */
export const LOCAL = { author: 'you' };
/** The origin of changes made by the in-app AI on the person's behalf. */
export const AI = { author: 'AI' };
/** The origin of history bookkeeping, which undo ignores. */
const HISTORY = { author: 'history' };

export type CellKind = 'def' | 'watch' | 'solve' | 'check' | 'note' | 'comment';

export interface Version {
  n: number;
  author: string;
  note: string;
  time: number;
  /** Y.encodeSnapshot of the document after this step. */
  snapshot: Uint8Array;
}

/** A frozen result: the value and everything needed to say how it was obtained. */
export interface Snapshot {
  name: string;
  cellId: string;
  cellText: string;
  context: string;
  version: number;
  /** The cache key of the result, a hash of its inputs. */
  key: string;
  value: unknown;
  time: number;
}

/** How a summand of a cell is drawn: an order of its layers (reached by interchanges) and offsets of pieces moved for looks. Hints never change a term. */
export interface Hint {
  order?: number[];
  offsets?: Record<string, [number, number]>;
}

export interface CellData {
  id: string;
  text: string;
}

/** The inputs of a workspace: what a branch starts from and what accepting it merges. */
export interface Inputs {
  context: string;
  cells: CellData[];
  hints: Record<string, Hint>;
}

/** A scratch branch: a copy of the document to try something on, accepted into it or discarded. */
export interface BranchRecord {
  id: string;
  name: string;
  author: string;
  created: number;
  /** The document's inputs when the branch was made, for a three-way merge on accept. */
  base: Inputs;
  /** The branch's own document, as a Yjs update. */
  state: Uint8Array;
}

/** What a branch changes relative to the document, cell by cell. */
export interface Comparison {
  context?: { from: string; to: string };
  added: CellData[];
  removed: CellData[];
  changed: { id: string; from: string; to: string }[];
  hints: number;
  /** Cells the document also changed since the branch was made; accepting takes the branch's text. */
  conflicts: string[];
}

/** The kind of cell a text starts, from its first word. */
export function kindOf(text: string): CellKind | null {
  const w = text.trimStart().split(/\s/, 1)[0];
  if (w.startsWith('#')) return 'comment';
  return (['def', 'watch', 'solve', 'check', 'note'] as const).find((k) => k === w) ?? null;
}

const LETTER: Record<CellKind, string> = { def: 'd', watch: 'w', solve: 's', check: 'k', note: 'n', comment: 'c' };

export class Doc {
  readonly ydoc: Y.Doc;
  readonly meta: Y.Map<unknown>;
  readonly cells: Y.Array<Y.Map<string>>;
  readonly history: Y.Array<Version>;
  readonly snapshots: Y.Array<Snapshot>;
  /** Layout hints, keyed by cell ID and summand word. */
  readonly hints: Y.Map<Hint>;
  /** Scratch branches of this document (only the main document has any). */
  readonly branches: Y.Map<BranchRecord>;
  /** Counters that undo leaves alone, so a cell ID is never reused. */
  private readonly counters: Y.Map<number>;
  readonly undoManager: Y.UndoManager;
  private pendingNote: string | null = null;

  constructor(ydoc = new Y.Doc({ gc: false })) {
    this.ydoc = ydoc;
    this.meta = ydoc.getMap('meta');
    this.cells = ydoc.getArray('cells');
    this.history = ydoc.getArray('history');
    this.snapshots = ydoc.getArray('snapshots');
    this.hints = ydoc.getMap('hints');
    this.branches = ydoc.getMap('branches');
    this.counters = ydoc.getMap('counters');
    this.undoManager = new Y.UndoManager([this.cells, this.meta, this.hints], {
      trackedOrigins: new Set([LOCAL, AI]),
      captureTimeout: 0,
    });
    ydoc.on('afterTransaction', (tr: Y.Transaction) => {
      if (tr.origin === HISTORY || !(tr.origin === LOCAL || tr.origin === AI || tr.origin === this.undoManager)) return;
      const changed = tr.changedParentTypes as Map<unknown, unknown>;
      if (!changed.has(this.cells) && !changed.has(this.meta) && !changed.has(this.hints)) return;
      const note = tr.origin === this.undoManager ? (this.undoManager.undoing ? 'undo' : 'redo') : (this.pendingNote ?? 'edit');
      const author = tr.origin === AI ? AI.author : LOCAL.author;
      this.pendingNote = null;
      queueMicrotask(() => this.record(note, author));
    });
  }

  // reading

  get name(): string {
    return (this.meta.get('name') as string) ?? 'Untitled';
  }
  get context(): string {
    return (this.meta.get('context') as string) ?? 'klr type=A2 nu=2i+j ring=QQ';
  }
  list(): CellData[] {
    return this.cells.toArray().map((m) => ({ id: m.get('id')!, text: m.get('text') ?? '' }));
  }
  index(id: string): number {
    return this.cells.toArray().findIndex((m) => m.get('id') === id);
  }
  versions(): Version[] {
    return this.history.toArray();
  }

  // changing: each call is one step

  /** Who makes the steps: the person, or the AI while it acts for them. */
  author: typeof LOCAL | typeof AI = LOCAL;

  step(note: string, fn: () => void) {
    this.pendingNote = note;
    this.ydoc.transact(fn, this.author);
  }

  /**
   * Runs `fn` with the AI as the author of its steps, and makes them one undo step: a multi-step
   * AI action undoes as one.
   */
  async asAI<T>(fn: () => Promise<T>): Promise<T> {
    const um = this.undoManager;
    um.stopCapturing();
    this.author = AI;
    um.captureTimeout = Number.MAX_SAFE_INTEGER;
    try {
      return await fn();
    } finally {
      this.author = LOCAL;
      um.captureTimeout = 0;
      um.stopCapturing();
    }
  }

  rename(name: string) {
    this.step(`rename to ${name}`, () => this.meta.set('name', name));
  }

  /** `context.set` */
  setContext(context: string) {
    this.step(`context ${context}`, () => this.meta.set('context', context));
  }

  freshId(kind: CellKind | null): string {
    const n = this.counters.get('nextId') ?? 1;
    this.counters.set('nextId', n + 1);
    return `${LETTER[kind ?? 'def']}${n}`;
  }

  /** `cell.add`: inserts a cell at `index` (the end by default) and returns its ID. */
  addCell(text: string, index = this.cells.length): string {
    let id = '';
    this.step(`add ${text.split('\n')[0].slice(0, 60)}`, () => {
      id = this.freshId(kindOf(text));
      const m = new Y.Map<string>();
      m.set('id', id);
      m.set('text', text);
      this.cells.insert(index, [m]);
    });
    return id;
  }

  /** `cell.update` */
  updateCell(id: string, text: string, note = `edit ${id}`) {
    const i = this.index(id);
    if (i < 0 || this.cells.get(i).get('text') === text) return;
    this.step(note, () => this.cells.get(i).set('text', text));
  }

  hint(cellId: string, word: string): Hint | undefined {
    return this.hints.get(`${cellId} ${word}`);
  }

  /** `layout.hint`: sets or clears how a summand is drawn. */
  setHint(cellId: string, word: string, hint: Hint | null, note: string) {
    const key = `${cellId} ${word}`;
    this.step(note, () => (hint && (hint.order || Object.keys(hint.offsets ?? {}).length) ? this.hints.set(key, hint) : this.hints.delete(key)));
  }

  /** `cell.remove` */
  removeCell(id: string) {
    const i = this.index(id);
    if (i >= 0) this.step(`remove ${id}`, () => this.cells.delete(i, 1));
  }

  /** `cell.move`: moves a cell to position `to` among the others. */
  moveCell(id: string, to: number) {
    const i = this.index(id);
    if (i < 0 || i === to) return;
    this.step(`move ${id}`, () => {
      const text = this.cells.get(i).get('text')!;
      this.cells.delete(i, 1);
      const m = new Y.Map<string>();
      m.set('id', id);
      m.set('text', text);
      this.cells.insert(Math.max(0, Math.min(to, this.cells.length)), [m]);
    });
  }

  /** Replaces the whole workspace, as one step (import, or restoring a version). */
  replace(context: string, cells: CellData[], note: string) {
    this.step(note, () => {
      this.meta.set('context', context);
      this.cells.delete(0, this.cells.length);
      this.cells.insert(0, cells.map(toMap));
      const max = Math.max(0, ...cells.map((c) => Number(c.id.slice(1)) || 0));
      this.counters.set('nextId', Math.max(max + 1, this.counters.get('nextId') ?? 1));
    });
  }

  /** `result.pin`: freezes a result as a snapshot (not undoable: snapshots are a record). */
  pin(s: Snapshot) {
    this.ydoc.transact(() => this.snapshots.push([s]), HISTORY);
  }

  undo() {
    this.undoManager.undo();
  }
  redo() {
    this.undoManager.redo();
  }

  /** The workspace as it was at a version, read-only. */
  at(n: number): { context: string; cells: CellData[] } | null {
    const v = this.history.toArray().find((h) => h.n === n);
    if (!v) return null;
    const old = Y.createDocFromSnapshot(this.ydoc, Y.decodeSnapshot(v.snapshot));
    const meta = old.getMap('meta');
    const cells = old.getArray<Y.Map<string>>('cells').toArray().map((m) => ({ id: m.get('id')!, text: m.get('text') ?? '' }));
    return { context: (meta.get('context') as string) ?? this.context, cells };
  }

  /** The workspace file (.dcw) of the current inputs, cells as entered. */
  toDcw(state = { context: this.context, cells: this.list() }): string {
    const lines = [FORMAT, `context ${state.context}`];
    for (const c of state.cells) {
      if (kindOf(c.text) === 'comment') lines.push(c.text.trim());
      else lines.push(`${c.text.trim()}  # id=${c.id}`);
    }
    return lines.join('\n') + '\n';
  }

  /** The current inputs. */
  inputs(): Inputs {
    return { context: this.context, cells: this.list(), hints: Object.fromEntries(this.hints.entries()) };
  }

  // branches

  /** `branch.create`: a scratch copy of the document as it is now. */
  createBranch(name: string): BranchRecord {
    const b: BranchRecord = {
      id: `b${this.counters.get('nextBranch') ?? 1}`,
      name,
      author: this.author.author,
      created: Date.now(),
      base: this.inputs(),
      state: Y.encodeStateAsUpdate(this.ydoc),
    };
    this.ydoc.transact(() => {
      this.counters.set('nextBranch', (this.counters.get('nextBranch') ?? 1) + 1);
      this.branches.set(b.id, b);
    }, HISTORY);
    return b;
  }

  /** The branch's own document; changes to it are kept with `saveBranch`. */
  openBranch(id: string): Doc | null {
    const b = this.branches.get(id);
    if (!b) return null;
    const ydoc = new Y.Doc({ gc: false });
    Y.applyUpdate(ydoc, b.state);
    const d = new Doc(ydoc);
    d.ydoc.transact(() => d.branches.clear(), HISTORY);
    return d;
  }

  saveBranch(id: string, d: Doc) {
    const b = this.branches.get(id);
    if (b) this.ydoc.transact(() => this.branches.set(id, { ...b, state: Y.encodeStateAsUpdate(d.ydoc) }), HISTORY);
  }

  /** `branch.discard` */
  discardBranch(id: string) {
    this.ydoc.transact(() => this.branches.delete(id), HISTORY);
  }

  /** `branch.compare`: what accepting the branch would change in the document now. */
  compareBranch(id: string, d: Doc): Comparison {
    const b = this.branches.get(id);
    if (!b) throw new Error(`no branch ${id}`);
    return merge(b.base, this.inputs(), d.inputs()).comparison;
  }

  /**
   * `branch.accept`: merges the branch's changes into the document as one step and removes the
   * branch. Changes made to the document since the branch was made are kept, except in cells the
   * branch also changed, where the branch's text wins (listed as conflicts).
   */
  acceptBranch(id: string, d: Doc): Comparison {
    const b = this.branches.get(id);
    if (!b) throw new Error(`no branch ${id}`);
    const { result, comparison } = merge(b.base, this.inputs(), d.inputs(), (kind) => this.freshId(kindOf(kind)));
    this.step(`accept branch ${b.name}`, () => this.setInputs(result));
    this.discardBranch(id);
    return comparison;
  }

  /** Makes the inputs equal to `target` as one step, changing as little as possible. */
  setInputsStep(target: Inputs, note: string) {
    this.step(note, () => this.setInputs(target));
  }

  /** Makes the inputs equal to `target` with the fewest changes (inside a step). */
  private setInputs(target: Inputs) {
    if (this.context !== target.context) this.meta.set('context', target.context);
    for (const [k, h] of Object.entries(target.hints)) {
      if (JSON.stringify(this.hints.get(k)) !== JSON.stringify(h)) this.hints.set(k, h);
    }
    for (const k of [...this.hints.keys()]) if (!(k in target.hints)) this.hints.delete(k);
    const want = new Set(target.cells.map((c) => c.id));
    for (let i = this.cells.length - 1; i >= 0; i--) if (!want.has(this.cells.get(i).get('id')!)) this.cells.delete(i, 1);
    target.cells.forEach((c, i) => {
      const at = this.index(c.id);
      if (at === i) {
        if (this.cells.get(i).get('text') !== c.text) this.cells.get(i).set('text', c.text);
        return;
      }
      if (at >= 0) this.cells.delete(at, 1);
      this.cells.insert(i, [toMap(c)]);
    });
  }

  private record(note: string, author: string) {
    const n = this.history.length + 1;
    const snapshot = Y.encodeSnapshot(Y.snapshot(this.ydoc));
    this.ydoc.transact(() => {
      this.history.push([{ n, author, note, time: Date.now(), snapshot }]);
      this.meta.set('edited', Date.now());
    }, HISTORY);
  }
}

function toMap(c: CellData): Y.Map<string> {
  const m = new Y.Map<string>();
  m.set('id', c.id);
  m.set('text', c.text);
  return m;
}

/**
 * Three-way merge of a branch into the document: `base` is the document when the branch was made,
 * `main` the document now, `branch` the branch now. A cell the branch added gets a fresh ID if the
 * document has since used its ID (`fresh` makes one).
 */
export function merge(base: Inputs, main: Inputs, branch: Inputs, fresh?: (text: string) => string): { result: Inputs; comparison: Comparison } {
  const baseText = new Map(base.cells.map((c) => [c.id, c.text]));
  const mainText = new Map(main.cells.map((c) => [c.id, c.text]));
  const branchIds = new Set(branch.cells.map((c) => c.id));
  const comparison: Comparison = { added: [], removed: [], changed: [], hints: 0, conflicts: [] };
  const context = branch.context !== base.context ? branch.context : main.context;
  if (context !== main.context) comparison.context = { from: main.context, to: context };

  // cells the branch removed go; cells it changed take its text
  const cells: CellData[] = [];
  for (const c of main.cells) {
    if (baseText.has(c.id) && !branchIds.has(c.id)) {
      comparison.removed.push(c);
      continue;
    }
    const bt = branch.cells.find((x) => x.id === c.id)?.text;
    if (bt !== undefined && baseText.has(c.id) && bt !== baseText.get(c.id) && bt !== c.text) {
      if (c.text !== baseText.get(c.id)) comparison.conflicts.push(c.id);
      comparison.changed.push({ id: c.id, from: c.text, to: bt });
      cells.push({ id: c.id, text: bt });
    } else cells.push(c);
  }
  // cells the branch added go after the nearest cell before them in the branch that is kept
  const ids = new Map<string, string>();
  branch.cells.forEach((c, i) => {
    if (baseText.has(c.id)) return;
    const id = mainText.has(c.id) && fresh ? fresh(c.text) : c.id;
    ids.set(c.id, id);
    const added = { id, text: c.text };
    comparison.added.push(added);
    const prev = branch.cells.slice(0, i).reverse().map((p) => ids.get(p.id) ?? p.id).find((p) => cells.some((x) => x.id === p));
    cells.splice(prev ? cells.findIndex((x) => x.id === prev) + 1 : 0, 0, added);
  });

  const hints = { ...main.hints };
  const keys = new Set([...Object.keys(base.hints), ...Object.keys(branch.hints)]);
  for (const k of keys) {
    const [b, n] = [JSON.stringify(base.hints[k]), JSON.stringify(branch.hints[k])];
    if (b === n) continue;
    if (branch.hints[k]) hints[k] = branch.hints[k];
    else delete hints[k];
    comparison.hints++;
  }
  return { result: { context, cells, hints }, comparison };
}
