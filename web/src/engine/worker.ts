// One engine instance in a Web Worker. The main thread compiles the WebAssembly module once and
// sends it here, so starting a worker (for example after cancelling a job) is cheap.
import init, { call, enable_progress, version } from './pkg/diagcalc_wasm.js';

export type ToWorker =
  | { type: 'init'; module: WebAssembly.Module }
  | { type: 'call'; id: number; op: string; request: string };

export type FromWorker =
  | { type: 'ready'; version: string }
  | { type: 'result'; id: number; response: string; ms: number }
  /** The sizes the running call has reached (core `progress::Sizes`), as JSON. */
  | { type: 'progress'; id: number; sizes: string }
  | { type: 'failed'; id: number; message: string };

const post = (m: FromWorker) => (self as unknown as Worker).postMessage(m);

/**
 * Progress reports from the core, passed on to the main thread: when the stage changes, and
 * otherwise at most every 150 ms, so the last one received is never far behind when a job is
 * stopped at its time limit.
 */
let running = 0;
let lastPost = 0;
let lastStage = '';
(globalThis as unknown as { diagcalcProgress: (s: string) => void }).diagcalcProgress = (sizes: string) => {
  const now = performance.now();
  const stage = /"stage":"([^"]*)"/.exec(sizes)?.[1] ?? '';
  if (stage === lastStage && now - lastPost < 150) return;
  lastStage = stage;
  lastPost = now;
  post({ type: 'progress', id: running, sizes });
};

self.onmessage = async (e: MessageEvent<ToWorker>) => {
  const m = e.data;
  if (m.type === 'init') {
    await init({ module_or_path: m.module });
    enable_progress();
    post({ type: 'ready', version: version() });
    return;
  }
  const t = performance.now();
  running = m.id;
  lastStage = '';
  try {
    const response = call(m.op, m.request);
    post({ type: 'result', id: m.id, response, ms: performance.now() - t });
  } catch (err) {
    // a panic in the core: the instance is unusable, so the client replaces this worker
    post({ type: 'failed', id: m.id, message: String(err) });
  }
};
