// Incremental evaluation of a workspace.
//
// Each cell's result is keyed by a hash of its inputs: the engine build, the context, the scalars
// it mentions (unknowns of solve cells), the cell's text, and the keys of the definitions it uses.
// An edit therefore changes the keys of exactly the cells downstream of it; every other cell, and
// any state reached again by undo, is a cache hit. A cell is evaluated by running a small
// workspace holding just the definitions it needs and itself, as a job that is cancelled when
// its inputs change again.
import type { CallOptions, Outcome, Sizes } from '../engine/engine';
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
  | { kind: 'exploded'; status: 'exploded'; message: string; diagnostics?: Sizes };

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
  /** When the job computing the cell's value started (Date.now()), while it runs. */
  since?: number;
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
  /** The workspace each job runs, by key: the cell as the command-line tool would take it. */
  private workspaces = new Map<string, string>();
  /** The scalars each job's workspace runs with, by key. */
  private scalars = new Map<string, string[]>();
  /** Time limits asked for once, by cell id, for the next job of that cell. */
  private limits = new Map<string, number>();
  private states = new Map<string, CellState>();
  /** States before synchronous invalidation, retained until the new keys are known. */
  private invalidated = new Map<string, CellState>();
  private generation = 0;
  private appliedGeneration = 0;
  private remoteTimer?: ReturnType<typeof setTimeout>;
  private remoteWaiters: { resolve: () => void; reject: (error: unknown) => void }[] = [];
  private completed = new Map<string, Stored>();
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

  /** Stops the job computing one cell; the cell shows as failed with 'cancelled'. */
  cancel(id: string) {
    const key = this.states.get(id)?.key;
    if (key) this.jobs.get(key)?.abort();
  }

  /** Computes a cell again, for example after it was cancelled; `timeoutMs` sets the time limit for this run. */
  rerun(id: string, timeoutMs?: number) {
    const s = this.states.get(id);
    if (!s) return;
    if (timeoutMs) this.limits.set(id, timeoutMs);
    if (s.key) this.completed.delete(s.key);
    this.states.set(id, { ...s, key: undefined, valueKey: undefined });
    void this.update(this.current.context, this.current.cells);
  }

  /** The workspace that computes a cell (the definitions it needs, then the cell), as a .dcw file. */
  workspaceOf(id: string): string | undefined {
    const key = this.states.get(id)?.key;
    return key ? this.workspaces.get(key) : undefined;
  }

  snapshotRun(id: string): OpTypes['workspace.run']['request'] | undefined {
    const key = this.states.get(id)?.key;
    const workspace = key && this.workspaces.get(key);
    return workspace ? { workspace, scalars: [...(this.scalars.get(key!) ?? [])], summands: this.opts.summands ?? 200 } : undefined;
  }

  /**
   * Which terms of the definitions a watch uses produced one summand of its value
   * (`workspace.trace`), computed as a job like the value itself.
   */
  trace(id: string, word: string, signal?: AbortSignal): Promise<Outcome<OpTypes['workspace.trace']['response']>> {
    const key = this.states.get(id)?.key;
    const workspace = key && this.workspaces.get(key);
    if (!workspace) return Promise.resolve({ status: 'done', ms: 0, response: { status: 'error', kind: 'invalid', message: 'the cell has no value to trace yet' } });
    const timeoutMs = this.opts.timeoutMs ?? 120_000;
    return this.engine.call('workspace.trace', { workspace, cell: id, word, scalars: this.scalars.get(key!) ?? [] }, { signal, timeoutMs });
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
  update(context: string, cells: CellData[], mode: 'local' | 'remote' = 'local'): Promise<void> {
    const gen = ++this.generation;
    const changed = context !== this.current.context || cells.length !== this.current.cells.length ||
      cells.some((c, i) => c.id !== this.current.cells[i]?.id || c.text !== this.current.cells[i]?.text);
    // Own the snapshot: a caller may mutate its array during the asynchronous parse.
    this.current = { context, cells: cells.map(c => ({ ...c })) };
    if (changed) {
      // Dependency discovery and cache reads are asynchronous. Until they finish, no old
      // mathematical result may claim to be current, including in collapsed cells.
      for (const s of this.states.values()) {
        if (s.status === 'static') continue;
        if (!this.invalidated.has(s.id)) this.invalidated.set(s.id, s);
        this.set({ ...s, status: s.value ? 'stale' : 'running', parseError: undefined });
      }
    }
    if (mode === 'remote') {
      const waiting = new Promise<void>((resolve, reject) => this.remoteWaiters.push({ resolve, reject }));
      // Fixed small window, not a trailing debounce: continuous remote traffic cannot starve it.
      this.remoteTimer ??= setTimeout(() => {
        this.remoteTimer = undefined;
        this.flushUpdate(this.generation);
      }, 75);
      return waiting;
    }
    if (this.remoteTimer) clearTimeout(this.remoteTimer);
    this.remoteTimer = undefined;
    return this.flushUpdate(gen);
  }

  private flushUpdate(gen: number): Promise<void> {
    const waiters = this.remoteWaiters.splice(0);
    const { context, cells } = this.current;
    const applying = this.apply(context, cells, gen);
    applying.then(() => waiters.forEach(w => w.resolve()), error => waiters.forEach(w => w.reject(error)));
    return applying;
  }

  private async apply(context: string, cells: CellData[], gen: number): Promise<void> {
    const parsed = await Promise.all(cells.map((c) => this.parse(c.text)));
    if (gen !== this.generation) return;

    // the unknowns of solve cells are scalars of the workspace; each cell gets the ones it or the
    // definitions it uses mention, so a solve elsewhere never changes its value or its key
    const unknowns = new Set(parsed.flatMap((p) => ('cell' in p ? p.cell.unknowns ?? [] : [])));
    const keys: (string | undefined)[] = [];
    const deps: number[][] = [];
    const scalars: string[][] = [];
    const defined = new Map<string, number>(); // name → index of the latest definition so far
    const blocked: (string | undefined)[] = [];
    let invalid: string | undefined;
    cells.forEach((c, i) => {
      const p = parsed[i];
      deps[i] = [];
      scalars[i] = [];
      if (!('cell' in p)) { invalid ??= c.id; return; }
      if (p.cell.kind === 'note' || p.cell.kind === 'comment' || p.cell.kind === 'blank') return;
      // An unparseable cell may introduce/shadow a name. Omitting it from a mini-workspace
      // could silently use an older binding. Fail closed until that syntax is repaired.
      if (invalid) { blocked[i] = invalid; return; }
      const used = p.cell.uses.filter((n) => defined.has(n)).map((n) => defined.get(n)!);
      deps[i] = used;
      scalars[i] = [...new Set([...p.cell.uses.filter((n) => unknowns.has(n)), ...(p.cell.unknowns ?? []), ...used.flatMap((j) => scalars[j])])].sort();
      keys[i] = hash(JSON.stringify([this.opts.engineId, context, scalars[i], c.text.trim(), used.map((j) => keys[j] ?? 'unknown')]));
      if (p.cell.defines) defined.set(p.cell.defines, i);
    });

    const lookup = [...new Set(keys.filter((key, i): key is string =>
      !!key && this.states.get(cells[i].id)?.key !== key && !this.jobs.has(key)))];
    const hits = lookup.length
      ? this.cache.getMany ? await this.cache.getMany(lookup)
        : new Map(await Promise.all(lookup.map(async key => [key, await this.cache.get(key)] as const)))
      : new Map<string, unknown>();
    if (gen !== this.generation) return;
    this.appliedGeneration = gen;
    const wanted = new Set(keys.filter((key): key is string => !!key));
    for (let i = 0; i < cells.length; i++) {
      const c = cells[i];
      const p = parsed[i];
      const old = this.invalidated.get(c.id) ?? this.states.get(c.id);
      const s: CellState = { id: c.id, text: c.text, status: 'static', value: old?.value, valueKey: old?.valueKey, previous: old?.previous, ms: old?.ms };
      if ('error' in p) {
        s.parseError = p.error;
        s.status = 'failed';
        s.value = s.valueKey = undefined;
      } else {
        s.parsed = p.cell;
        if (blocked[i]) {
          s.status = 'failed';
          s.value = { kind: 'error', status: 'error', message: `Cannot compute: cell ${blocked[i]} does not parse.` };
          s.valueKey = undefined;
          this.set(s);
          continue;
        }
        s.key = keys[i];
        if (s.key) {
          const hit = this.completed.get(s.key) ?? hits.get(s.key)
            ?? (old?.valueKey === s.key && old.value ? { value: old.value, ms: old.ms ?? 0 } : undefined);
          if (hit !== undefined) {
            // kept for tracing a summand and for the command line, as for a computed value
            this.workspaces.set(s.key, this.miniWorkspace(context, cells, deps, i));
            this.scalars.set(s.key, scalars[i]);
            this.show(s, old, s.key, hit as Stored);
          } else {
            s.status = old?.value && old.valueKey !== s.key ? 'stale' : 'running';
            if (old?.key === s.key && old.status !== 'stale' && old.status !== 'running') {
              s.status = old.status; // failed or exploded with these very inputs
            } else {
              // the time computing counts from when this key's job started
              s.since = old?.key === s.key && old.since ? old.since : Date.now();
              this.start(s.key, this.miniWorkspace(context, cells, deps, i), cells[i].id, scalars[i]);
            }
          }
        }
      }
      this.set(s);
    }
    this.invalidated.clear();
    for (const id of [...this.states.keys()]) if (!cells.some((c) => c.id === id)) this.states.delete(id);
    // jobs no longer needed by any cell are cancelled
    for (const [key, a] of this.jobs) if (!wanted.has(key)) { a.abort(); this.jobs.delete(key); }
    const live = new Set([...this.states.values()].map((s) => s.key));
    for (const key of this.workspaces.keys()) if (!live.has(key)) this.workspaces.delete(key);
    for (const key of this.scalars.keys()) if (!live.has(key)) this.scalars.delete(key);
    for (const key of this.completed.keys()) if (!live.has(key)) this.completed.delete(key);
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
    this.workspaces.set(key, workspace);
    this.scalars.set(key, scalars);
    if (this.jobs.has(key)) return;
    const abort = new AbortController();
    this.jobs.set(key, abort);
    const timeoutMs = this.limits.get(id) ?? this.opts.timeoutMs ?? 120_000;
    this.limits.delete(id);
    this.engine
      .call('workspace.run', { workspace, summands: this.opts.summands ?? 200, scalars }, { signal: abort.signal, timeoutMs })
      .then(async (o) => {
        // Key equality alone is insufficient after edit-away/edit-back: bind the result to
        // this exact invocation, and never trust an engine that resolves after abort.
        if (this.jobs.get(key) !== abort) return;
        this.jobs.delete(key);
        if (abort.signal.aborted) o = { status: 'cancelled', ms: o.ms };
        let stored: Stored;
        if (o.status === 'cancelled') {
          // cells still waiting on this key were stopped by the person, not by an edit
          for (const s of this.states.values()) {
            if (s.key === key && (s.status === 'running' || s.status === 'stale')) {
              this.set({ ...s, status: 'failed', since: undefined, previous: s.value, value: { kind: 'error', status: 'error', message: 'cancelled' } });
            }
          }
          return;
        }
        if (o.status === 'exploded') {
          stored = {
            value: { kind: 'exploded', status: 'exploded', message: `${o.message[0].toUpperCase()}${o.message.slice(1)}.`, ...(o.diagnostics ? { diagnostics: o.diagnostics } : {}) },
            ms: o.ms,
          };
        } else {
          const r = o.response;
          if (r.status !== 'ok') stored = { value: { kind: 'error', status: 'error', message: r.message, error_kind: r.kind }, ms: o.ms };
          else stored = { value: r.cells.find((c) => c.id === id) ?? { kind: 'error', status: 'error', message: 'no result' }, ms: o.ms };
          // Persistence is not on the local result-display critical path.
          void this.cache.put(key, stored).catch(() => {});
        }
        this.completed.set(key, stored);
        // A newer input snapshot may still be parsing/coalescing. Its apply pass will
        // consume this result only if its causal key still matches.
        if (this.appliedGeneration !== this.generation) return;
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
    s.value = 'id' in v ? { ...v, id: s.id } : v;
    s.valueKey = key;
    s.ms = stored.ms;
    s.key = key;
    s.status = v.status === 'ok' ? 'fresh' : v.status === 'exploded' ? 'exploded' : 'failed';
    s.since = undefined;
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
