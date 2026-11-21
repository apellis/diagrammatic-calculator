// The deformation solver on a complex cell (`complex.deform`, docs/complexes.md, "Parameters and
// deformations"): pick the entries of the differentials that may be corrected, solve d² = 0 order
// by order in h, and read what came back: the correction at each order, the freedom left, or the
// order and the composite where the equations have no solution. The result is offered as a new
// cell, or, on request, written over the cell; nothing is replaced without asking.
import { createContext, createSignal, For, Show, useContext, type Accessor } from 'solid-js';
import type { ComplexAction, ComplexJ, FailureJ } from './Complexes';

/** One unknown: a whole differential (from position `from`), or one entry of it. */
export interface DeformUnknown { from: number; row?: number; col?: number }
type Matrix = (string | null)[][];
type MapJ = { at: number | number[]; matrix: Matrix }[];
/** What `complex.deform` answered (its `ok` shape). */
export interface DeformResult {
  solved: boolean;
  solved_to_order: number | null;
  complex: ComplexJ & Record<string, unknown>;
  orders: { order: number; correction: MapJ; freedom_dimension: number; freedom: MapJ[] }[];
  failures: (FailureJ & { order: number })[];
}
/** A run of the solver: the complex it was run on (as JSON text), the unknowns, and the answer. */
export interface DeformRun { input: string; unknowns: DeformUnknown[]; result: DeformResult }

/** `truncate=h^N` (and `hdeg=d`) in a context line, or null when it computes exactly in h. */
export function truncationOf(context: string): { order: number; hdeg: number } | null {
  const t = /(?:^|\s)truncate=h\^(\d+)(?=\s|$)/.exec(context);
  if (!t) return null;
  const d = /(?:^|\s)hdeg=(-?\d+)(?=\s|$)/.exec(context);
  return { order: Number(t[1]), hdeg: d ? Number(d[1]) : 0 };
}

/** The workspace's side of the solver: the truncation of its context, the selections and the last runs. */
export interface DeformStore {
  truncation: Accessor<{ order: number; hdeg: number } | null>;
  /** The entries picked in a cell (keyed by its name): `from` → set of `row,col`. */
  picked: (name: string) => Map<number, Set<string>>;
  setPicked: (name: string, p: Map<number, Set<string>>) => void;
  run: (name: string) => DeformRun | undefined;
  open: (name: string) => boolean;
  setOpen: (name: string, on: boolean) => void;
  setRun: (name: string, r: DeformRun | undefined) => void;
}
export function createDeformStore(context: Accessor<string>): DeformStore {
  const [picks, setPicks] = createSignal(new Map<string, Map<number, Set<string>>>());
  const [runs, setRuns] = createSignal(new Map<string, DeformRun>());
  const [opened, setOpened] = createSignal(new Set<string>());
  return {
    truncation: () => truncationOf(context()),
    picked: (name) => picks().get(name) ?? new Map(),
    setPicked: (name, p) => setPicks((m) => new Map(m).set(name, p)),
    run: (name) => runs().get(name),
    open: (name) => opened().has(name),
    setOpen: (name, on) => setOpened((s) => { const n = new Set(s); if (on) n.add(name); else n.delete(name); return n; }),
    setRun: (name, r) => setRuns((m) => { const n = new Map(m); if (r) n.set(name, r); else n.delete(name); return n; }),
  };
}
export const DeformContext = createContext<DeformStore>();
export const useDeform = () => useContext(DeformContext);

/** The picked entries as unknowns: a whole differential when every entry of it is picked. */
export function unknownsOf(picked: Map<number, Set<string>>, size: (from: number) => [number, number]): DeformUnknown[] {
  const out: DeformUnknown[] = [];
  for (const [from, set] of [...picked].sort((a, b) => a[0] - b[0])) {
    if (!set.size) continue;
    const [rows, cols] = size(from);
    if (set.size === rows * cols) { out.push({ from }); continue; }
    for (const k of [...set].sort()) {
      const [row, col] = k.split(',').map(Number);
      out.push({ from, row, col });
    }
  }
  return out;
}

const pos = (p: number | number[]) => (Array.isArray(p) ? `(${p.join(', ')})` : String(p)).replace(/-/g, '−');
const nonzero = (e: string | null) => e != null && e.trim() !== '' && e.trim() !== '0';
/** The nonzero entries of a map, one line each. */
function entriesOf(m: MapJ): { at: string; row: number; col: number; text: string }[] {
  return m.flatMap((c) => c.matrix.flatMap((r, i) => r.flatMap((e, j) => (nonzero(e) ? [{ at: pos(c.at), row: i, col: j, text: e! }] : []))));
}
function MapLines(props: { m: MapJ; testid?: string }) {
  return (
    <Show when={entriesOf(props.m).length} fallback={<span class="hint">zero</span>}>
      <ul class="dx-entries" data-testid={props.testid}>
        <For each={entriesOf(props.m)}>{(e) => <li>d from {e.at}, row {e.row}, column {e.col}: <code>{e.text}</code></li>}</For>
      </ul>
    </Show>
  );
}

/**
 * The solver's panel under a complex cell. `diffs` lists the differentials with their sizes,
 * `complex` is the complex as the engine read it, and `onAction` runs the solver or applies its result.
 */
export function DeformPanel(props: { name: string; complex: ComplexJ; diffs: { from: number; rows: number; cols: number }[]; onAction: (a: ComplexAction) => void; busy: boolean; onClose: () => void }) {
  const store = useDeform();
  const trunc = () => store?.truncation() ?? null;
  const picked = () => store?.picked(props.name) ?? new Map<number, Set<string>>();
  const toggle = (from: number, keys: string[], on: boolean) => {
    const next = new Map(picked());
    const set = new Set(next.get(from) ?? []);
    for (const k of keys) { if (on) set.add(k); else set.delete(k); }
    next.set(from, set);
    store?.setPicked(props.name, next);
  };
  const all = (d: { rows: number; cols: number }) => Array.from({ length: d.rows * d.cols }, (_, n) => `${Math.floor(n / d.cols)},${n % d.cols}`);
  const size = (from: number): [number, number] => { const d = props.diffs.find((x) => x.from === from); return d ? [d.rows, d.cols] : [0, 0]; };
  const unknowns = () => unknownsOf(picked(), size);
  const run = () => store?.run(props.name);
  // a run on an earlier version of the complex is still shown, but labelled, and it cannot be applied
  const current = () => run()?.input === JSON.stringify(props.complex);
  const r = () => run()?.result;
  const failure = () => r()?.failures[0];
  return (
    <div class="dx" data-testid="deform-panel">
      <div class="row"><b>Deform</b><span class="hint">solve d² = 0 order by order in h</span><span style={{ flex: 1 }} />
        <button class="ghost" onClick={() => props.onClose()} aria-label="Close the deformation panel">Close</button></div>
      <Show when={trunc()} fallback={
        <div class="hint" data-testid="deform-needs-truncate">
          The solver works with coefficients in k[h]/(h<sup>N+1</sup>). Add <code>truncate=h^N</code> to the context line
          (and <code>hdeg=d</code> for the degree of h, default 0), for instance <code>truncate=h^2 hdeg=2</code>; then pick
          the entries that may change and solve.
        </div>
      }>
        {(t) => <>
          <div class="hint">Coefficients in k[h]/(h<sup>{t().order + 1}</sup>), h of degree {t().hdeg}. Tick the entries that may be
            corrected; each order m adds h<sup>m</sup>X<sub>m</sub> to them, with X<sub>m</sub> of degree {t().hdeg ? `${t().hdeg < 0 ? '' : '−'}${Math.abs(t().hdeg)}m` : '0'}. The entries as written are kept.</div>
          <Show when={props.diffs.length} fallback={<div class="hint">This complex has no differential to correct.</div>}>
            <div class="dx-pick">
              <For each={props.diffs}>{(d) => (
                <fieldset class="dx-diff" data-testid="deform-diff">
                  <legend>
                    <label><input type="checkbox" checked={(picked().get(d.from)?.size ?? 0) === d.rows * d.cols && d.rows * d.cols > 0}
                      onChange={(e) => toggle(d.from, all(d), e.currentTarget.checked)} data-testid="deform-whole" /> d from {pos(d.from)}</label>
                    <span class="hint"> ({d.rows} × {d.cols})</span>
                  </legend>
                  <table class="dx-grid"><tbody>
                    <For each={Array.from({ length: d.rows }, (_, i) => i)}>{(i) => (
                      <tr><For each={Array.from({ length: d.cols }, (_, j) => j)}>{(j) => (
                        <td><input type="checkbox" checked={picked().get(d.from)?.has(`${i},${j}`) ?? false}
                          onChange={(e) => toggle(d.from, [`${i},${j}`], e.currentTarget.checked)}
                          aria-label={`d from ${pos(d.from)}, row ${i}, column ${j}`} data-testid="deform-entry" /></td>
                      )}</For></tr>
                    )}</For>
                  </tbody></table>
                </fieldset>
              )}</For>
            </div>
          </Show>
          <div class="row">
            <button class="primary" disabled={props.busy || !unknowns().length} onClick={() => props.onAction({ deform: unknowns() })} data-testid="deform-solve"
              title={unknowns().length ? 'Find corrections, one order of h at a time, so that d² = 0' : 'Tick at least one entry first'}>
              Solve to order {t().order}
            </button>
            <Show when={!unknowns().length}><span class="hint">Tick at least one entry.</span></Show>
          </div>
        </>}
      </Show>
      <Show when={r()}>
        {(res) => (
          <div class="dx-result" data-testid="deform-result">
            <Show when={!current()}><div class="hint" data-testid="deform-stale">This result is for an earlier version of the complex; solve again to use it.</div></Show>
            <div class="row" style={{ 'flex-wrap': 'wrap' }}>
              <span class={`badge ${res().solved ? 'good' : 'bad'}`} data-testid="deform-verdict">{res().solved ? 'solved' : failure()?.order === 0 ? 'not a complex at h = 0' : `obstructed at order ${failure()?.order}`}</span>
              <span>{res().solved_to_order == null ? 'The h-free part does not square to zero, so no order can be solved.' : `Solved through order ${res().solved_to_order}.`}</span>
            </div>
            <Show when={failure()}>{(f) => <div class="cx-failures" data-testid="deform-failure"><b>{f().location}</b>: {f().detail}<Show when={f().maps?.length}><span class="hint"> (marked in red above)</span></Show></div>}</Show>
            <ol class="dx-orders">
              <For each={res().orders}>{(o) => (
                <li data-testid="deform-order">
                  <div>Order {o.order}: correction <MapLines m={o.correction} testid="deform-correction" /></div>
                  <div class="hint">{o.freedom_dimension === 0 ? 'No freedom: this correction is the only one.' : `Freedom of dimension ${o.freedom_dimension}: any other correction differs from this one by a combination of these${o.freedom.length < o.freedom_dimension ? ` (the first ${o.freedom.length} shown)` : ''}.`}</div>
                  <Show when={o.freedom.length}>
                    <details><summary>The freedom at order {o.order}</summary>
                      <ol><For each={o.freedom}>{(y) => <li><MapLines m={y} /></li>}</For></ol>
                    </details>
                  </Show>
                </li>
              )}</For>
            </ol>
            <Show when={current() && res().solved_to_order != null && res().orders.some((o) => entriesOf(o.correction).length)}>
              <div class="row">
                <button class="primary" disabled={props.busy} onClick={() => props.onAction({ deformApply: 'new' })} data-testid="deform-add"
                  title="Add the corrected complex as a new cell below; this cell stays as it is">
                  Add the corrected complex as a new cell
                </button>
                <button disabled={props.busy} onClick={() => props.onAction({ deformApply: 'replace' })} data-testid="deform-replace"
                  title="Write the corrected differentials into this cell (the change can be undone)">
                  Write into this cell
                </button>
                <Show when={!res().solved}><span class="hint">(corrected through order {res().solved_to_order})</span></Show>
              </div>
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}
