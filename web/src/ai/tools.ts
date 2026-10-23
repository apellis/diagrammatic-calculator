// The AI's tools: the engine operations (generated from spec/ops) and the workspace operations
// (cells, context, branches, the timeline), and how the page runs each call.
//
// Every tool input is checked against its schema before it runs, since streamed tool inputs are
// not validated by the API. Every call that changes the workspace is one step with the AI as
// author and the model's note, and is logged in the activity timeline. Removing cells and
// accepting a branch are only proposed: the person accepts or rejects them.
import type { Calls } from '../eval/evaluator';
import type { CellState } from '../eval/evaluator';
import type { Comparison, Doc, TimelineEntry } from '../doc/model';
import { kindOf } from '../doc/model';
import { OP_TOOLS } from './optools.gen';
import type { OpName } from '../engine/ops.gen';

export interface ToolDef {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/** What the AI sees of a selection: references resolved when the message is sent. */
export interface SelectionRef {
  version: number;
  cell: string;
  summand: number;
  word: string;
  text: string;
  layers: number[];
  rectangle?: { layers: [number, number]; strands: [number, number] };
}

/** A change the AI proposes and the person accepts or rejects. */
export interface Proposal {
  id: number;
  /** `write`: any change to a document someone else owns, which waits for the person. */
  kind: 'remove' | 'accept-branch' | 'write';
  /** The cell to remove, the branch to accept, or what the write does. */
  target: string;
  note: string;
  state: 'open' | 'accepted' | 'rejected';
}

/** What the page gives the tools to act on. */
export interface ToolHost {
  engine: Calls;
  /** The document itself. */
  main: Doc;
  /** What is shown: the document or a branch of it. */
  doc(): Doc;
  branch(): { id: string; name: string } | null;
  createBranch(name: string): string;
  openBranch(id: string | null): void;
  compareBranch(): Comparison | null;
  discardBranch(): void;
  /** Cell states once every cell is computed (or after a time limit). */
  settled(): Promise<Map<string, CellState>>;
  termContext(): string;
  selection(): SelectionRef | null;
  /** `apply` makes a `write` proposal's change once the person accepts it. */
  propose(p: Omit<Proposal, 'id' | 'state'>, apply?: () => void): Proposal;
  /**
   * Whether the document belongs to someone else (opened from a share or a link). Its text may
   * have been written to steer the AI, so every change the AI makes to it waits for the person,
   * and the AI's notes are not written into it.
   */
  foreign(): boolean;
}

const note = { type: 'string', description: 'One line saying why, shown in the history and the activity timeline.' };

const WORKSPACE_TOOLS: ToolDef[] = [
  {
    name: 'workspace_read',
    description: 'The context, the branch shown (if any), and every cell with its ID, kind, text, status and value. Waits until the values are computed.',
    input_schema: { type: 'object', properties: { cells: { type: 'array', items: { type: 'string' }, description: 'Only these cell IDs.' } } },
  },
  {
    name: 'selection_read',
    description: "What the person has selected in the canvas: version, cell, summand, word, layers and the rectangle of layers × strands. Null when nothing is selected.",
    input_schema: { type: 'object', properties: {} },
  },
  {
    name: 'cell_add',
    description: 'Adds a cell (def, watch, solve, check or note, in the workspace syntax) and returns its ID.',
    input_schema: {
      type: 'object',
      required: ['text', 'note'],
      properties: {
        text: { type: 'string', description: 'The cell, e.g. "def a := x_1 e(iij)" or "check idempotent(a)".' },
        after: { type: 'string', description: 'Put it after this cell ID; at the end by default.' },
        note,
      },
    },
  },
  {
    name: 'cell_update',
    description: "Replaces a cell's text.",
    input_schema: { type: 'object', required: ['id', 'text', 'note'], properties: { id: { type: 'string' }, text: { type: 'string' }, note } },
  },
  {
    name: 'cell_move',
    description: 'Moves a cell to a position (0 is the first).',
    input_schema: { type: 'object', required: ['id', 'index', 'note'], properties: { id: { type: 'string' }, index: { type: 'integer', minimum: 0 }, note } },
  },
  {
    name: 'cell_remove',
    description: 'Proposes removing a cell; the person accepts or rejects the proposal.',
    input_schema: { type: 'object', required: ['id', 'note'], properties: { id: { type: 'string' }, note } },
  },
  {
    name: 'context_set',
    description: 'Changes the context line (presentation and options); every cell is recomputed.',
    input_schema: { type: 'object', required: ['context', 'note'], properties: { context: { type: 'string', description: 'Without the word "context".' }, note } },
  },
  {
    name: 'branch_create',
    description: 'Starts a scratch branch from what is shown and shows it; later edits happen on the branch until it is accepted, discarded or left.',
    input_schema: { type: 'object', required: ['name'], properties: { name: { type: 'string', description: 'A short name, e.g. "slide the dot down".' } } },
  },
  {
    name: 'branch_compare',
    description: 'What accepting the branch shown would change in the document: cells changed, added and removed, and conflicts.',
    input_schema: { type: 'object', properties: {} },
  },
  {
    name: 'branch_accept',
    description: 'Proposes accepting the branch shown into the document; the person confirms.',
    input_schema: { type: 'object', required: ['note'], properties: { note } },
  },
  {
    name: 'branch_discard',
    description: 'Discards the branch shown and returns to the document.',
    input_schema: { type: 'object', required: ['note'], properties: { note } },
  },
  {
    name: 'branch_leave',
    description: 'Returns to the document, keeping the branch for later.',
    input_schema: { type: 'object', properties: {} },
  },
  {
    name: 'timeline_note',
    description: 'Adds a one-line note to the activity timeline, saying what you are doing next.',
    input_schema: { type: 'object', required: ['text'], properties: { text: { type: 'string' } } },
  },
  {
    name: 'watch_pin',
    description: 'Pins a watch cell as a measure of progress; pinned watches sit at the top of the outline.',
    input_schema: { type: 'object', required: ['id'], properties: { id: { type: 'string' }, unpin: { type: 'boolean' } } },
  },
  {
    name: 'milestone_write',
    description:
      'Writes a short summary at a milestone into the activity timeline, with its key diagrams: the current values of up to four def or watch cells, each frozen as a snapshot so a later edit does not change what the milestone reports.',
    input_schema: {
      type: 'object',
      required: ['title', 'summary'],
      properties: {
        title: { type: 'string' },
        summary: { type: 'string' },
        cells: { type: 'array', items: { type: 'string' }, maxItems: 4, description: 'IDs of the def or watch cells whose values are the key diagrams.' },
      },
    },
  },
];

const OPS = new Map(OP_TOOLS.map((t) => [t.name, t.op]));

/** One line saying what a workspace write does, for the person to confirm. */
function describeWrite(name: string, a: Record<string, unknown>): string {
  const short = (s: unknown) => String(s ?? '').split('\n')[0].slice(0, 120);
  switch (name) {
    case 'cell_add':
      return `add the cell “${short(a.text)}”`;
    case 'cell_update':
      return `change ${short(a.id)} to “${short(a.text)}”`;
    case 'cell_move':
      return `move ${short(a.id)} to position ${short(a.index)}`;
    case 'context_set':
      return `set the context to “${short(a.context)}”`;
    case 'branch_create':
      return `start the branch “${short(a.name)}”`;
    case 'branch_discard':
      return 'discard the branch shown';
    case 'watch_pin':
      return `${a.unpin ? 'unpin' : 'pin'} ${short(a.id)}`;
  }
  return name;
}

/** Every tool, in a stable order (the list is part of the cached prompt prefix). */
export const TOOLS: ToolDef[] = [...WORKSPACE_TOOLS, ...OP_TOOLS.map(({ name, description, input_schema }) => ({ name, description, input_schema }))];

export interface ToolResult {
  content: string;
  isError?: boolean;
  /** A one-line summary for the chat and the timeline. */
  summary: string;
}

/** Checks a value against the subset of JSON Schema the tool schemas use; returns the first problem. */
export function validate(schema: Record<string, unknown>, v: unknown, at = 'input'): string | null {
  const s = schema as {
    type?: string; properties?: Record<string, Record<string, unknown>>; required?: string[]; items?: Record<string, unknown>;
    enum?: unknown[]; const?: unknown; minimum?: number; minItems?: number; maxItems?: number; oneOf?: Record<string, unknown>[]; anyOf?: Record<string, unknown>[];
  };
  if (s.oneOf || s.anyOf) {
    const alts = s.oneOf ?? s.anyOf!;
    return alts.some((a) => validate(a, v, at) === null) ? null : `${at} matches none of its allowed forms`;
  }
  if (s.const !== undefined && v !== s.const) return `${at} must be ${JSON.stringify(s.const)}`;
  if (s.enum && !s.enum.includes(v)) return `${at} must be one of ${s.enum.map((x) => JSON.stringify(x)).join(', ')}`;
  switch (s.type) {
    case 'object': {
      if (!v || typeof v !== 'object' || Array.isArray(v)) return `${at} must be an object`;
      const o = v as Record<string, unknown>;
      for (const r of s.required ?? []) if (o[r] === undefined) return `${at}.${r} is missing`;
      for (const [k, x] of Object.entries(o)) {
        const p = s.properties?.[k];
        if (p) {
          const e = validate(p, x, `${at}.${k}`);
          if (e) return e;
        }
      }
      return null;
    }
    case 'array': {
      if (!Array.isArray(v)) return `${at} must be an array`;
      if (s.minItems !== undefined && v.length < s.minItems) return `${at} needs at least ${s.minItems} items`;
      if (s.maxItems !== undefined && v.length > s.maxItems) return `${at} takes at most ${s.maxItems} items`;
      for (let i = 0; i < v.length; i++) {
        const e = s.items ? validate(s.items, v[i], `${at}[${i}]`) : null;
        if (e) return e;
      }
      return null;
    }
    case 'string':
      return typeof v === 'string' ? null : `${at} must be a string`;
    case 'integer':
      if (typeof v !== 'number' || !Number.isInteger(v)) return `${at} must be an integer`;
      return s.minimum !== undefined && v < s.minimum ? `${at} must be at least ${s.minimum}` : null;
    case 'number':
      return typeof v === 'number' ? null : `${at} must be a number`;
    case 'boolean':
      return typeof v === 'boolean' ? null : `${at} must be true or false`;
  }
  return null;
}

const MAX_RESULT = 24_000;
const clip = (s: string, n = MAX_RESULT) => (s.length > n ? `${s.slice(0, n)}… [${s.length - n} more characters left out]` : s);

/** A cell's value, compactly, for the AI. */
export function cellView(id: string, text: string, s: CellState | undefined): Record<string, unknown> {
  const out: Record<string, unknown> = { id, kind: s?.parsed?.kind ?? kindOf(text) ?? 'def', text, status: s?.status ?? 'unknown' };
  if (s?.parseError) return { ...out, error: s.parseError };
  const v = s?.value as Record<string, unknown> | undefined;
  if (!v) return out;
  if (v.status !== 'ok') return { ...out, error: v.message };
  const pick = (...ks: string[]) => Object.fromEntries(ks.filter((k) => v[k] !== undefined).map((k) => [k, typeof v[k] === 'string' ? clip(v[k] as string, 4000) : v[k]]));
  const p = v.provenance as { strategy?: string; certificate?: { status?: string }; notes?: string[] } | undefined;
  const provenance = p && { strategy: p.strategy, certificate: p.certificate?.status, notes: p.notes };
  switch (v.kind) {
    case 'def':
      return { ...out, ...pick('name', 'terms', 'term'), provenance };
    case 'watch': {
      const degrees = [...new Set(((v.summands as { degree: number }[] | undefined) ?? []).map((x) => x.degree))].sort((a, b) => a - b);
      return { ...out, ...pick('terms', 'normal_form'), degrees, provenance };
    }
    case 'check':
      return { ...out, ...pick('holds', 'witness'), provenance };
    case 'solve':
      return { ...out, ...pick('outcome', 'variables', 'solution', 'particular', 'directions', 'dimension', 'points', 'equations'), provenance };
  }
  return out;
}

function log(host: ToolHost, e: Omit<TimelineEntry, 'time' | 'author'>) {
  // nothing the AI writes reaches someone else's document unless the person confirms it
  if (host.foreign()) return;
  const b = host.branch();
  host.main.log({ ...e, ...(b ? { branch: b.name } : {}) });
}

const ASK_CONTEXT = 'diagcalc.ai.askBeforeContext';

/**
 * Whether the AI's context changes on the person's own documents wait for the person (a setting
 * of this browser, off by default; on someone else's document every change waits anyway).
 */
export function askBeforeContext(): boolean {
  try {
    return localStorage.getItem(ASK_CONTEXT) === '1';
  } catch {
    return false;
  }
}

export function setAskBeforeContext(on: boolean) {
  try {
    if (on) localStorage.setItem(ASK_CONTEXT, '1');
    else localStorage.removeItem(ASK_CONTEXT);
  } catch {
    // storage blocked: the default (no confirmation) stays
  }
}

/** Tools that change the document (or its branches), and so wait for the person on someone else's document. */
const WRITES = new Set(['cell_add', 'cell_update', 'cell_move', 'context_set', 'branch_create', 'branch_discard', 'watch_pin']);

/** Runs one tool call. Never throws: failures come back as error results the model can read. */
export async function runTool(host: ToolHost, name: string, input: unknown): Promise<ToolResult> {
  const def = TOOLS.find((t) => t.name === name);
  if (!def) return { content: `There is no tool named ${name}.`, isError: true, summary: `unknown tool ${name}` };
  const problem = validate(def.input_schema, input);
  if (problem) return { content: JSON.stringify({ INVALID_INPUT: problem, received: input }), isError: true, summary: `${name}: ${problem}` };
  const a = input as Record<string, unknown>;
  try {
    const op = OPS.get(name);
    if (op) return await runOp(host, op, a);
    return await runWorkspace(host, name, a);
  } catch (e) {
    return { content: `The tool failed: ${(e as Error).message}`, isError: true, summary: `${name} failed` };
  }
}

async function runOp(host: ToolHost, op: OpName, a: Record<string, unknown>): Promise<ToolResult> {
  const request = { ...a, context: (a.context as string | undefined) ?? host.termContext() };
  const o = await host.engine.call(op, request as never, { lane: 'job', timeoutMs: 60_000 });
  if (o.status !== 'done') {
    const why = o.status === 'cancelled' ? 'was cancelled' : `stopped: ${o.message}`;
    return { content: `The operation ${why}.`, isError: true, summary: `${op} ${why}` };
  }
  const r = o.response as Record<string, unknown>;
  const ok = r.status === 'ok';
  const what = typeof r.terms === 'number' ? `${r.terms} terms` : typeof r.term === 'string' ? clip(r.term as string, 80) : ok ? 'ok' : String(r.message ?? 'error');
  return { content: clip(JSON.stringify(r)), isError: !ok, summary: `${op}: ${what}` };
}

async function runWorkspace(host: ToolHost, name: string, a: Record<string, unknown>, confirmed = false): Promise<ToolResult> {
  const doc = host.doc();
  const n = a.note as string | undefined;
  const ok = (summary: string, content: unknown = { status: 'ok' }) => ({ content: typeof content === 'string' ? content : JSON.stringify(content), summary });
  const fail = (message: string) => ({ content: message, isError: true, summary: message });
  const stepped = (summary: string, content?: unknown) => {
    log(host, { kind: 'tool', text: n ?? summary, detail: summary, version: doc.versions().length + 1 });
    return ok(summary, content);
  };
  const foreign = WRITES.has(name) && host.foreign();
  if ((foreign || (name === 'context_set' && askBeforeContext())) && !confirmed) {
    const what = describeWrite(name, a);
    host.propose({ kind: 'write', target: what, note: n ?? what }, () => void runWorkspace(host, name, a, true));
    const why = foreign ? 'This document belongs to someone else, so' : 'The person asked to confirm every change of the context, so';
    return ok(`proposed: ${what}`, `${why} the change (${what}) is only proposed; the person will accept or reject it. Do not wait for the answer.`);
  }
  switch (name) {
    case 'workspace_read': {
      const states = await host.settled();
      const only = a.cells as string[] | undefined;
      const cells = host.doc().list().filter((c) => !only || only.includes(c.id)).map((c) => cellView(c.id, c.text, states.get(c.id)));
      const b = host.branch();
      return ok(`read ${cells.length} cells`, { context: host.doc().context, branch: b?.name ?? null, version: host.doc().versions().length, cells });
    }
    case 'selection_read':
      return ok('read the selection', { selection: host.selection() });
    case 'cell_add': {
      const text = (a.text as string).trim();
      if (!kindOf(text)) return fail('A cell starts with def, watch, solve, check or note.');
      const after = a.after as string | undefined;
      const at = after ? doc.index(after) + 1 : doc.cells.length;
      if (after && at === 0) return fail(`There is no cell ${after}.`);
      const id = doc.addCell(text, at, n);
      return stepped(`added ${id}: ${text.slice(0, 60)}`, { status: 'ok', id });
    }
    case 'cell_update': {
      const id = a.id as string;
      if (doc.index(id) < 0) return fail(`There is no cell ${id}.`);
      const text = (a.text as string).trim();
      if (!kindOf(text)) return fail('A cell starts with def, watch, solve, check or note.');
      doc.updateCell(id, text, n);
      return stepped(`changed ${id}`);
    }
    case 'cell_move': {
      const id = a.id as string;
      if (doc.index(id) < 0) return fail(`There is no cell ${id}.`);
      doc.moveCell(id, a.index as number, n);
      return stepped(`moved ${id}`);
    }
    case 'cell_remove': {
      const id = a.id as string;
      if (doc.index(id) < 0) return fail(`There is no cell ${id}.`);
      host.propose({ kind: 'remove', target: id, note: n! });
      log(host, { kind: 'proposal', text: n!, detail: `remove ${id}` });
      return ok(`proposed removing ${id}`, `Proposed removing ${id}; the person will accept or reject it. Do not wait for the answer.`);
    }
    case 'context_set': {
      const c = (a.context as string).replace(/^context\s+/, '').trim();
      const o = await host.engine.call('context.describe', { context: c }, { lane: 'interactive' });
      if (o.status === 'done' && o.response.status !== 'ok') return fail(`The context is not valid: ${o.response.message}`);
      doc.setContext(c, n);
      return stepped(`context ${c}`);
    }
    case 'branch_create': {
      const id = host.createBranch(a.name as string);
      log(host, { kind: 'tool', text: `branch “${a.name}”`, detail: 'branch_create' });
      return ok(`started branch ${a.name}`, { status: 'ok', branch: id });
    }
    case 'branch_compare': {
      const c = host.compareBranch();
      return c ? ok('compared the branch', c) : fail('No branch is shown.');
    }
    case 'branch_accept': {
      const b = host.branch();
      if (!b) return fail('No branch is shown.');
      host.propose({ kind: 'accept-branch', target: b.id, note: n! });
      log(host, { kind: 'proposal', text: n!, detail: `accept branch ${b.name}` });
      return ok(`proposed accepting ${b.name}`, `Proposed accepting “${b.name}”; the person will confirm or not. Do not wait for the answer.`);
    }
    case 'branch_discard': {
      const b = host.branch();
      if (!b) return fail('No branch is shown.');
      host.discardBranch();
      log(host, { kind: 'tool', text: n!, detail: `discard branch ${b.name}` });
      return ok(`discarded ${b.name}`);
    }
    case 'branch_leave':
      host.openBranch(null);
      return ok('back to the document');
    case 'timeline_note':
      log(host, { kind: 'note', text: a.text as string });
      return ok('noted', host.foreign() ? 'Not written: this document belongs to someone else, so the activity timeline is left alone.' : undefined);
    case 'watch_pin': {
      const id = a.id as string;
      const i = host.main.index(id);
      if (i < 0 || kindOf(host.main.list()[i].text) !== 'watch') return fail(`${id} is not a watch of the document.`);
      host.main.pinWatch(id, !a.unpin);
      return ok(`${a.unpin ? 'unpinned' : 'pinned'} ${id}`);
    }
    case 'milestone_write': {
      if (host.foreign()) return ok('milestone not written', 'Not written: this document belongs to someone else, so the activity timeline is left alone; tell the person in the chat instead.');
      const ids = (a.cells as string[] | undefined) ?? [];
      const states = ids.length ? await host.settled() : new Map<string, CellState>();
      const keys: string[] = [];
      for (const id of ids) {
        const s = states.get(id);
        const v = s?.value as { status?: string; kind?: string } | undefined;
        if (doc.index(id) < 0) return fail(`There is no cell ${id}.`);
        if (!s || s.status !== 'fresh' || !s.valueKey || v?.status !== 'ok' || !['def', 'watch'].includes(v.kind ?? '')) return fail(`${id} has no current value of a def or watch cell to show.`);
        // the same result already frozen is not frozen twice
        if (!host.main.snapshotList().some((x) => x.key === s.valueKey)) {
          host.main.pin({ name: `${a.title as string} (${id})`, cellId: id, cellText: s.text, context: doc.context, version: doc.versions().length, key: s.valueKey, value: s.value, time: Date.now() });
        }
        keys.push(s.valueKey);
      }
      log(host, { kind: 'milestone', text: a.title as string, detail: a.summary as string, ...(keys.length ? { snapshots: keys } : {}) });
      return ok(keys.length ? `milestone written with ${keys.length} ${keys.length === 1 ? 'diagram' : 'diagrams'}` : 'milestone written');
    }
  }
  return fail(`There is no tool named ${name}.`);
}
