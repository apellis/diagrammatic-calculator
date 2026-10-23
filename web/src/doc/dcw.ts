// Workspace files (.dcw) and history files (.dch) to and from documents.
import * as Y from 'yjs';
import { engine } from '../app';
import { FORMAT, idNumber, LETTER, validEntry, validSnapshot, type CellData, type Doc, type Snapshot, type TimelineEntry } from './model';
import type { WorkspaceParseCell } from '../engine/ops.gen';

export interface Imported {
  context: string;
  cells: CellData[];
}

function cellText(c: WorkspaceParseCell): string {
  const opts = Object.entries(c.options).map(([k, v]) => `  ${k}=${v}`).join('');
  return c.text + opts;
}

/** Reads a .dcw file; blank lines are dropped, comments become comment cells. */
export async function parseDcw(text: string): Promise<Imported> {
  const o = await engine.call('workspace.parse', { workspace: text }, { lane: 'interactive' });
  if (o.status !== 'done') throw new Error('cancelled');
  const r = o.response;
  if (r.status !== 'ok' || !('cells' in r)) throw new Error('message' in r ? r.message : 'cannot read the file');
  // comments carry no ID in the file; they get the next numbers after the file's own IDs
  let n = 1 + Math.max(0, ...r.cells.map((c) => idNumber(c.id ?? undefined)));
  const cells = r.cells
    .filter((c) => c.kind !== 'blank')
    .map((c) => ({ id: c.id ?? `${LETTER.comment}${n++}`, text: cellText(c) }));
  return { context: r.context, cells };
}

/** The document as a .dcw file, in canonical form when every cell parses. */
export async function exportDcw(doc: Doc): Promise<{ text: string; canonical: boolean }> {
  const raw = doc.toDcw();
  const o = await engine.call('workspace.parse', { workspace: raw }, { lane: 'interactive' });
  if (o.status === 'done' && o.response.status === 'ok' && 'canonical' in o.response) return { text: o.response.canonical, canonical: true };
  return { text: raw, canonical: false };
}

/** The version history and snapshots as a .dch file (format diagcalc-history/1, see spec/history.md). */
export function exportDch(doc: Doc): string {
  const versions = doc.versions().map((v) => {
    const at = doc.at(v.n);
    return { n: v.n, author: v.author, note: v.note, time: iso(v.time), workspace: at ? doc.toDcw(at) : null };
  });
  const snapshots = doc.snapshotList().map((s) => ({ ...s, time: iso(s.time) }));
  const timeline = doc.timelineList().map((t) => ({ ...t, time: iso(t.time) }));
  return JSON.stringify({ format: 'diagcalc-history/1', name: doc.name, versions, snapshots, timeline }, null, 2) + '\n';
}

/** A time as ISO 8601, or null for one out of the Date range (shared content may hold anything). */
const iso = (t: number) => (Math.abs(t) <= 8.64e15 ? new Date(t).toISOString() : null);

export const fileName = (name: string, ext: string) => `${name.replace(/[^\w.-]+/g, '-').replace(/^-+|-+$/g, '') || 'workspace'}.${ext}`;

/** What a .dch file holds, read and checked: the versions that could be read, and what was left out. */
export interface History {
  name: string | null;
  versions: { n: number; author: string; note: string; time: number; workspace: Imported }[];
  snapshots: Snapshot[];
  timeline: TimelineEntry[];
  /** How many entries could not be read and were left out. */
  skipped: number;
}

/** Whether a file's text is a history file (.dch) rather than a workspace. */
export const isDch = (text: string) => /^\s*\{/.test(text) && text.includes('diagcalc-history/');

const time = (t: unknown): number | null => {
  const n = typeof t === 'string' ? Date.parse(t) : typeof t === 'number' ? t : NaN;
  return Number.isFinite(n) ? n : null;
};

/**
 * A version's workspace as written in a .dch file, cells as entered: through the engine when it
 * parses, and otherwise line by line (a cell is everything up to its `# id=`), so a version with a
 * cell that never parsed is still read as it was.
 */
async function readVersion(text: string): Promise<Imported | null> {
  try {
    return await parseDcw(text);
  } catch {
    const lines = text.split('\n');
    if (lines[0]?.trim() !== FORMAT || !lines[1]?.startsWith('context ')) return null;
    const cells: CellData[] = [];
    let pending: string[] = [];
    let n = 1 + Math.max(0, ...lines.map((l) => idNumber(/# id=([A-Za-z0-9]+)\s*$/.exec(l)?.[1])));
    for (const line of lines.slice(2)) {
      const m = /^(.*?)\s*# id=([A-Za-z0-9]+)\s*$/.exec(line);
      if (m) {
        cells.push({ id: m[2], text: [...pending, m[1]].join('\n') });
        pending = [];
      } else if (!pending.length && line.trim().startsWith('#')) cells.push({ id: `${LETTER.comment}${n++}`, text: line.trim() });
      else if (pending.length || line.trim()) pending.push(line);
    }
    return { context: lines[1].slice('context '.length).trim(), cells };
  }
}

/** Reads a history file (format diagcalc-history/1, spec/history.md). Throws when it is not one. */
export async function parseDch(text: string): Promise<History> {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    throw new Error('this is not a history file: it is not JSON');
  }
  const h = raw as { format?: unknown; name?: unknown; versions?: unknown; snapshots?: unknown; timeline?: unknown };
  if (!h || typeof h !== 'object' || h.format !== 'diagcalc-history/1') throw new Error('this is not a history file (format diagcalc-history/1)');
  let skipped = 0;
  const versions: History['versions'] = [];
  for (const v of Array.isArray(h.versions) ? h.versions : []) {
    const x = v as { n?: unknown; author?: unknown; note?: unknown; time?: unknown; workspace?: unknown };
    const t = time(x?.time);
    const ws = typeof x?.workspace === 'string' ? await readVersion(x.workspace) : null;
    if (!ws || t === null) {
      skipped++;
      continue;
    }
    versions.push({ n: versions.length + 1, author: typeof x.author === 'string' ? x.author : 'you', note: typeof x.note === 'string' ? x.note : 'edit', time: t, workspace: ws });
  }
  const snapshots: Snapshot[] = [];
  for (const s of Array.isArray(h.snapshots) ? h.snapshots : []) {
    const v = { ...(s as object), time: time((s as { time?: unknown })?.time) };
    if (validSnapshot(v)) snapshots.push(v);
    else skipped++;
  }
  const timeline: TimelineEntry[] = [];
  for (const e of Array.isArray(h.timeline) ? h.timeline : []) {
    const v = { ...(e as object), time: time((e as { time?: unknown })?.time) };
    if (validEntry(v)) timeline.push(v);
    else skipped++;
  }
  if (!versions.length) throw new Error('the history file has no version that can be read');
  return { name: typeof h.name === 'string' && h.name.trim() ? h.name.trim() : null, versions, snapshots, timeline, skipped };
}

/** Records changes that are neither steps nor undoable: the history being rebuilt. */
const IMPORT = { author: 'import' };

/**
 * Fills an empty document with a history: each version becomes a version again, with its author,
 * note and time, and the workspace as it was then; the last one is the document's state. Pinned
 * snapshots and the AI timeline come along. Cell numbers used anywhere in the history are never
 * given to new cells.
 */
export function fillFromHistory(d: Doc, h: History) {
  const counters = d.ydoc.getMap<number>('counters');
  let max = 0;
  for (const v of h.versions) {
    d.ydoc.transact(() => {
      d.meta.set('context', v.workspace.context);
      d.cells.delete(0, d.cells.length);
      d.cells.insert(
        0,
        v.workspace.cells.map((c) => {
          const m = new Y.Map<unknown>();
          m.set('id', c.id);
          m.set('text', new Y.Text(c.text));
          m.set('key', crypto.randomUUID());
          return m;
        }),
      );
    }, IMPORT);
    max = Math.max(max, ...v.workspace.cells.map((c) => idNumber(c.id)));
    const snapshot = new Uint8Array();
    const update = Y.encodeStateAsUpdate(d.ydoc);
    d.ydoc.transact(() => {
      d.history.push([{ n: v.n, author: v.author, note: v.note, time: v.time, snapshot, update }]);
      d.meta.set('edited', v.time);
    }, IMPORT);
  }
  d.ydoc.transact(() => {
    counters.set('nextId', max + 1);
    if (h.snapshots.length) d.snapshots.push(h.snapshots);
    if (h.timeline.length) d.timeline.push(h.timeline);
  }, IMPORT);
}

