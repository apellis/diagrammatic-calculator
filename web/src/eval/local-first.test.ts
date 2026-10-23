import { afterEach, expect, it, vi } from 'vitest';
import { IDBFactory, IDBDatabase } from 'fake-indexeddb';
import { ResultCache, type CacheStore } from './cache';
import { Evaluator, type Calls } from './evaluator';

// Deterministic scheduler harness, not a substitute for mathematical WASM tests.
function harness() {
  const runs: { signal?: AbortSignal; finish: () => void; workspace: string }[] = [];
  const engine = { call: vi.fn(async (op: string, request: { cell: string; workspace: string }, opts?: { signal?: AbortSignal }) => {
    if (op === 'workspace.parse') {
      const defines = /^def (\w+)/.exec(request.cell)?.[1];
      return { status: 'done', ms: 0, response: { status: 'ok', cell: { kind: defines ? 'def' : 'watch', defines, uses: [...request.cell.matchAll(/\ba\d+\b/g)].map(m => m[0]).filter(n => n !== defines) } } };
    }
    return new Promise(resolve => runs.push({ workspace: request.workspace, signal: opts?.signal, finish: () => resolve({ status: 'done', ms: 1, response: { status: 'ok', cells: [...request.workspace.matchAll(/# id=(\w+)/g)].map(m => ({ id: m[1], kind: 'watch', status: 'ok', normal_form: request.workspace })) } }) }));
  }) } as unknown as Calls;
  const map = new Map<string, unknown>();
  const cache = { get: vi.fn(async (key: string) => map.get(key)), getMany: vi.fn(async (keys: string[]) => new Map(keys.map(k => [k, map.get(k)]))), put: vi.fn(async (k: string, v: unknown) => { map.set(k, v); }), clear: async () => { map.clear(); } };
  const ev = new Evaluator(engine, cache as CacheStore, { engineId: 'synthetic' }, () => {});
  return { ev, cache, runs };
}
const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

it('batches cold cache reads into one IndexedDB transaction', async () => {
  vi.stubGlobal('indexedDB', new IDBFactory());
  const cache = new ResultCache(100000, 0);
  await cache.put('a', { n: 1 }); await cache.put('b', { n: 2 });
  const spy = vi.spyOn(IDBDatabase.prototype, 'transaction');
  const hits = await cache.getMany(['a', 'b', 'missing']);
  expect(hits.get('a')).toEqual({ n: 1 }); expect(hits.get('b')).toEqual({ n: 2 });
  expect(hits.get('missing')).toBeUndefined(); expect(spy).toHaveBeenCalledTimes(1);
});

it('batches changed keys and keeps a running job needed by another cell', async () => {
  const { ev, cache, runs } = harness();
  await ev.update('ctx', [{ id: 'a', text: 'watch 1' }, { id: 'b', text: 'watch 1' }]);
  expect(cache.getMany).toHaveBeenCalledTimes(1); expect(cache.get).not.toHaveBeenCalled();
  await ev.update('ctx', [{ id: 'a', text: 'watch 2' }, { id: 'b', text: 'watch 1' }]);
  expect(runs).toHaveLength(2); expect(runs[0].signal?.aborted).toBe(false);
  expect(cache.getMany.mock.calls[1][0]).toHaveLength(1);
  runs.forEach(r => r.finish()); await flush();
  expect(ev.state('b')?.status).toBe('fresh');
});

it('coalesces remote refreshes but never holds a local update', async () => {
  vi.useFakeTimers();
  const { ev, runs } = harness();
  const first = ev.update('ctx', [{ id: 'a', text: 'watch 1' }], 'remote');
  const second = ev.update('ctx', [{ id: 'a', text: 'watch 2' }], 'remote');
  await flush(); expect(runs).toHaveLength(0);
  await ev.update('ctx', [{ id: 'a', text: 'watch 3' }]);
  await Promise.all([first, second]);
  expect(runs).toHaveLength(1); expect(runs[0].workspace).toContain('watch 3');
  await vi.advanceTimersByTimeAsync(1000); expect(runs).toHaveLength(1);
});

it('ignores late cancelled results, including returning to the same key', async () => {
  const { ev, runs } = harness();
  await ev.update('ctx', [{ id: 'a', text: 'watch 1' }]);
  await ev.update('ctx', [{ id: 'a', text: 'watch 2' }]);
  await ev.update('ctx', [{ id: 'a', text: 'watch 1' }]);
  runs[0].finish(); await flush(); expect(ev.state('a')?.status).toBe('running');
  runs[2].finish(); await flush(); expect(ev.state('a')?.status).toBe('fresh');
});

it('publishes local results without waiting for durable cache writes', async () => {
  const { ev, cache, runs } = harness();
  cache.put.mockImplementation(() => new Promise(() => {}));
  await ev.update('ctx', [{ id: 'a', text: 'watch 1' }]);
  runs[0].finish(); await flush();
  expect(ev.state('a')?.status).toBe('fresh');
});

it('bounds remote coalescing even under continuous updates', async () => {
  vi.useFakeTimers();
  const { ev, runs } = harness();
  const waiting = ev.update('ctx', [{ id: 'a', text: 'watch 1' }], 'remote');
  await vi.advanceTimersByTimeAsync(50);
  const later = ev.update('ctx', [{ id: 'a', text: 'watch 2' }], 'remote');
  await vi.advanceTimersByTimeAsync(25); await Promise.all([waiting, later]);
  expect(runs).toHaveLength(1); expect(runs[0].workspace).toContain('watch 2');
});

it('does not publish an old result during a pending newer remote snapshot', async () => {
  vi.useFakeTimers();
  const { ev, runs } = harness();
  await ev.update('ctx', [{ id: 'a', text: 'watch 1' }]);
  const pending = ev.update('ctx', [{ id: 'a', text: 'watch 2' }], 'remote');
  runs[0].finish(); await flush();
  expect(ev.state('a')?.value).toBeUndefined();
  await vi.advanceTimersByTimeAsync(75); await pending;
  expect(ev.state('a')?.text).toBe('watch 2');
  expect(ev.state('a')?.status).toBe('running');
});

it.each([25, 50])('synthetic 50-cell edit: recomputes only the %i-cell dependent chain', async (dependent) => {
  const { ev, runs } = harness();
  const cells = Array.from({ length: 50 }, (_, i) => ({ id: `c${i}`, text: i < dependent ? `def a${i} := ${i ? `a${i - 1}` : '1'}` : `watch ${i}` }));
  await ev.update('ctx', cells); runs.forEach(r => r.finish()); await flush();
  const before = runs.length;
  const started = performance.now();
  await ev.update('ctx', [{ ...cells[0], text: 'def a0 := 2' }, ...cells.slice(1)]);
  runs.slice(before).forEach(r => r.finish()); await flush();
  expect(runs.length - before).toBe(dependent);
  expect(cells.every(c => ev.state(c.id)?.status === 'fresh')).toBe(true);
  console.info(`Synthetic 50-cell/${dependent}-dependent edit-to-fresh: ${(performance.now() - started).toFixed(2)}ms; recomputed ${runs.length - before}`);
});
