import { describe, expect, it } from 'vitest';
import { enable_progress } from './pkg/diagcalc_wasm.js';
import { nodeEngine } from '../eval/testing';
import type { Sizes } from './engine';

describe('progress reports', () => {
  it('reach the page while a workspace runs, with the largest sizes reached', async () => {
    const engine = nodeEngine();
    const seen: Sizes[] = [];
    (globalThis as { diagcalcProgress?: (s: string) => void }).diagcalcProgress = (s) => seen.push(JSON.parse(s));
    enable_progress();
    const workspace = 'diagcalc-workspace/1\ncontext nilhecke nu=3 ring=QQ\ndef a := x_1 + x_2 + psi_1 + psi_2  # id=d1\nwatch a a a  # id=w2\n';
    const o = await engine.call('workspace.run', { workspace, summands: 0, scalars: [] });
    expect(o.status).toBe('done');
    const last = seen.at(-1)!;
    expect(last.cell).toBe('w2');
    expect(last.stage).toBe('normal form');
    expect(last.terms).toBeGreaterThan(1);
    expect(last.layers).toBeGreaterThanOrEqual(3);
    // each cell starts again from zero
    expect(seen.some((s) => s.cell === 'd1' && s.stage === 'elaborating')).toBe(true);
  });
});
