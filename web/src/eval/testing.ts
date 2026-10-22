// The engine for unit tests: the WebAssembly build run directly in Node, without workers.
import fs from 'node:fs';
import { initSync, call } from '../engine/pkg/diagcalc_wasm.js';
import type { Calls } from './evaluator';
import type { CacheStore } from './cache';

let ready = false;

export function nodeEngine(): Calls & { calls: string[] } {
  if (!ready) {
    initSync({ module: fs.readFileSync(new URL('../engine/pkg/diagcalc_wasm_bg.wasm', import.meta.url)) });
    ready = true;
  }
  const calls: string[] = [];
  return {
    calls,
    async call(op, request, opts) {
      if (opts?.signal?.aborted) return { status: 'cancelled', ms: 0 };
      calls.push(op);
      await new Promise((r) => setTimeout(r, 0));
      const t = performance.now();
      const response = JSON.parse(call(op, JSON.stringify(request)));
      return { status: 'done', response, ms: performance.now() - t };
    },
  };
}

export function memoryCache(): CacheStore & { map: Map<string, unknown> } {
  const map = new Map<string, unknown>();
  return {
    map,
    async get(k) { return map.get(k); },
    async put(k, v) { map.set(k, v); },
    async clear() { map.clear(); },
  };
}
