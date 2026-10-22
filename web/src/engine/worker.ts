// One engine instance in a Web Worker. The main thread compiles the WebAssembly module once and
// sends it here, so starting a worker (for example after cancelling a job) is cheap.
import init, { call, version } from './pkg/diagcalc_wasm.js';

export type ToWorker =
  | { type: 'init'; module: WebAssembly.Module }
  | { type: 'call'; id: number; op: string; request: string };

export type FromWorker =
  | { type: 'ready'; version: string }
  | { type: 'result'; id: number; response: string; ms: number }
  | { type: 'failed'; id: number; message: string };

const post = (m: FromWorker) => (self as unknown as Worker).postMessage(m);

self.onmessage = async (e: MessageEvent<ToWorker>) => {
  const m = e.data;
  if (m.type === 'init') {
    await init({ module_or_path: m.module });
    post({ type: 'ready', version: version() });
    return;
  }
  const t = performance.now();
  try {
    const response = call(m.op, m.request);
    post({ type: 'result', id: m.id, response, ms: performance.now() - t });
  } catch (err) {
    // a panic in the core: the instance is unusable, so the client replaces this worker
    post({ type: 'failed', id: m.id, message: String(err) });
  }
};
