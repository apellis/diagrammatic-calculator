// Incremental evaluation of a workspace.
//
// Each cell's result is keyed by a hash of its inputs: the engine build, the context, the scalars
// it mentions (unknowns of solve cells), the cell's text, and the keys of the definitions it uses.
// An edit therefore changes the keys of exactly the cells downstream of it; every other cell, and
// any state reached again by undo, is a cache hit. A cell is evaluated by running a small
// workspace holding just the definitions it needs and itself, as a job that is cancelled when
// its inputs change again.
import type { CallOptions, Outcome } from '../engine/engine';
import type { OpName, OpTypes, WorkspaceParseCell, WorkspaceRunCell } from '../engine/ops.gen';
import type { CellData } from '../doc/model';
import { hash } from './hash';
import type { CacheStore } from './cache';

export interface Calls {
  call<O extends OpName>(op: O, request: OpTypes[O]['request'], opts?: CallOptions): Promise<Outcome<OpTypes[O]['response']>>;
}

/** A cell's computed value: the engine's answer, or why there is none. */
export type CellValue =
  | WorkspaceRunCell
  | { kind: 'error'; status: 'error'; message: string; error_kind?: string }
  | { kind: 'exploded'; status: 'exploded'; message: string };

export type Status =
  /** the value shown is the value of the current inputs */
  | 'fresh'
  /** the inputs changed; the value shown is an older one and a job is computing the new one */
  | 'stale'
  /** no value yet; a job is computing it */
  | 'running'
  | 'failed'
  | 'exploded'
  /** notes and comments: nothing to compute */
  | 'static';

export interface CellState {
  id: string;
  text: string;
  parsed?: WorkspaceParseCell;
  parseError?: string;
  key?: string;
  status: Status;
  value?: CellValue;
  /** The key of `value`: equal to `key` when the cell is fresh. */
  valueKey?: string;
  /** The value shown before the current one, for showing what changed. */
  previous?: CellValue;
  /** Milliseconds the engine took for the value. */
  ms?: number;
}

export interface EvaluatorOptions {
  /** Identifies the engine build, so a new build never reads an old build's results. */
  engineId: string;
  /** Summands drawn per definition and watch. */
  summands?: number;
  timeoutMs?: number;
}

type Parsed = { cell: WorkspaceParseCell } | { error: string };

export class Evaluator {
  private parses = new Map<string, Promise<Parsed>>();
  private jobs = new Map<string, AbortController>();
  private states = new Map<string, CellState>();
  private generation = 0;
  private current: { context: string; cells: CellData[] } = { context: '', cells: [] };

  constructor(
    private engine: Calls,
    private cache: CacheStore,
    private opts: EvaluatorOptions,
    /** Called with every cell whose state changed. */
    private onChange: (s: CellState) => void,
  ) {}

  state(id: string): CellState | undefined {
    return this.states.get(id);
  }

  /** Jobs running or waiting. */
  running(): number {
    return this.jobs.size;
  }

  /** Cancels every job; the cells they were computing show as failed with 'cancelled'. */
  cancelAll() {
    for (const a of this.jobs.values()) a.abort();
  }

  /** Computes a cell again, for example after it was cancelled. */
  rerun(id: string) {
    const s = this.states.get(id);
    if (!s) return;
    this.states.set(id, { ...s, key: undefined });
    void this.update(this.current.context, this.current.cells);
  }

  parse(text: string): Promise<Parsed> {
    let p = this.parses.get(text);
    if (!p) {
      p = this.engine.call('workspace.parse', { cell: text }, { lane: 'interactive' }).then((o) => {
        if (o.status !== 'done') return { error: 'cancelled' };
        const r = o.response;
        return r.status === 'ok' && 'cell' in r ? { cell: r.cell } : { error: 'message' in r ? r.message : 'error' };
      });
      this.parses.set(text, p);
    }
    return p;
  }

  /** Brings every cell's state up to date with the workspace's inputs. */
  async update(context: string, cells: CellData[]): Promise<void> {
    const gen = ++this.generation;
    this.current = { context, cells };
    const parsed = await Promise.all(cells.map((c) => this.parse(c.text)));
    if (gen !== this.generation) return;

    // the unknowns of solve cells are scalars of the workspace; each cell gets the ones it or the
    // definitions it uses mention, so a solve elsewhere never changes its value or its key
    const unknowns = new Set(parsed.flatMap((p) => ('cell' in p ? p.cell.unknowns ?? [] : [])));
    const keys: (string | undefined)[] = [];
    const deps: number[][] = [];
    const scalars: string[][] = [];
    const defined = new Map<string, number>(); // name → index of the latest definition so far
    cells.forEach((c, i) => {
      const p = parsed[i];
      deps[i] = [];
      scalars[i] = [];
      if (!('cell' in p) || p.cell.kind === 'note' || p.cell.kind === 'comment' || p.cell.kind === 'blank') return;
      const used = p.cell.uses.filter((n) => defined.has(n)).map((n) => defined.get(n)!);
      deps[i] = used;
      scalars[i] = [...new Set([...p.cell.uses.filter((n) => unknowns.has(n)), ...(p.cell.unknowns ?? []), ...used.flatMap((j) => scalars[j])])].sort();
      keys[i] = hash(JSON.stringify([this.opts.engineId, context, scalars[i], c.text.trim(), used.map((j) => keys[j] ?? 'unknown')]));
      if (p.cell.defines) defined.set(p.cell.defines, i);
    });

    const wanted = new Set<string>();
    for (let i = 0; i < cells.length; i++) {
      const c = cells[i];
      const p = parsed[i];
      const old = this.states.get(c.id);
      const s: CellState = { id: c.id, text: c.text, status: 'static', value: old?.value, valueKey: old?.valueKey, previous: old?.previous, ms: old?.ms };
      if ('error' in p) {
        s.parseError = p.error;
        s.status = 'failed';
        s.value = s.valueKey = undefined;
      } else {
        s.parsed = p.cell;
        s.key = keys[i];
        if (s.key) {
          const hit = await this.cache.get(s.key);
          if (gen !== this.generation) return;
          if (hit !== undefined) {
            this.show(s, old, s.key, hit as Stored);
          } else {
            s.status = old?.value && old.valueKey !== s.key ? 'stale' : 'running';
            if (old?.key === s.key && old.status !== 'stale' && old.status !== 'running') {
              s.status = old.status; // failed or exploded with these very inputs
            } else {
              wanted.add(s.key);
              this.start(s.key, this.miniWorkspace(context, cells, deps, i), cells[i].id, scalars[i]);
            }
          }
        }
      }
      this.set(s);
    }
    for (const id of [...this.states.keys()]) if (!cells.some((c) => c.id === id)) this.states.delete(id);
    // jobs no longer needed by any cell are cancelled
    for (const [key, a] of this.jobs) if (!wanted.has(key)) { a.abort(); this.jobs.delete(key); }
  }

  /** The workspace that evaluates cell i: the definitions it needs, in order, then the cell. */
  private miniWorkspace(context: string, cells: CellData[], deps: number[][], i: number): string {
    const need = new Set<number>();
    const visit = (j: number) => {
      for (const d of deps[j]) if (!need.has(d)) { need.add(d); visit(d); }
    };
    visit(i);
    const lines = ['diagcalc-workspace/1', `context ${context}`];
    for (const j of [...need].sort((a, b) => a - b)) lines.push(`${cells[j].text.trim()}  # id=${cells[j].id}`);
    lines.push(`${cells[i].text.trim()}  # id=${cells[i].id}`);
    return lines.join('\n') + '\n';
  }

  private start(key: string, workspace: string, id: string, scalars: string[]) {
    if (this.jobs.has(key)) return;
    const abort = new AbortController();
    this.jobs.set(key, abort);
    this.engine
      .call('workspace.run', { workspace, summands: this.opts.summands ?? 200, scalars }, { signal: abort.signal, timeoutMs: this.opts.timeoutMs ?? 120_000 })
      .then(async (o) => {
        if (this.jobs.get(key) === abort) this.jobs.delete(key);
        let stored: Stored;
        if (o.status === 'cancelled') {
          // cells still waiting on this key were stopped by the person, not by an edit
          for (const s of this.states.values()) {
            if (s.key === key && (s.status === 'running' || s.status === 'stale')) {
              this.set({ ...s, status: 'failed', previous: s.value, value: { kind: 'error', status: 'error', message: 'cancelled' } });
            }
          }
          return;
        }
        if (o.status === 'exploded') {
          stored = { value: { kind: 'exploded', status: 'exploded', message: `${o.message}; the command-line tool has no time limit` }, ms: o.ms };
        } else {
          const r = o.response;
          if (r.status !== 'ok') stored = { value: { kind: 'error', status: 'error', message: r.message, error_kind: r.kind }, ms: o.ms };
          else stored = { value: r.cells.find((c) => c.id === id) ?? { kind: 'error', status: 'error', message: 'no result' }, ms: o.ms };
          await this.cache.put(key, stored);
        }
        // every cell currently waiting on this key gets the value
        for (const s of this.states.values()) {
          if (s.key === key) this.show({ ...s }, s, key, stored);
        }
      });
  }

  private show(s: CellState, old: CellState | undefined, key: string, stored: Stored) {
    const v = stored.value;
    const sameValue = old?.valueKey === key;
    s.previous = sameValue ? old!.previous : old?.value ?? s.previous;
    s.value = v;
    s.valueKey = key;
    s.ms = stored.ms;
    s.key = key;
    s.status = v.status === 'ok' ? 'fresh' : v.status === 'exploded' ? 'exploded' : 'failed';
    this.set(s);
  }

  private set(s: CellState) {
    this.states.set(s.id, s);
    this.onChange(s);
  }

  /** The inputs last given to `update`. */
  inputs() {
    return this.current;
  }
}

interface Stored {
  value: CellValue;
  ms: number;
}
