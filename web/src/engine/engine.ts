// The engine as the app sees it: typed operation calls answered by WebAssembly workers.
//
// Two lanes. The interactive lane is one worker for quick operations (parsing a cell, describing
// a context); its calls run in order, each stopped after INTERACTIVE_TIMEOUT_MS (a shared or
// imported document must not be able to freeze it). The job lane is a pool of workers for
// evaluations that can take long; a job can be cancelled, or stopped at its time limit. Stopping
// a call replaces its worker after a short cancellation grace period.
import wasmUrl from './pkg/diagcalc_wasm_bg.wasm?url';
import type { OpName, OpTypes } from './ops.gen';
import type { FromWorker, ToWorker } from './worker';

/** What a call returns besides the operation's response. */
export type Outcome<R> =
  | { status: 'done'; response: R; ms: number }
  | { status: 'cancelled'; ms: number }
  | { status: 'exploded'; ms: number; message: string; diagnostics?: Sizes };

/** The sizes a computation reached (core `progress::Sizes`): a stopped job's size diagnostics. */
export interface Sizes {
  stage: string;
  cell?: string;
  terms: number;
  layers: number;
  dots: number;
  basis?: number;
  polynomial?: number;
  steps?: number;
}

/** What a job stopped at its time limit says: "stopped at the 120-second limit". */
export function limitMessage(ms: number): string {
  const s = Math.round(ms / 1000);
  return s % 60 === 0 && s >= 120 ? `stopped at the ${s / 60}-minute limit` : `stopped at the ${s}-second limit`;
}

/** The time limit of every interactive call, unless the caller gives another. */
export const INTERACTIVE_TIMEOUT_MS = 10_000;
export const CANCELLATION_GRACE_MS = 2_000;

export interface CallOptions {
  /** 'interactive' for quick operations, 'job' (the default) for evaluations. */
  lane?: 'interactive' | 'job';
  signal?: AbortSignal;
  /** Stop the call after this many milliseconds (interactive calls: INTERACTIVE_TIMEOUT_MS by default). */
  timeoutMs?: number;
}

interface Pending {
  id: number;
  op: string;
  request: string;
  started: number;
  resolve: (o: Outcome<unknown>) => void;
  signal?: AbortSignal;
  timeoutMs?: number;
  /** The last sizes the call reported. */
  sizes?: Sizes;
  cancelled?: boolean;
  removeAbort?: () => void;
}

class Slot {
  worker!: Worker;
  ready!: Promise<string>;
  current: Pending | null = null;
  timer: ReturnType<typeof setTimeout> | null = null;
  cancelTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private module: Promise<WebAssembly.Module>,
    private onIdle: () => void,
  ) {
    this.start();
  }

  start() {
    const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    this.worker = worker;
    this.ready = new Promise((resolve, reject) => {
      this.worker.onmessage = (e: MessageEvent<FromWorker>) => {
        if (this.worker !== worker) return;
        const m = e.data;
        if (m.type === 'ready') return resolve(m.version);
        if (!this.current || this.current.id !== m.id) return;
        if (m.type === 'progress') {
          if (this.current?.id === m.id) {
            try {
              this.current.sizes = JSON.parse(m.sizes) as Sizes;
            } catch {
              // a malformed report is ignored
            }
          }
          return;
        }
        if (m.type === 'result') this.finish({ status: 'done', response: JSON.parse(m.response), ms: m.ms });
        else if (m.type === 'failed') {
          this.stop({
            status: 'done',
            ms: performance.now() - (this.current?.started ?? 0),
            response: { status: 'error', kind: 'internal', message: m.message },
          });
        }
      };
      this.worker.onerror = (e) => reject(e);
    });
    this.module.then((module) => {
      if (this.worker === worker) worker.postMessage({ type: 'init', module } satisfies ToWorker);
    });
  }

  restart() {
    this.worker.terminate();
    this.start();
  }

  run(p: Pending) {
    this.current = p;
    p.started = performance.now();
    if (p.timeoutMs) {
      this.timer = setTimeout(() => {
        this.stop({ status: 'exploded', ms: performance.now() - p.started, message: limitMessage(p.timeoutMs!), ...(p.sizes ? { diagnostics: p.sizes } : {}) });
      }, p.timeoutMs);
    }
    const abort = () => {
      if (this.current !== p) return;
      p.cancelled = true;
      if (this.timer) clearTimeout(this.timer);
      this.timer = null;
      // The actual WASM call is synchronous and has no cancellation flag. Posting a
      // cancel message cannot interrupt it. Let it finish during grace (discarding
      // the result), otherwise replace the worker. Do not claim cooperative support.
      this.cancelTimer = setTimeout(() => {
        if (this.current === p) this.stop({ status: 'cancelled', ms: performance.now() - p.started });
      }, CANCELLATION_GRACE_MS);
    };
    p.signal?.addEventListener('abort', abort, { once: true });
    p.removeAbort = () => p.signal?.removeEventListener('abort', abort);
    this.ready.then(() => {
      if (this.current !== p) return;
      if (p.cancelled) this.finish({ status: 'cancelled', ms: performance.now() - p.started });
      else this.worker.postMessage({ type: 'call', id: p.id, op: p.op, request: p.request } satisfies ToWorker);
    });
  }

  /** Ends the running call without its result, replacing the worker. */
  stop(o: Outcome<unknown>) {
    // Replace before pumping the next call; otherwise it can be sent to the dead worker.
    this.restart();
    this.finish(o);
  }

  private finish(o: Outcome<unknown>) {
    const p = this.current;
    if (!p) return;
    if (this.timer) clearTimeout(this.timer);
    if (this.cancelTimer) clearTimeout(this.cancelTimer);
    this.timer = this.cancelTimer = null;
    p.removeAbort?.();
    this.current = null;
    p.resolve(p.cancelled ? { status: 'cancelled', ms: performance.now() - p.started } : o);
    this.onIdle();
  }
}

export class Engine {
  private module: Promise<WebAssembly.Module>;
  private interactive: Slot;
  private pool: Slot[];
  private queues: { interactive: Pending[]; job: Pending[] } = { interactive: [], job: [] };
  private nextId = 1;
  /** Number of calls waiting or running, for the status light. */
  onLoad: (n: number) => void = () => {};
  readonly version: Promise<string>;

  constructor(poolSize = Math.max(1, Math.min(4, (navigator.hardwareConcurrency || 2) - 1))) {
    this.module = WebAssembly.compileStreaming(fetch(wasmUrl));
    this.interactive = new Slot(this.module, () => this.pump());
    this.pool = Array.from({ length: poolSize }, () => new Slot(this.module, () => this.pump()));
    this.version = this.interactive.ready;
  }

  call<O extends OpName>(op: O, request: OpTypes[O]['request'], opts: CallOptions = {}): Promise<Outcome<OpTypes[O]['response']>> {
    const lane = opts.lane ?? 'job';
    return new Promise((resolve) => {
      if (opts.signal?.aborted) return resolve({ status: 'cancelled', ms: 0 });
      const p: Pending = {
        id: this.nextId++,
        op,
        request: JSON.stringify(request),
        started: 0,
        resolve: resolve as (o: Outcome<unknown>) => void,
        signal: opts.signal,
        timeoutMs: lane === 'job' ? opts.timeoutMs : (opts.timeoutMs ?? INTERACTIVE_TIMEOUT_MS),
      };
      // a call cancelled while it waits leaves the queue at once
      opts.signal?.addEventListener('abort', () => {
        const q = this.queues[lane];
        const i = q.indexOf(p);
        if (i >= 0) {
          q.splice(i, 1);
          resolve({ status: 'cancelled', ms: 0 });
          this.report();
        }
      });
      this.queues[lane].push(p);
      this.pump();
    });
  }

  /** How many calls are waiting or running. */
  load(): number {
    const running = [this.interactive, ...this.pool].filter((s) => s.current).length;
    return running + this.queues.interactive.length + this.queues.job.length;
  }

  private pump() {
    if (!this.interactive.current && this.queues.interactive.length) this.interactive.run(this.queues.interactive.shift()!);
    for (const s of this.pool) {
      if (!s.current && this.queues.job.length) s.run(this.queues.job.shift()!);
    }
    this.report();
  }

  private report() {
    queueMicrotask(() => this.onLoad(this.load()));
  }
}
