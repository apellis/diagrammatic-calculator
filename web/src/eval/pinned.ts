import type { WorkspaceRunRequest, WorkspaceRunCell } from '../engine/ops.gen';
import type { Calls } from './evaluator';

/** Compare the result and its provenance; diagrams and the receiving cell's ID are not values. */
export function comparePinned(a: unknown, b: unknown): boolean {
  const canonical = (value: unknown, root = false): unknown => {
    if (Array.isArray(value)) return value.map(x => canonical(x));
    if (!value || typeof value !== 'object') return value;
    return Object.fromEntries(Object.entries(value).filter(([key]) => key !== 'svg' && !(root && ['id', 'summands'].includes(key)))
      .sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([key, v]) => [key, canonical(v)]));
  };
  return JSON.stringify(canonical(a, true)) === JSON.stringify(canonical(b, true));
}

/** Run the frozen inputs directly: never read a collaborator's result cache or mutate the document. */
export async function recomputePinned(engine: Calls, request: WorkspaceRunRequest, signal?: AbortSignal): Promise<WorkspaceRunCell> {
  const result = await engine.call('workspace.run', request, { signal, timeoutMs: 120_000 });
  if (result.status !== 'done') throw new Error(`Recomputation ${result.status}`);
  if (result.response.status !== 'ok') throw new Error(result.response.message);
  const value = result.response.cells.at(-1);
  if (!value || value.status !== 'ok') throw new Error(value?.message ?? 'No computed result');
  return value;
}
