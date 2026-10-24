// The document: a Yjs CRDT holding the workspace's inputs (context and cells), the pinned
// snapshots. Private versions and branches live in a separate sidecar, never on the wire.
//
// Every change goes through `step`, so it is one undoable step with an author and a note, and
// it is recorded as a version in the history.
//
// A shared document's content comes from every editor, so nothing in it is trusted to have the
// shape this code writes: every read checks it and skips or replaces what does not fit, so a
// malformed or hostile update cannot stop the document from opening (`problems` says what was
// skipped, and `cleanCopy` makes a fresh document without it). The history, its authors, and the
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
  /** Full Yjs state for GC-enabled checkpoints; absent on legacy snapshots. */
  update?: Uint8Array;
  retained?: boolean;
  authorKind?: 'person' | 'ai';
}

/** A frozen result: the value and everything needed to say how it was obtained. */
export interface Snapshot {
  author?: string;
  /** Just the cell and its dependencies, not unrelated private branch content. */
  run?: { workspace: string; scalars?: string[]; summands?: number };
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
  /**
   * The occurrence-ID counters (`occ-d`, …) when the branch was made: an ID numbered below them
   * is shared with the document, one numbered from them on was made in the branch and is
   * renumbered on accept. Missing in branches made before IDs were carried on accept.
   */
  counters?: Record<string, number>;
}

/** One line of the AI activity timeline. */
export interface TimelineEntry {
  time: number;
  author: string;
  authorKind?: 'person' | 'ai';
  kind: 'request' | 'tool' | 'note' | 'milestone' | 'proposal' | 'error';
  text: string;
  /** More detail: a tool's input, a milestone's summary. */
  detail?: string;
  /** The version the step made, when it changed the workspace. */
  version?: number;
  /** The branch it happened on, if not the document itself. */
  branch?: string;
  /** A milestone's key diagrams: the `key`s of the snapshots that froze them. */
  snapshots?: string[];
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
 * The two letters of a cell ID by kind. Occurrence IDs of a diagram are one letter and a number
 * (d dot, c crossing, u cup, n cap, b bubble, g other piece, s strand; docs/spec.md, IDs), so a cell
 * ID never reads as one. Documents and files from before the two-letter prefixes keep their
 * one-letter IDs (d1, w2, …), which are read as they are; numbers are shared by every kind.
 */
export const LETTER: Record<CellKind, string> = { def: 'df', watch: 'wt', solve: 'sv', check: 'ck', note: 'nt', comment: 'rm' };

/** The number of a cell ID (`df12` and the older `d12` both give 12), or 0 if it has none. */
export const idNumber = (id: string | undefined): number => Number(/^[A-Za-z]*(\d+)$/.exec(id ?? '')?.[1] ?? 0) || 0;

export const DEFAULT_CONTEXT = 'klr type=A2 nu=2i+j ring=QQ';

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isStr = (v: unknown): v is string => typeof v === 'string';

/** A cell as stored: a map with a string ID (its text, if not a string, reads as empty). */
function isCell(m: unknown): m is Y.Map<unknown> {
  return m instanceof Y.Map && isStr(m.get('id'));
}
const cellData = (m: Y.Map<unknown>): CellData => {
  const text = m.get('text');
  return { id: m.get('id') as string, text: text instanceof Y.Text ? text.toString() : isStr(text) ? text : '' };
};

/** Occurrence IDs as stored: a letter and a number each. */
export function validIds(v: unknown): v is string[] {
  return Array.isArray(v) && v.every((x) => isStr(x) && /^[a-z][0-9]+$/.test(x));
}
const validWords = (v: unknown): v is string[] => Array.isArray(v) && v.every(isStr);

/**
 * The key, after the cell ID, under which a strand's ID is stored: its anchor (`bottom 2`,
 * `on u3`; src/canvas/occurrences.ts) after a bar, which no summand word starts with.
 */
export const strandKey = (anchor: string) => `|${anchor}`;

export function validVersion(v: unknown): v is Version {
  return isObj(v) && isNum(v.n) && isStr(v.author) && isStr(v.note) && isNum(v.time) && v.snapshot instanceof Uint8Array && (v.update === undefined || v.update instanceof Uint8Array);
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
    (v.branch === undefined || isStr(v.branch)) &&
    (v.snapshots === undefined || (Array.isArray(v.snapshots) && v.snapshots.length <= 16 && v.snapshots.every(isStr)))
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
  return (
    isObj(v) && isStr(v.id) && isStr(v.name) && isStr(v.author) && isNum(v.created) && validInputs(v.base) && v.state instanceof Uint8Array &&
    (v.counters === undefined || (isObj(v.counters) && Object.values(v.counters).every((n) => Number.isSafeInteger(n))))
  );
}

export class Doc {
  readonly ydoc: Y.Doc;
  readonly privateDoc: Y.Doc;
  readonly cellOrder: Y.Array<string>;
  readonly proposals: Y.Map<ProposalRecord>;
  readonly localOrigin = { author: 'you', kind: 'person' as const };
  readonly aiOrigin = { author: 'AI', kind: 'ai' as const };
  readonly meta: Y.Map<unknown>;
  readonly cells: Y.Array<Y.Map<unknown>>;
  readonly history: Y.Array<Version>;
  readonly snapshots: Y.Array<Snapshot>;
  /** Layout hints, keyed by cell ID and summand word. */
  readonly hints: Y.Map<Hint>;
  /**
   * Generator-occurrence IDs (docs/spec.md, IDs), keyed like hints by cell ID and summand word:
   * one ID per layer of the word's canonical layering, from the bottom. Gestures carry them through
   * rewrites by the provenance map; a summand drawn without them takes them from the summands drawn
   * before it (`drawn`, by `term.diff`) or gets fresh ones. A strand's ID is kept under the cell ID
   * and `strandKey` of where it starts.
   */
  readonly occurrences: Y.Map<string[]>;
  /**
   * For each cell, the summand words its canvas last drew (the first 60): the summands a term
   * written in the cell's place takes its occurrence IDs from. A record of drawing, outside undo.
   */
  readonly drawn: Y.Map<string[]>;
  /** The AI activity timeline. */
  readonly timeline: Y.Array<TimelineEntry>;
  /** Scratch branches of this document (only the main document has any). */
  readonly branches: Y.Map<BranchRecord>;
  /** Counters that undo leaves alone, so a cell ID is never reused. */
  private readonly counters: Y.Map<number>;
  readonly undoManager: Y.UndoManager;
  private pendingNote: string | null = null;

  constructor(ydoc = new Y.Doc({ gc: true }), privateDoc = new Y.Doc()) {
    this.ydoc = ydoc;
    this.privateDoc = privateDoc;
    this.cellOrder = ydoc.getArray('cellOrder');
    this.proposals = ydoc.getMap('proposals');
    this.meta = ydoc.getMap('meta');
    this.cells = ydoc.getArray('cells');
    this.history = privateDoc.getArray('history');
    this.snapshots = ydoc.getArray('snapshots');
    this.hints = ydoc.getMap('hints');
    this.occurrences = ydoc.getMap('occurrences');
    this.drawn = ydoc.getMap('drawn');
    this.timeline = ydoc.getArray('timeline');
    this.branches = privateDoc.getMap('branches');
    this.counters = ydoc.getMap('counters');
    this.undoManager = new Y.UndoManager([this.cells, this.cellOrder, this.meta, this.hints, this.occurrences], {
      trackedOrigins: new Set([this.localOrigin, this.aiOrigin]),
      captureTimeout: 0,
    });
    ydoc.on('afterTransaction', (tr: Y.Transaction) => {
      if (tr.origin === HISTORY || !(tr.origin === this.localOrigin || tr.origin === this.aiOrigin || tr.origin === this.undoManager)) return;
      const changed = tr.changedParentTypes as Map<unknown, unknown>;
      if (!changed.has(this.cells) && !changed.has(this.cellOrder) && !changed.has(this.meta) && !changed.has(this.hints)) return;
      const note = tr.origin === this.undoManager ? (this.undoManager.undoing ? 'undo' : 'redo') : (this.pendingNote ?? 'edit');
      const author = tr.origin === this.aiOrigin ? this.aiOrigin.author : this.localOrigin.author;
      this.pendingNote = null;
      this.record(note, author);
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
    return this.orderedCells().map(cellData);
  }
  /** Stable row identity for UI ownership; public IDs can be repaired after a merge. */
  keyedList(): (CellData & { key: string })[] {
    return this.orderedCells().map((m) => ({ ...cellData(m), key: cellKey(m) }));
  }
  /** The position in `cells` of the cell `id` (counting entries that are not cells, which `list` skips). */
  index(id: string): number {
    return this.orderedCells().findIndex((m) => m.get('id') === id);
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
    const legacyHistory = this.ydoc.share.has('history') ? this.ydoc.getArray<Version>('history').toArray() : [];
    count('versions', this.history.length + legacyHistory.length, this.versions().length + legacyHistory.filter(validVersion).length);
    count('snapshots', this.snapshots.length, this.snapshotList().length);
    count('timeline entries', this.timeline.length, this.timelineList().length);
    const legacyBranches = this.ydoc.share.has('branches') ? [...this.ydoc.getMap<BranchRecord>('branches').values()] : [];
    count('branches', this.branches.size + legacyBranches.length, this.branchList().length + legacyBranches.filter(validBranch).length);
    count('layout hints', this.hints.size, [...this.hints.values()].filter(validHint).length);
    count('occurrence IDs', this.occurrences.size, [...this.occurrences.values()].filter(validIds).length);
    count('drawn summands', this.drawn.size, [...this.drawn.values()].filter(validWords).length);
    return out;
  }

  /**
   * A fresh document with the current content and nothing else: no deleted text, no history,
   * no branches, and nothing malformed. For repairing a damaged document, and for sharing a
   * document without what was ever deleted from it.
   */
  cleanCopy(name = this.name): Doc {
    const d = new Doc(new Y.Doc({ gc: true }));
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
      for (const [k, ids] of this.occurrences.entries()) if (validIds(ids)) d.occurrences.set(k, ids);
      for (const [k, words] of this.drawn.entries()) if (validWords(words)) d.drawn.set(k, words);
      for (const [k, n] of this.counters.entries()) if (k.startsWith('occ-') && Number.isSafeInteger(n)) d.counters.set(k, n);
      if (snapshots.length) d.snapshots.push(snapshots);
      d.counters.set('nextId', Math.max(this.nextId(), 1 + Math.max(0, ...cells.map((c) => idNumber(c.id)))));
    }, HISTORY);
    d.record('clean copy', LOCAL.author);
    return d;
  }

  /** Owner-only conversion; caller must publish a fresh generation, never merge into legacy. */
  migrateLegacy(): Doc {
    const d = this.cleanCopy();
    d.history.delete(0, d.history.length);
    Y.applyUpdate(d.privateDoc, Y.encodeStateAsUpdate(this.privateDoc));
    d.privateDoc.getMap('migration').set('legacyUpdate', Y.encodeStateAsUpdate(this.ydoc));
    const legacy = this.ydoc.share.has('history') ? this.ydoc.getArray<Version>('history').toArray().filter(validVersion) : [];
    for (const v of legacy) {
      try {
        const at = Y.createDocFromSnapshot(this.ydoc, Y.decodeSnapshot(v.snapshot));
        d.history.push([{ ...v, update: Y.encodeStateAsUpdate(at) }]);
        at.destroy();
      } catch { d.history.push([v]); }
    }
    if (this.ydoc.share.has('branches')) {
      for (const [id, b] of this.ydoc.getMap<BranchRecord>('branches')) if (validBranch(b)) d.branches.set(id, b);
    }
    for (const entry of this.timelineList()) d.timeline.push([entry]);
    for (const [id, p] of this.proposals) if (validProposal(p)) d.proposals.set(id, p);
    d.checkpoint('migration');
    return d;
  }

  get needsMigration(): boolean {
    return this.ydoc.share.has('history') || this.ydoc.share.has('branches') || this.cells.toArray().some(m => isCell(m) && !(m.get('text') instanceof Y.Text));
  }

  /** The next cell number; if the counter is malformed, one past every number in use. */
  private nextId(): number {
    const n = this.counters.get('nextId');
    return Number.isSafeInteger(n) && n! > 0 ? n! : 1 + Math.max(0, ...this.list().map((c) => idNumber(c.id)));
  }

  // changing: each call is one step

  /** Who makes the steps: the person, or the AI while it acts for them. */
  author: { author: string; kind: 'person' | 'ai' } = this.localOrigin;

  setIdentity(userId?: string) {
    this.localOrigin.author = userId || 'you';
    this.aiOrigin.author = userId || 'AI';
  }

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
    if (this.author === this.aiOrigin) return;
    this.undoManager.stopCapturing();
    this.author = this.aiOrigin;
    this.undoManager.captureTimeout = Number.MAX_SAFE_INTEGER;
  }

  endAI() {
    if (this.author !== this.aiOrigin) return;
    this.author = this.localOrigin;
    this.undoManager.captureTimeout = 0;
    this.undoManager.stopCapturing();
  }

  /** Adds a line to the activity timeline (a record, not an undoable step). */
  log(entry: Omit<TimelineEntry, 'time' | 'author'> & { author?: string }) {
    this.ydoc.transact(() => this.timeline.push([{ time: Date.now(), author: this.aiOrigin.author, authorKind: 'ai', ...entry }]), HISTORY);
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
      const m = toMap({ id, text });
      this.insertCell(m, index);
    });
    return id;
  }

  /**
   * `cell.update`. `ids`, when given, names the occurrence IDs of the new text's summands (by
   * word), given a way to make fresh ones; they are stored in the same step.
   */
  updateCell(id: string, text: string, note = `edit ${id}`, ids?: (fresh: (letter: string) => string) => Record<string, string[]>) {
    const i = this.index(id);
    if (i < 0 || this.list()[i].text === text) return;
    this.step(note, () => {
      setText(this.orderedCells()[i], text);
      if (ids) for (const [word, list] of Object.entries(ids((l) => this.freshOccurrence(l)))) this.occurrences.set(`${id} ${word}`, list);
    });
  }

  /** The occurrence IDs of a summand of a cell, one per canonical layer from the bottom. */
  occurrenceIds(cellId: string, word: string): string[] | undefined {
    const ids = this.occurrences.get(`${cellId} ${word}`);
    return validIds(ids) ? ids : undefined;
  }

  /** The ID of a cell's strand that starts at `anchor` (src/canvas/occurrences.ts, strandAnchors). */
  strandId(cellId: string, anchor: string): string | undefined {
    const ids = this.occurrences.get(`${cellId} ${strandKey(anchor)}`);
    return validIds(ids) && ids.length === 1 ? ids[0] : undefined;
  }

  /** The summand words the canvas of a cell last drew. */
  drawnWords(cellId: string): string[] | undefined {
    const w = this.drawn.get(cellId);
    return validWords(w) ? w : undefined;
  }

  /**
   * Records what the canvas drew: the occurrence IDs `ids` gives (keys after the cell ID: summand
   * words and strand keys), numbered on from every ID the document has used, and the words drawn.
   * Not an undoable step and not a version: it records what was drawn, as creation order needs,
   * and changes no input.
   */
  recordDrawn(cellId: string, ids: (fresh: (letter: string) => string) => Record<string, string[]>, words: string[]) {
    this.ydoc.transact(() => {
      for (const [k, list] of Object.entries(ids((l) => this.freshOccurrence(l)))) this.occurrences.set(`${cellId} ${k}`, list);
      const was = this.drawnWords(cellId);
      if (!was || was.length !== words.length || was.some((w, i) => w !== words[i])) this.drawn.set(cellId, words);
    }, HISTORY);
  }

  /** The occurrence-ID counters, by key (`occ-d`, …). */
  private occurrenceCounters(): Record<string, number> {
    return Object.fromEntries([...this.counters.entries()].filter(([k, n]) => k.startsWith('occ-') && Number.isSafeInteger(n)));
  }

  /** The next occurrence ID of a kind (d dot, c crossing, …); undo leaves the counter alone, so an ID is never reused. */
  private freshOccurrence(letter: string): string {
    const n = this.counters.get(`occ-${letter}`);
    const next = Number.isSafeInteger(n) && n! > 0 ? n! : 1;
    this.counters.set(`occ-${letter}`, next + 1);
    return `${letter}${next}`;
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
    const i = this.cells.toArray().findIndex(m => isCell(m) && m.get('id') === id);
    if (i >= 0) this.step(note, () => this.cells.delete(i, 1));
  }

  /** `cell.move`: moves a cell to position `to` among the others. */
  moveCell(id: string, to: number, note = `move ${id}`) {
    const i = this.index(id);
    if (i < 0 || i === to) return;
    this.step(note, () => {
      const cell = this.orderedCells()[i];
      const order = this.orderedCells().filter(m => m !== cell).map(cellKey);
      order.splice(Math.max(0, Math.min(to, order.length)), 0, cellKey(cell));
      this.writeOrder(order);
    });
  }

  /** Replaces the whole workspace, as one step (import, or restoring a version). */
  replace(context: string, cells: CellData[], note: string) {
    this.step(note, () => {
      this.meta.set('context', context);
      this.cells.delete(0, this.cells.length);
      this.cells.insert(0, cells.map(toMap));
      const max = Math.max(0, ...cells.map((c) => idNumber(c.id)));
      this.counters.set('nextId', Math.max(max + 1, this.nextId()));
    });
  }

  /** `result.pin`: freezes a result as a snapshot (not undoable: snapshots are a record). */
  pin(s: Snapshot) {
    this.ydoc.transact(() => this.snapshots.push([{ ...s, author: s.author ?? this.localOrigin.author }]), HISTORY);
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
      const old = v.update ? new Y.Doc() : Y.createDocFromSnapshot(this.ydoc, Y.decodeSnapshot(v.snapshot));
      if (v.update) Y.applyUpdate(old, v.update);
      const ctx = old.getMap('meta').get('context');
      const reader = new Doc(old);
      const cells = reader.list();
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

  /**
   * `branch.create`: a scratch copy of the document as it is now. A branch made from another
   * branch `from` (which then replaces its state) keeps the lower of the two counters, so the IDs
   * that branch made are new to the document too.
   */
  createBranch(name: string, from?: string): BranchRecord {
    const counters = this.occurrenceCounters();
    const parent = from === undefined ? undefined : this.branch(from);
    if (parent) {
      for (const k of Object.keys(counters)) counters[k] = Math.min(counters[k], parent.counters?.[k] ?? 1);
    }
    const b: BranchRecord = {
      id: `b${crypto.randomUUID()}`,
      name,
      author: this.author.author,
      created: Date.now(),
      base: this.inputs(),
      state: Y.encodeStateAsUpdate(this.ydoc),
      counters,
    };
    this.privateDoc.transact(() => {
      this.branches.set(b.id, b);
    }, HISTORY);
    return b;
  }

  /** The branch's own document; changes to it are kept with `saveBranch`. */
  openBranch(id: string): Doc | null {
    const b = this.branch(id);
    if (!b) return null;
    const ydoc = new Y.Doc({ gc: true });
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
    if (b) this.privateDoc.transact(() => this.branches.set(id, { ...b, state: Y.encodeStateAsUpdate(d.ydoc) }), HISTORY);
  }

  /** `branch.discard` */
  discardBranch(id: string) {
    this.privateDoc.transact(() => this.branches.delete(id), HISTORY);
  }

  /** `branch.compare`: what accepting the branch would change in the document now. */
  compareBranch(id: string, d: Doc): Comparison {
    const b = this.branch(id);
    if (!b) throw new Error(`no branch ${id}`);
    return merge(b.base, this.inputs(), d.inputs()).comparison;
  }

  /**
   * What accepting the branch would make the document, without changing anything: the merged
   * inputs and the comparison. A cell the branch added whose ID the document has since used gets a
   * temporary ID here (accepting gives it a fresh one).
   */
  previewBranch(id: string, d: Doc): { before: Inputs; after: Inputs; comparison: Comparison } {
    const b = this.branch(id);
    if (!b) throw new Error(`no branch ${id}`);
    const before = this.inputs();
    let n = 0;
    const { result, comparison } = merge(b.base, before, d.inputs(), () => `preview${++n}`);
    return { before, after: result, comparison };
  }

  /**
   * `branch.accept`: merges the branch's changes into the document as one step and removes the
   * branch. Changes made to the document since the branch was made are kept, except in cells the
   * branch also changed, where the branch's text wins (listed as conflicts).
   */
  acceptBranch(id: string, d: Doc): Comparison {
    const b = this.branch(id);
    if (!b) throw new Error(`no branch ${id}`);
    const renamed = new Map<string, string>();
    const { result, comparison } = merge(b.base, this.inputs(), d.inputs(), (kind) => this.freshId(kindOf(kind)), renamed);
    this.step(`accept branch ${b.name}`, () => {
      this.setInputs(result);
      this.carryOccurrences(b, d, comparison, renamed);
    });
    this.discardBranch(id);
    return comparison;
  }

  /**
   * Inside the accept step: the occurrence IDs (and drawn words) of the cells whose text came from
   * the branch, so a piece keeps the ID it had there. An ID the branch shares with the document
   * stays; one the branch made gets the document's next of its kind (the same one wherever it
   * occurs), since the document may have used that number since.
   */
  private carryOccurrences(b: BranchRecord, d: Doc, comparison: Comparison, renamed: Map<string, string>) {
    const cells = new Map<string, string>();
    for (const c of comparison.changed) cells.set(c.id, c.id);
    for (const c of comparison.added) cells.set([...renamed].find(([, to]) => to === c.id)?.[0] ?? c.id, c.id);
    const fresh = new Map<string, string>();
    for (const [from, to] of cells) {
      const own = new Set([...this.occurrences.entries()].filter(([k, v]) => k.startsWith(`${to} `) && validIds(v)).flatMap(([, v]) => v));
      const shared = (id: string) => (b.counters ? Number(id.slice(1)) < (b.counters[`occ-${id[0]}`] ?? 1) : own.has(id));
      const map = (id: string) => {
        if (shared(id)) return id;
        if (!fresh.has(id)) fresh.set(id, this.freshOccurrence(id[0]));
        return fresh.get(id)!;
      };
      for (const [k, ids] of d.occurrences.entries()) {
        if (!k.startsWith(`${from} `) || !validIds(ids)) continue;
        const key = `${to} ${k.slice(from.length + 1)}`;
        const now = this.occurrences.get(key);
        const next = ids.map(map);
        if (!validIds(now) || now.some((x, i) => x !== next[i]) || now.length !== next.length) this.occurrences.set(key, next);
      }
      // outside the undo scope: a record of drawing
      const words = d.drawnWords(from);
      if (words) this.drawn.set(to, words);
    }
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
      if (!isCell(m) || !want.has(m.get('id') as string)) this.cells.delete(i, 1);
    }
    target.cells.forEach((c, i) => {
      const at = this.index(c.id);
      if (at < 0) this.insertCell(toMap(c), i);
      else {
        setText(this.orderedCells()[at], c.text);
        if (at !== i) {
          const order = this.orderedCells().map(cellKey);
          const [key] = order.splice(at, 1);
          order.splice(i, 0, key);
          this.writeOrder(order);
        }
      }
    });
  }

  /** Explicit retained recovery checkpoint, outside the shared document. */
  checkpoint(note = 'checkpoint', author = this.localOrigin.author, retained = true): Version {
    const n = Math.max(0, ...this.versions().map(v => v.n)) + 1;
    const v = { n, author, authorKind: this.author.kind, note, retained, time: Date.now(), snapshot: new Uint8Array(), update: Y.encodeStateAsUpdate(this.ydoc) };
    this.history.push([v]);
    return v;
  }

  private record(note: string, author: string) {
    this.checkpoint(note, author, false);
    // Bound automatic recovery; explicitly retained checkpoints and imported history stay.
    let excess = this.versions().filter(v => v.retained === false).length - 50;
    for (let i = 0; excess > 0 && i < this.history.length;) {
      if (this.history.get(i)?.retained === false) { this.history.delete(i, 1); excess--; }
      else i++;
    }
    this.ydoc.transact(() => this.meta.set('edited', Date.now()), HISTORY);
  }

  private orderedCells(): Y.Map<unknown>[] {
    const cells = this.cells.toArray().filter(isCell);
    const order = this.cellOrder.toArray();
    const rank = new Map(order.map((key, i) => [key, i]));
    return cells.sort((a, b) => (rank.get(cellKey(a)) ?? Infinity) - (rank.get(cellKey(b)) ?? Infinity));
  }

  private writeOrder(keys: string[]) {
    // Splice only the changed range; concurrent untouched insertions survive.
    const old = this.cellOrder.toArray();
    let start = 0, end = 0;
    while (start < old.length && start < keys.length && old[start] === keys[start]) start++;
    while (end < old.length - start && end < keys.length - start && old[old.length - 1 - end] === keys[keys.length - 1 - end]) end++;
    if (old.length - start - end) this.cellOrder.delete(start, old.length - start - end);
    this.cellOrder.insert(start, keys.slice(start, keys.length - end));
  }

  private insertCell(cell: Y.Map<unknown>, index: number) {
    const keys = this.orderedCells().map(cellKey);
    this.cells.push([cell]);
    keys.splice(Math.max(0, Math.min(index, keys.length)), 0, cellKey(cell));
    this.writeOrder(keys);
  }

  cellText(id: string): Y.Text | undefined {
    const text = this.orderedCells().find(m => m.get('id') === id)?.get('text');
    return text instanceof Y.Text ? text : undefined;
  }

  proposalList(): ProposalRecord[] {
    return [...this.proposals.values()].filter(validProposal);
  }

  propose(record: ProposalRecord) {
    if (!validProposal(record)) throw new Error('invalid proposal');
    this.step('propose', () => this.proposals.set(record.id, record));
  }

}

function toMap(c: CellData): Y.Map<unknown> {
  const m = new Y.Map<unknown>();
  m.set('id', c.id);
  m.set('text', new Y.Text(c.text));
  m.set('key', crypto.randomUUID());
  return m;
}

/**
 * Three-way merge of a branch into the document: `base` is the document when the branch was made,
 * `main` the document now, `branch` the branch now. A cell the branch added gets a fresh ID if the
 * document has since used its ID (`fresh` makes one); `renamed` collects each added cell's ID in
 * the branch and in the result.
 */
export function merge(
  base: Inputs, main: Inputs, branch: Inputs, fresh?: (text: string) => string, renamed?: Map<string, string>,
): { result: Inputs; comparison: Comparison } {
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
    renamed?.set(c.id, id);
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

function cellKey(m: Y.Map<unknown>): string {
  return isStr(m.get('key')) ? m.get('key') as string : `legacy:${m.get('id')}`;
}

/** Preserve unchanged characters so disconnected commits merge instead of replacing text. */
function setText(m: Y.Map<unknown>, next: string) {
  const text = m.get('text');
  if (!(text instanceof Y.Text)) { m.set('text', new Y.Text(next)); return; }
  const old = text.toString();
  let start = 0, end = 0;
  while (start < old.length && start < next.length && old[start] === next[start]) start++;
  while (end < old.length - start && end < next.length - start && old[old.length - 1 - end] === next[next.length - 1 - end]) end++;
  if (old.length - start - end) text.delete(start, old.length - start - end);
  const insert = next.slice(start, next.length - end);
  if (insert) text.insert(start, insert);
}

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export interface ProposalRecord {
  id: string;
  author: string;
  time: number;
  title: string;
  payload: JsonValue;
  status: 'pending' | 'accepted' | 'rejected';
  decidedBy?: string;
}
export function validProposal(v: unknown): v is ProposalRecord {
  const json = (x: unknown): boolean => x === null || isStr(x) || typeof x === 'boolean' || isNum(x) || (Array.isArray(x) ? x.every(json) : isObj(x) && Object.getPrototypeOf(x) === Object.prototype && Object.values(x).every(json));
  try { return isObj(v) && isStr(v.id) && isStr(v.author) && isNum(v.time) && isStr(v.title) && ['pending', 'accepted', 'rejected'].includes(v.status as string) && json(v.payload) && (v.decidedBy === undefined || isStr(v.decidedBy)); } catch { return false; }
}
