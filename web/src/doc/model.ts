// The document: a Yjs CRDT holding the workspace's inputs (context and cells), the pinned
// snapshots and the version history. Results are never stored here (see src/eval/).
//
// Every change goes through `step`, so it is one undoable step with an author and a note, and
// it is recorded as a version in the history.
//
// A shared document's content comes from every editor, so nothing in it is trusted to have the
// shape this code writes: every read checks it and skips or replaces what does not fit, so a
// malformed or hostile update cannot stop the document from opening (`problems` says what was
// skipped, and `cleanCopy` makes a fresh document without it). The history, its authors and the
// timeline are a cooperative record, not evidence: any editor can write any entry.
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

/** One line of the AI activity timeline. */
export interface TimelineEntry {
  time: number;
  author: string;
  kind: 'request' | 'tool' | 'note' | 'milestone' | 'proposal' | 'error';
  text: string;
  /** More detail: a tool's input, a milestone's summary. */
  detail?: string;
  /** The version the step made, when it changed the workspace. */
  version?: number;
  /** The branch it happened on, if not the document itself. */
  branch?: string;
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

/**
 * The letter of a cell ID by kind. A comment's is r (remark), not c, which names crossings among the
 * occurrence IDs of a diagram (docs/spec.md, IDs).
 */
export const LETTER: Record<CellKind, string> = { def: 'd', watch: 'w', solve: 's', check: 'k', note: 'n', comment: 'r' };

export const DEFAULT_CONTEXT = 'klr type=A2 nu=2i+j ring=QQ';

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isStr = (v: unknown): v is string => typeof v === 'string';

/** A cell as stored: a map with a string ID (its text, if not a string, reads as empty). */
function isCell(m: unknown): m is Y.Map<string> {
  return m instanceof Y.Map && isStr(m.get('id'));
}
const cellData = (m: Y.Map<string>): CellData => {
  const text = m.get('text');
  return { id: m.get('id')!, text: isStr(text) ? text : '' };
};

export function validVersion(v: unknown): v is Version {
  return isObj(v) && isNum(v.n) && isStr(v.author) && isStr(v.note) && isNum(v.time) && v.snapshot instanceof Uint8Array;
}
export function validSnapshot(v: unknown): v is Snapshot {
  return isObj(v) && isStr(v.name) && isStr(v.cellId) && isStr(v.cellText) && isStr(v.context) && isNum(v.version) && isStr(v.key) && isNum(v.time);
}
const KINDS = new Set(['request', 'tool', 'note', 'milestone', 'proposal', 'error']);
export function validEntry(v: unknown): v is TimelineEntry {
  return (
    isObj(v) &&
    isNum(v.time) &&
    isStr(v.author) &&
    KINDS.has(v.kind as string) &&
    isStr(v.text) &&
    (v.detail === undefined || isStr(v.detail)) &&
    (v.version === undefined || isNum(v.version)) &&
    (v.branch === undefined || isStr(v.branch))
  );
}
export function validHint(v: unknown): v is Hint {
  if (!isObj(v)) return false;
  if (v.order !== undefined && !(Array.isArray(v.order) && v.order.every((x) => Number.isInteger(x)))) return false;
  if (v.offsets !== undefined && !(isObj(v.offsets) && Object.values(v.offsets).every((o) => Array.isArray(o) && o.length === 2 && o.every(isNum)))) return false;
  return true;
}
function validInputs(v: unknown): v is Inputs {
  return (
    isObj(v) &&
    isStr(v.context) &&
    Array.isArray(v.cells) &&
    v.cells.every((c) => isObj(c) && isStr(c.id) && isStr(c.text)) &&
    isObj(v.hints) &&
    Object.values(v.hints).every(validHint)
  );
}
export function validBranch(v: unknown): v is BranchRecord {
  return isObj(v) && isStr(v.id) && isStr(v.name) && isStr(v.author) && isNum(v.created) && validInputs(v.base) && v.state instanceof Uint8Array;
}

export class Doc {
  readonly ydoc: Y.Doc;
  readonly meta: Y.Map<unknown>;
  readonly cells: Y.Array<Y.Map<string>>;
  readonly history: Y.Array<Version>;
  readonly snapshots: Y.Array<Snapshot>;
  /** Layout hints, keyed by cell ID and summand word. */
  readonly hints: Y.Map<Hint>;
  /** The AI activity timeline. */
  readonly timeline: Y.Array<TimelineEntry>;
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
    this.timeline = ydoc.getArray('timeline');
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
    const n = this.meta.get('name');
    return isStr(n) ? n : 'Untitled';
  }
  get context(): string {
    const c = this.meta.get('context');
    return isStr(c) ? c : DEFAULT_CONTEXT;
  }
  /** When the document was last edited, if recorded. */
  get edited(): number | undefined {
    const e = this.meta.get('edited');
    return isNum(e) ? e : undefined;
  }
  list(): CellData[] {
    return this.cells.toArray().filter(isCell).map(cellData);
  }
  /** The position in `cells` of the cell `id` (counting entries that are not cells, which `list` skips). */
  index(id: string): number {
    return this.cells.toArray().findIndex((m) => isCell(m) && m.get('id') === id);
  }
  versions(): Version[] {
    return this.history.toArray().filter(validVersion);
  }
  snapshotList(): Snapshot[] {
    return this.snapshots.toArray().filter(validSnapshot);
  }
  timelineList(): TimelineEntry[] {
    return this.timeline.toArray().filter(validEntry);
  }
  branch(id: string): BranchRecord | undefined {
    const b = this.branches.get(id);
    return validBranch(b) ? b : undefined;
  }
  branchList(): BranchRecord[] {
    return [...this.branches.values()].filter(validBranch);
  }

  /**
   * What in the shared content does not have the shape this app writes (and is skipped when
   * reading); empty for a sound document.
   */
  problems(): string[] {
    const out: string[] = [];
    const count = (what: string, all: number, good: number) => all !== good && out.push(`${all - good} malformed ${what}`);
    if (this.meta.has('name') && !isStr(this.meta.get('name'))) out.push('a malformed name');
    if (this.meta.has('context') && !isStr(this.meta.get('context'))) out.push('a malformed context');
    count('cells', this.cells.length, this.list().length);
    count('versions', this.history.length, this.versions().length);
    count('snapshots', this.snapshots.length, this.snapshotList().length);
    count('timeline entries', this.timeline.length, this.timelineList().length);
    count('branches', this.branches.size, this.branchList().length);
    count('layout hints', this.hints.size, [...this.hints.values()].filter(validHint).length);
    return out;
  }

  /**
   * A fresh document with the current content and nothing else: no deleted text, no history,
   * no branches, and nothing malformed. For repairing a damaged document, and for sharing a
   * document without what was ever deleted from it.
   */
  cleanCopy(name = this.name): Doc {
    const d = new Doc(new Y.Doc({ gc: false }));
    const cells = this.list();
    const hints = Object.fromEntries([...this.hints.entries()].filter(([, h]) => validHint(h)));
    const snapshots = this.snapshotList();
    const pinned = this.pinned;
    d.ydoc.transact(() => {
      d.meta.set('name', name);
      d.meta.set('context', this.context);
      if (pinned.length) d.meta.set('pinned', pinned);
      d.cells.insert(0, cells.map(toMap));
      for (const [k, h] of Object.entries(hints)) d.hints.set(k, h);
      if (snapshots.length) d.snapshots.push(snapshots);
      d.counters.set('nextId', Math.max(this.nextId(), 1 + Math.max(0, ...cells.map((c) => Number(c.id.slice(1)) || 0))));
    }, HISTORY);
    d.record('clean copy', LOCAL.author);
    return d;
  }

  /** The next cell number; if the counter is malformed, one past every number in use. */
  private nextId(): number {
    const n = this.counters.get('nextId');
    return Number.isSafeInteger(n) && n! > 0 ? n! : 1 + Math.max(0, ...this.list().map((c) => Number(c.id.slice(1)) || 0));
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
    this.beginAI();
    try {
      return await fn();
    } finally {
      this.endAI();
    }
  }

  /** From now until `endAI`, steps are the AI's and undo as one. */
  beginAI() {
    if (this.author === AI) return;
    this.undoManager.stopCapturing();
    this.author = AI;
    this.undoManager.captureTimeout = Number.MAX_SAFE_INTEGER;
  }

  endAI() {
    if (this.author !== AI) return;
    this.author = LOCAL;
    this.undoManager.captureTimeout = 0;
    this.undoManager.stopCapturing();
  }

  /** Adds a line to the activity timeline (a record, not an undoable step). */
  log(entry: Omit<TimelineEntry, 'time' | 'author'> & { author?: string }) {
    this.ydoc.transact(() => this.timeline.push([{ time: Date.now(), author: AI.author, ...entry }]), HISTORY);
  }

  /** Watches the AI pinned as measures of its progress. */
  get pinned(): string[] {
    const p = this.meta.get('pinned');
    return Array.isArray(p) ? p.filter(isStr) : [];
  }
  pinWatch(id: string, pinned = true) {
    const now = this.pinned.filter((x) => x !== id);
    this.step(`${pinned ? 'pin' : 'unpin'} ${id}`, () => this.meta.set('pinned', pinned ? [...now, id] : now));
  }

  rename(name: string) {
    this.step(`rename to ${name}`, () => this.meta.set('name', name));
  }

  /** `context.set` */
  setContext(context: string, note = `context ${context}`) {
    this.step(note, () => this.meta.set('context', context));
  }

  freshId(kind: CellKind | null): string {
    const n = this.nextId();
    this.counters.set('nextId', n + 1);
    return `${LETTER[kind ?? 'def']}${n}`;
  }

  /** `cell.add`: inserts a cell at `index` (the end by default) and returns its ID. */
  addCell(text: string, index = this.cells.length, note = `add ${text.split('\n')[0].slice(0, 60)}`): string {
    let id = '';
    this.step(note, () => {
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
    const h = this.hints.get(`${cellId} ${word}`);
    return validHint(h) ? h : undefined;
  }

  /** `layout.hint`: sets or clears how a summand is drawn. */
  setHint(cellId: string, word: string, hint: Hint | null, note: string) {
    const key = `${cellId} ${word}`;
    this.step(note, () => (hint && (hint.order || Object.keys(hint.offsets ?? {}).length) ? this.hints.set(key, hint) : this.hints.delete(key)));
  }

  /** `cell.remove` */
  removeCell(id: string, note = `remove ${id}`) {
    const i = this.index(id);
    if (i >= 0) this.step(note, () => this.cells.delete(i, 1));
  }

  /** `cell.move`: moves a cell to position `to` among the others. */
  moveCell(id: string, to: number, note = `move ${id}`) {
    const i = this.index(id);
    if (i < 0 || i === to) return;
    this.step(note, () => {
      const text = cellData(this.cells.get(i)).text;
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
      this.counters.set('nextId', Math.max(max + 1, this.nextId()));
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

  /** The workspace as it was at a version, read-only; null if there is no such version or its snapshot cannot be read. */
  at(n: number): { context: string; cells: CellData[] } | null {
    const v = this.versions().find((h) => h.n === n);
    if (!v) return null;
    try {
      const old = Y.createDocFromSnapshot(this.ydoc, Y.decodeSnapshot(v.snapshot));
      const ctx = old.getMap('meta').get('context');
      const cells = old.getArray<unknown>('cells').toArray().filter(isCell).map(cellData);
      return { context: isStr(ctx) ? ctx : this.context, cells };
    } catch {
      return null;
    }
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
      id: `b${Number.isSafeInteger(this.counters.get('nextBranch')) ? this.counters.get('nextBranch') : 1}`,
      name,
      author: this.author.author,
      created: Date.now(),
      base: this.inputs(),
      state: Y.encodeStateAsUpdate(this.ydoc),
    };
    this.ydoc.transact(() => {
      const nb = this.counters.get('nextBranch');
      this.counters.set('nextBranch', (Number.isSafeInteger(nb) ? nb! : 1) + 1);
      this.branches.set(b.id, b);
    }, HISTORY);
    return b;
  }

  /** The branch's own document; changes to it are kept with `saveBranch`. */
  openBranch(id: string): Doc | null {
    const b = this.branch(id);
    if (!b) return null;
    const ydoc = new Y.Doc({ gc: false });
    try {
      Y.applyUpdate(ydoc, b.state);
    } catch {
      return null;
    }
    const d = new Doc(ydoc);
    d.ydoc.transact(() => d.branches.clear(), HISTORY);
    return d;
  }

  saveBranch(id: string, d: Doc) {
    const b = this.branch(id);
    if (b) this.ydoc.transact(() => this.branches.set(id, { ...b, state: Y.encodeStateAsUpdate(d.ydoc) }), HISTORY);
  }

  /** `branch.discard` */
  discardBranch(id: string) {
    this.ydoc.transact(() => this.branches.delete(id), HISTORY);
  }

  /** `branch.compare`: what accepting the branch would change in the document now. */
  compareBranch(id: string, d: Doc): Comparison {
    const b = this.branch(id);
    if (!b) throw new Error(`no branch ${id}`);
    return merge(b.base, this.inputs(), d.inputs()).comparison;
  }

  /**
   * `branch.accept`: merges the branch's changes into the document as one step and removes the
   * branch. Changes made to the document since the branch was made are kept, except in cells the
   * branch also changed, where the branch's text wins (listed as conflicts).
   */
  acceptBranch(id: string, d: Doc): Comparison {
    const b = this.branch(id);
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
    for (let i = this.cells.length - 1; i >= 0; i--) {
      const m = this.cells.get(i);
      if (!isCell(m) || !want.has(m.get('id')!)) this.cells.delete(i, 1);
    }
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
