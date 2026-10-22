// The document: a Yjs CRDT holding the workspace's inputs (context and cells), the pinned
// snapshots and the version history. Results are never stored here (see src/eval/).
//
// Every change goes through `step`, so it is one undoable step with an author and a note, and
// it is recorded as a version in the history.
import * as Y from 'yjs';

export const FORMAT = 'diagcalc-workspace/1';

/** The origin of changes made in this browser by the person using it. */
export const LOCAL = { author: 'you' };
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
    this.counters = ydoc.getMap('counters');
    this.undoManager = new Y.UndoManager([this.cells, this.meta, this.hints], {
      trackedOrigins: new Set([LOCAL]),
      captureTimeout: 0,
    });
    ydoc.on('afterTransaction', (tr: Y.Transaction) => {
      if (tr.origin === HISTORY || !(tr.origin === LOCAL || tr.origin === this.undoManager)) return;
      const changed = tr.changedParentTypes as Map<unknown, unknown>;
      if (!changed.has(this.cells) && !changed.has(this.meta) && !changed.has(this.hints)) return;
      const note = tr.origin === this.undoManager ? (this.undoManager.undoing ? 'undo' : 'redo') : (this.pendingNote ?? 'edit');
      this.pendingNote = null;
      queueMicrotask(() => this.record(note));
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

  step(note: string, fn: () => void) {
    this.pendingNote = note;
    this.ydoc.transact(fn, LOCAL);
  }

  rename(name: string) {
    this.step(`rename to ${name}`, () => this.meta.set('name', name));
  }

  /** `context.set` */
  setContext(context: string) {
    this.step(`context ${context}`, () => this.meta.set('context', context));
  }

  private freshId(kind: CellKind | null): string {
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

  private record(note: string) {
    const n = this.history.length + 1;
    const snapshot = Y.encodeSnapshot(Y.snapshot(this.ydoc));
    this.ydoc.transact(() => {
      this.history.push([{ n, author: LOCAL.author, note, time: Date.now(), snapshot }]);
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
