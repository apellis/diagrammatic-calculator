import { afterEach, expect, it, vi } from 'vitest';
vi.mock('../engine/pkg/diagcalc_wasm_bg.wasm?url', () => ({ default: 'mock.wasm' }));
import { Engine } from '../engine/engine';

class FakeWorker {
  static all: FakeWorker[] = [];
  onmessage?: (e: { data: unknown }) => void;
  onerror?: unknown;
  terminate = vi.fn();
  requests: { id: number }[] = [];
  constructor() { FakeWorker.all.push(this); }
  postMessage(m: { type: string; id: number }) {
    if (m.type === 'init') this.send({ type: 'ready', version: 'test' });
    else this.requests.push(m);
  }
  send(data: unknown) { this.onmessage?.({ data }); }
}
const flush = async () => { for (let i = 0; i < 10; i++) await Promise.resolve(); };
function setup() {
  vi.useFakeTimers(); FakeWorker.all = [];
  vi.stubGlobal('Worker', FakeWorker);
  vi.stubGlobal('fetch', vi.fn(async () => ({})));
  vi.spyOn(WebAssembly, 'compileStreaming').mockResolvedValue({} as WebAssembly.Module);
  return new Engine(1);
}
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

it('allows a cancelled synchronous worker a grace period, ignoring its result and reusing it', async () => {
  const engine = setup(); const abort = new AbortController();
  const result = engine.call('workspace.parse', { cell: 'watch 1' }, { signal: abort.signal });
  await flush(); const worker = FakeWorker.all[1]; const id = worker.requests[0].id;
  abort.abort(); await flush(); expect(worker.terminate).not.toHaveBeenCalled();
  worker.send({ type: 'result', id, response: JSON.stringify({ status: 'ok' }), ms: 3 });
  expect((await result).status).toBe('cancelled');
  await vi.advanceTimersByTimeAsync(2000); expect(worker.terminate).not.toHaveBeenCalled();
});

it('does not dispatch a call cancelled before the worker becomes ready', async () => {
  const engine = setup(); const abort = new AbortController();
  const result = engine.call('workspace.parse', { cell: 'watch 1' }, { signal: abort.signal });
  abort.abort(); await flush();
  expect((await result).status).toBe('cancelled');
  expect(FakeWorker.all[1].requests).toHaveLength(0);
  expect(FakeWorker.all[1].terminate).not.toHaveBeenCalled();
});

it('terminates after grace and fences late messages from replaced workers', async () => {
  const engine = setup(); const abort = new AbortController();
  const old = engine.call('workspace.parse', { cell: 'watch 1' }, { signal: abort.signal });
  await flush(); const worker = FakeWorker.all[1]; const id = worker.requests[0].id;
  abort.abort();
  let done = false;
  const next = engine.call('workspace.parse', { cell: 'watch 2' }).then(o => { done = true; return o; });
  await vi.advanceTimersByTimeAsync(1999); expect(worker.terminate).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1); expect(worker.terminate).toHaveBeenCalledOnce();
  expect((await old).status).toBe('cancelled');
  await flush(); worker.send({ type: 'result', id, response: '{}', ms: 1 }); await flush(); expect(done).toBe(false);
  const replacement = FakeWorker.all[2];
  replacement.send({ type: 'result', id: replacement.requests[0].id, response: '{"status":"ok"}', ms: 1 });
  expect((await next).status).toBe('done');
});
