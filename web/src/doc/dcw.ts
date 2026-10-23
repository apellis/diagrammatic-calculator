// Workspace files (.dcw) and history files (.dch) to and from documents.
import { engine } from '../app';
import type { CellData, Doc } from './model';
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
  let n = 0;
  const cells = r.cells
    .filter((c) => c.kind !== 'blank')
    .map((c) => ({ id: c.id ?? `c${1000 + n++}`, text: cellText(c) }));
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
