// Complex and module cells: the terms and maps of a complex drawn left to right, what a
// resolution still lacks, and the tools that take the next step. The cell's text is JSON, which
// these views read so that nobody has to (docs/complexes.md).
import { createSignal, For, Show } from 'solid-js';
import type { Summand } from '../engine/ops.gen';
import { Typeset } from './Typeset';

export interface SummandJ { object: string; shift?: number }
interface TermJ { at: number | number[]; summands: SummandJ[] }
/** A matrix entry: its text, or null (or `0`) for zero. */
type EntryText = string | null;
const isZero = (e: EntryText) => e == null || e.trim() === '0';
interface DiffJ { from: number | number[]; axis?: number; matrix: EntryText[][] }
interface ModuleJ { generators?: SummandJ[]; relations?: SummandJ[]; matrix?: EntryText[][]; head?: unknown }
export interface ComplexJ {
  terms?: TermJ[];
  differentials?: DiffJ[];
  dimension?: number;
  indexing?: 'cohomological' | 'homological';
  resolves?: { module: string | ModuleJ; augmentation?: EntryText[][] };
}
interface Dims { test: string; dimensions: string; powers?: [number, number][] }
interface FailureJ { location: string; detail: string }
interface HomologyJ { positions?: { at: number | number[]; zero: boolean; pieces?: { test: string; degree: number; dimension: number }[]; window?: { truncated: boolean } }[]; exact?: boolean }
interface ResolutionJ {
  holds?: boolean;
  failures?: FailureJ[];
  failure_count?: number;
  resolved?: boolean;
  complete?: boolean;
  length?: number;
  next_at?: number | null;
  not_exact_at?: number[];
  uncovered?: Dims[];
  uncovered_dimension?: number;
  module?: Dims[];
  module_dimension?: number;
  minimal?: boolean | null;
}
/** A strand of an object, left to right: a colour at a position, or a red strand with its weight. */
export interface StrandJ { label: string; at: string; red?: boolean }
/** What the engine draws for a cell (`display`): the strands of each object, each matrix entry. */
export interface DisplayJ { objects?: Record<string, StrandJ[]>; entries?: Record<string, { summands: Summand[]; more?: number }> }
export interface ComplexResult {
  kind: 'complex';
  display?: DisplayJ;
  complex?: ComplexJ;
  check?: { holds: boolean; failures?: FailureJ[]; failure_count?: number };
  homology?: HomologyJ;
  euler?: { terms?: { object: string; coefficient: string }[] };
  resolution?: ResolutionJ;
}
export interface ModuleResult {
  kind: 'module';
  display?: DisplayJ;
  module?: ModuleJ;
  holds?: boolean;
  failures?: FailureJ[];
  failure_count?: number;
  dimensions?: Dims[];
  dimension?: number;
  complete?: boolean;
}

/** What the tools of a complex or module cell can do. */
export type ComplexAction = 'extend' | 'kernel' | 'gauss' | 'tikz' | 'json' | 'resolve';

/** `complex R := {…}` or `module L := {…}`, read: the name and the JSON (null when it does not parse). */
export function readStructured(text: string): { kind: 'complex' | 'module'; name: string; value: ComplexJ & ModuleJ; tail: string } | null {
  const m = /^\s*(complex|module)\s+([^\s:]+)\s*:=\s*([\s\S]*)$/.exec(text);
  if (!m) return null;
  const body = m[3].trim();
  // the JSON object ends at its matching brace; options may follow it
  let depth = 0;
  let inString = false;
  let end = -1;
  for (let i = 0; i < body.length; i++) {
    const c = body[i];
    if (inString) {
      if (c === '\\') i++;
      else if (c === '"') inString = false;
    } else if (c === '"') inString = true;
    else if (c === '{') depth++;
    else if (c === '}' && --depth === 0) { end = i + 1; break; }
  }
  try {
    return { kind: m[1] as 'complex' | 'module', name: m[2], value: JSON.parse(end > 0 ? body.slice(0, end) : body), tail: end > 0 ? body.slice(end).trim() : '' };
  } catch {
    return null;
  }
}

/**
 * The text of a complex cell for a complex a tool returned: the defaults the engine fills in are
 * left out, and a resolution of a module cell keeps naming that cell (`module`), so the cell stays
 * as short as the one the person wrote and still follows the module when it changes.
 */
export function complexCellText(name: string, c: ComplexJ & Record<string, unknown>, module?: string, tail = ''): string {
  const out: Record<string, unknown> = {};
  const defaults: Record<string, unknown> = { dimension: 1, indexing: 'cohomological', odd_signs: 'koszul', squares: 'commute' };
  for (const [k, v] of Object.entries(c)) if (defaults[k] !== v) out[k] = v;
  const terms = (out.terms as TermJ[] | undefined)?.map((t) => ({ ...t, summands: t.summands.map((x) => (x.shift ? x : { object: x.object })) }));
  if (terms) out.terms = terms;
  if (module && out.resolves) out.resolves = { ...(out.resolves as object), module };
  return `complex ${name} := ${JSON.stringify(out)}${tail ? ' ' + tail : ''}`;
}

/** `P(i@1)⟨2⟩`: the projective of an object, shifted. */
export const summandText = (s: SummandJ) => `P(${s.object})${s.shift ? `⟨${s.shift}⟩` : ''}`;
const termText = (ss: SummandJ[]) => (ss.length ? ss.map(summandText).join(' ⊕ ') : '0');
const posText = (p: number | number[]) => (Array.isArray(p) ? `(${p.join(', ')})` : String(p)).replace(/-/g, '−');

/** One line for a collapsed complex or module cell: its name and its shape, not its JSON. */
export function structuredHead(text: string): string | null {
  const r = readStructured(text);
  if (!r) return null;
  const v = r.value;
  if (r.kind === 'module') {
    if (v.head !== undefined) {
      const h = v.head;
      const p = typeof h === 'string' ? summandText({ object: h }) : Array.isArray(h) ? termText(h as SummandJ[]) : summandText(h as SummandJ);
      return `${r.name} := ${p} / rad`;
    }
    const g = v.generators?.length ?? 0;
    const n = v.relations?.length ?? 0;
    return n ? `${r.name} := coker(${termText(v.relations!)} → ${termText(v.generators ?? [])})` : `${r.name} := ${termText(v.generators ?? [])}${g ? '' : ' (zero)'}`;
  }
  const ts = ordered(v);
  const shape = ts.length ? ts.map((t) => termText(t.summands)).join(' → ') : '0';
  const target = v.resolves ? ` → ${typeof v.resolves.module === 'string' ? v.resolves.module : 'M'}` : '';
  return `${r.name} := ${shape}${target}`;
}

/** The terms of a one-dimensional complex in the order its arrows point (left to right). */
function ordered(c: ComplexJ): TermJ[] {
  const ts = (c.terms ?? []).filter((t) => !Array.isArray(t.at) || t.at.length === 1);
  const at = (t: TermJ) => (Array.isArray(t.at) ? t.at[0] : t.at);
  const step = c.indexing === 'homological' ? -1 : 1;
  return [...ts].sort((a, b) => step * (at(a) - at(b)));
}

/**
 * A projective, named by its strands from left to right: `P(i+j | i j)⟨2⟩` with the red strands of
 * the context in red and each position as a subscript. The exact object is the tooltip.
 */
export function Proj(props: { s: SummandJ; d?: DisplayJ }) {
  const strands = () => props.d?.objects?.[props.s.object];
  return (
    <span class="cx-proj" title={summandText(props.s)} data-object={props.s.object}>
      <Show when={strands()} fallback={summandText(props.s)}>
        {(ss) => (
          <>
            P(<For each={ss()}>{(x, i) => <><Show when={i() > 0}>{' '}</Show><span class={x.red ? 'cx-strand cx-red' : 'cx-strand'}>{x.label}<sub>{x.at}</sub></span></>}</For>)
            <Show when={props.s.shift}><span class="cx-shift">⟨{props.s.shift}⟩</span></Show>
          </>
        )}
      </Show>
    </span>
  );
}

/** One matrix entry: the diagram of each of its summands, or its text when there is no drawing. */
function Entry(props: { e: EntryText; d?: DisplayJ }) {
  const drawn = () => (props.e == null ? undefined : props.d?.entries?.[props.e]);
  return (
    <Show when={!isZero(props.e)} fallback={<span class="hint">0</span>}>
      <Show when={drawn()?.summands.length} fallback={<Typeset text={props.e!} />}>
        <span class="cx-entry" title={props.e!} data-testid="cx-entry">
          <For each={drawn()!.summands}>
            {(x, k) => (
              <>
                <Show when={k() > 0 || x.negative}><span class="cx-sign">{x.negative ? '−' : '+'}</span></Show>
                <Show when={x.coefficient}><span class="cx-coeff">{x.coefficient}</span></Show>
                <span class="cx-diag" innerHTML={x.svg} />
              </>
            )}
          </For>
          <Show when={(drawn()!.more ?? 0) > 0}><span class="hint"> + {drawn()!.more} more</span></Show>
        </span>
      </Show>
    </Show>
  );
}

/** A matrix with its rows labelled by the target summands and its columns by the source ones. */
function MatrixView(props: { m?: EntryText[][]; label: string; d?: DisplayJ; src?: SummandJ[]; tgt?: SummandJ[] }) {
  const rows = () => props.m ?? [];
  const heads = () => !!(props.src?.length === rows()[0]?.length && props.tgt?.length === rows().length);
  return (
    <Show when={rows().length && rows()[0].length} fallback={<span class="hint">0</span>}>
      <table class="cx-matrix" aria-label={props.label}>
        <Show when={heads()}>
          <thead><tr><th /><For each={props.src}>{(s) => <th><Proj s={s} d={props.d} /></th>}</For></tr></thead>
        </Show>
        <tbody>
          <For each={rows()}>{(r, i) => <tr><Show when={heads()}><th><Proj s={props.tgt![i()]} d={props.d} /></th></Show><For each={r}>{(e) => <td><Entry e={e} d={props.d} /></td>}</For></tr>}</For>
        </tbody>
      </table>
    </Show>
  );
}

function Term(props: { t: TermJ; d?: DisplayJ; homology?: NonNullable<HomologyJ['positions']>[number]; caption?: string }) {
  return (
    <div class="cx-term" data-testid="cx-term">
      <div class="cx-summands">
        <Show when={props.t.summands.length} fallback={<span class="hint">0</span>}>
          <For each={props.t.summands}>{(s, i) => <><Show when={i() > 0}><span class="cx-oplus">⊕</span></Show><span class="cx-summand"><Proj s={s} d={props.d} /></span></>}</For>
        </Show>
      </div>
      <div class="cx-pos hint">{props.caption ?? posText(props.t.at)}</div>
      <Show when={props.homology}>
        {(h) => (
          <div class={`cx-h ${h().zero ? 'zero' : 'nonzero'}`} title={h().zero ? 'the homology here is zero' : 'the homology here is not zero'}>
            {h().zero ? 'H = 0' : `H ≠ 0${h().pieces?.length ? `: ${dimsOf(h().pieces!)}` : ''}`}
          </div>
        )}
      </Show>
    </div>
  );
}

/** Pieces of homology as graded dimensions per test object: `q^4 at i@1`. */
function dimsOf(ps: { test: string; degree: number; dimension: number }[]): string {
  const by = new Map<string, [number, number][]>();
  for (const p of ps) by.set(p.test, [...(by.get(p.test) ?? []), [p.degree, p.dimension]]);
  return [...by].map(([t, ds]) => `${laurent(ds)} at ${t}`).join(', ');
}
function laurent(ds: [number, number][]): string {
  const out = ds.filter(([, n]) => n).sort((a, b) => a[0] - b[0]).map(([d, n]) => {
    const c = Math.abs(n) === 1 && d !== 0 ? '' : String(Math.abs(n));
    const q = d === 0 ? '' : d === 1 ? 'q' : `q^${d}`;
    return `${n < 0 ? '−' : '+'}${c}${q}`;
  }).join(' ').replace(/^\+/, '');
  return out || '0';
}
const dimsLine = (ds?: Dims[]) => (ds ?? []).map((d) => `${d.dimensions} at ${d.test}`).join(', ');

/**
 * An arrow with its map: a 1×1 matrix is drawn on the arrow itself; a larger one is a button
 * (`2×3`) that opens the matrix, its rows and columns labelled by the summands they belong to.
 */
function Arrow(props: { m?: EntryText[][]; label: string; name?: string; d?: DisplayJ; src?: SummandJ[]; tgt?: SummandJ[] }) {
  const [open, setOpen] = createSignal(false);
  const rows = () => props.m?.length ?? 0;
  const cols = () => props.m?.[0]?.length ?? 0;
  const single = () => rows() === 1 && cols() === 1;
  const nonzero = () => (props.m ?? []).flat().filter((e) => !isZero(e)).length;
  return (
    <div class="cx-arrow">
      <Show when={props.m && rows() && cols()} fallback={<span class="cx-matrix-none" />}>
        <Show when={!single()} fallback={<div class="cx-on-arrow" aria-label={props.label}><Entry e={props.m![0][0]} d={props.d} /></div>}>
          <button class="cx-matrix-chip ghost" aria-expanded={open()} data-testid="cx-matrix-toggle" onClick={() => setOpen(!open())}
            title={open() ? 'Hide the matrix' : 'Show the matrix: rows are the summands of the target, columns those of the source'}>
            {rows()}×{cols()} matrix, {nonzero()} nonzero {open() ? '▴' : '▾'}
          </button>
          <Show when={open()}><MatrixView m={props.m} label={props.label} d={props.d} src={props.src} tgt={props.tgt} /></Show>
        </Show>
      </Show>
      <div class="cx-shaft" aria-hidden="true">{props.name ? <span class="cx-name">{props.name}</span> : null}⟶</div>
    </div>
  );
}

function Failures(props: { fs?: FailureJ[]; count?: number }) {
  return (
    <Show when={props.fs?.length}>
      <ul class="cx-failures">
        <For each={props.fs}>{(f) => <li><b>{f.location}</b>: {f.detail}</li>}</For>
        <Show when={(props.count ?? 0) > (props.fs?.length ?? 0)}><li class="hint">and {props.count! - props.fs!.length} more</li></Show>
      </ul>
    </Show>
  );
}

function Tools(props: { children: unknown }) {
  return <div class="cx-tools row" style={{ 'flex-wrap': 'wrap' }}>{props.children as never}</div>;
}

export function ComplexView(props: { r: ComplexResult; name?: string; module?: string; onAction?: (a: ComplexAction) => void; busy?: string }) {
  const c = () => props.r.complex ?? {};
  const oneDim = () => (c().dimension ?? 1) === 1;
  const terms = () => ordered(c());
  const at = (t: TermJ) => (Array.isArray(t.at) ? t.at[0] : t.at);
  const diff = (p: number) => c().differentials?.find((d) => (Array.isArray(d.from) ? d.from[0] : d.from) === p && !d.axis)?.matrix;
  const hom = (p: number) => props.r.homology?.positions?.find((x) => (Array.isArray(x.at) ? x.at[0] : x.at) === p);
  const res = () => props.r.resolution;
  const resolves = () => c().resolves;
  const d = () => props.r.display;
  const moduleGens = () => { const m = resolves()?.module; return typeof m === 'object' ? m.generators : undefined; };
  const moduleName = () => props.module ?? (typeof resolves()?.module === 'string' ? (resolves()!.module as string) : 'M');
  const checkOk = () => (resolves() ? res()?.holds !== false : props.r.check?.holds !== false);
  const failures = () => (resolves() ? res()?.failures : props.r.check?.failures);
  const failureCount = () => (resolves() ? res()?.failure_count : props.r.check?.failure_count);
  const act = (a: ComplexAction) => props.onAction?.(a);
  const busy = () => !!props.busy;
  return (
    <div class="cx" data-testid="complex-view">
      <Show when={oneDim()} fallback={<MultiView c={c()} d={props.r.display} />}>
        <div class="cx-diagram" role="figure" aria-label={`the complex${props.name ? ' ' + props.name : ''}`}>
          <For each={terms()}>
            {(t, i) => (
              <span class="cx-step">
                <Show when={i() > 0}><Arrow m={diff(at(terms()[i() - 1]))} label={`differential from ${posText(at(terms()[i() - 1]))}`} d={d()} src={terms()[i() - 1].summands} tgt={t.summands} /></Show>
                <Term t={t} d={d()} homology={resolves() ? undefined : hom(at(t))} />
              </span>
            )}
          </For>
          <Show when={resolves()}>
            <span class="cx-step">
              <Arrow m={resolves()!.augmentation} label="the augmentation ε" name="ε " d={d()} src={terms().at(-1)?.summands} tgt={moduleGens()} />
              <div class="cx-term cx-module" data-testid="cx-module"><div class="cx-summands"><span class="cx-summand">{moduleName()}</span></div><div class="cx-pos hint">module</div></div>
            </span>
          </Show>
        </div>
      </Show>
      <Show when={!checkOk()}>
        <div class="row"><span class="badge bad" data-testid="verdict">{resolves() ? 'not a resolution' : 'not a complex'}</span><span class="hint">{failureCount()} {failureCount() === 1 ? 'problem' : 'problems'}; fix the text of the cell, then the tools come back.</span></div>
        <Failures fs={failures()} count={failureCount()} />
      </Show>
      <Show when={checkOk() && resolves() && res()}>
        {(r) => (
          <div class="cx-status" data-testid="resolution-status">
            <Show when={r().resolved} fallback={
              <div class="row" style={{ 'flex-wrap': 'wrap' }}>
                <span class="badge warn">resolution in progress</span>
                <span>exact so far, length {r().length}. The kernel of the leftmost map is not yet covered{r().uncovered?.length ? `: ${dimsLine(r().uncovered)}` : ''}{r().complete === false ? ' (in the degrees computed)' : ''}.</span>
              </div>
            }>
              <div class="row"><span class="badge good" data-testid="verdict">resolves {moduleName()}</span>
                <span>length {r().length}{r().minimal === true ? ', minimal' : r().minimal === false ? ', not minimal' : ''}{r().complete === false ? ' (in the degrees computed)' : ''}</span></div>
            </Show>
            <div class="hint">{moduleName()}: {dimsLine(r().module) || 'zero'}{r().module_dimension !== undefined ? ` (dimension ${r().module_dimension})` : ''}</div>
          </div>
        )}
      </Show>
      <Show when={checkOk() && !resolves() && props.r.homology}>
        {(h) => (
          <div class="cx-status">
            <span class={`badge ${h().exact ? 'good' : 'warn'}`}>{h().exact ? 'exact' : 'not exact'}</span>
            <Show when={props.r.euler?.terms?.length}><span class="hint"> Euler characteristic: {props.r.euler!.terms!.map((t) => `(${t.coefficient})[${t.object}]`).join(' + ')}</span></Show>
          </div>
        )}
      </Show>
      <Show when={props.onAction}>
        <Tools>
          <Show when={checkOk() && resolves() && res() && !res()!.resolved}>
            <button class="primary" disabled={busy()} onClick={() => act('extend')} data-testid="cx-extend"
              title="Find generators of the kernel of the leftmost map, degree by degree, and add them as the next term">
              Cover the kernel{res()!.next_at != null ? ` (adds the term at ${posText(res()!.next_at!)})` : ''}
            </button>
          </Show>
          <Show when={checkOk() && !resolves() && props.r.resolution?.next_at != null}>
            <button class="primary" disabled={busy()} onClick={() => act('kernel')} data-testid="cx-kernel"
              title="Find generators of the kernel at the left end and add them as a new term">
              Cover the kernel at {posText(props.r.resolution!.next_at!)}
            </button>
          </Show>
          <Show when={checkOk() && oneDim()}>
            <button class={res()?.minimal === false ? 'primary' : ''} disabled={busy()} onClick={() => act('gauss')} data-testid="cx-gauss"
              title="Cancel pairs of summands joined by an invertible entry (Gaussian elimination); the result is homotopy equivalent and still resolves the same module">
              Minimize
            </button>
          </Show>
          <Show when={oneDim()}><button class="ghost" disabled={busy()} onClick={() => act('tikz')}>Copy TikZ</button></Show>
          <button class="ghost" disabled={busy()} onClick={() => act('json')} title="The complex as JSON, for Python or the command line">Copy JSON</button>
          <Show when={props.busy}><span class="hint" role="status"><span class="spinner" aria-hidden="true" /> {props.busy}</span></Show>
        </Tools>
      </Show>
    </div>
  );
}

/** A complex of dimension above one: its terms and differentials, listed. */
function MultiView(props: { c: ComplexJ; d?: DisplayJ }) {
  return (
    <div class="cx-multi">
      <div class="hint">A {props.c.dimension}-dimensional complex; the diagram is drawn for one-dimensional ones.</div>
      <ul>
        <For each={props.c.terms ?? []}>{(t) => <li>{posText(t.at)}: <For each={t.summands}>{(s, i) => <><Show when={i() > 0}> ⊕ </Show><Proj s={s} d={props.d} /></>}</For></li>}</For>
        <For each={props.c.differentials ?? []}>{(x) => <li>d<sub>{x.axis ?? 0}</sub> from {posText(x.from)}: <MatrixView m={x.matrix} label="differential" d={props.d} /></li>}</For>
      </ul>
    </div>
  );
}

export function ModuleView(props: { r: ModuleResult; name?: string; head?: boolean; onAction?: (a: ComplexAction) => void; busy?: string }) {
  const m = () => props.r.module ?? {};
  const gens = () => m().generators ?? [];
  const rels = () => m().relations ?? [];
  return (
    <div class="cx" data-testid="module-view">
      <div class="cx-diagram" role="figure" aria-label={`the presentation of ${props.name ?? 'the module'}`}>
        <Show when={rels().length}>
          <Term t={{ at: 1, summands: rels() }} d={props.r.display} caption="relations" />
          <Arrow m={m().matrix} label="the presentation matrix" name="φ " d={props.r.display} src={rels()} tgt={gens()} />
        </Show>
        <Term t={{ at: 0, summands: gens() }} d={props.r.display} caption="generators" />
        <Arrow label="" />
        <div class="cx-term cx-module"><div class="cx-summands"><span class="cx-summand">{props.name ?? 'M'}</span></div><div class="cx-pos hint">module</div></div>
      </div>
      <div class="hint cx-legend">{rels().length ? `${props.name ?? 'M'} = coker φ: generators on the right, relations on the left${props.head ? ', as found for P/rad P' : ''}.` : 'No relations: the module is projective.'}</div>
      <Show when={props.r.holds === false}>
        <div class="row"><span class="badge bad" data-testid="verdict">not a module</span></div>
        <Failures fs={props.r.failures} count={props.r.failure_count} />
      </Show>
      <Show when={props.r.dimensions}>
        <div class="cx-status">graded dimension: {dimsLine(props.r.dimensions) || 'zero'}{props.r.dimension !== undefined ? ` (total ${props.r.dimension})` : ''}{props.r.complete === false ? ' (in the degrees computed)' : ''}</div>
      </Show>
      <Show when={props.onAction}>
        <Tools>
          <Show when={props.r.holds !== false}>
            <button class="primary" disabled={!!props.busy} onClick={() => props.onAction!('resolve')} data-testid="md-resolve"
              title="Add a complex cell below: the generators as C₀, mapping onto the module; then cover kernels one step at a time">
              Start a resolution
            </button>
          </Show>
          <button class="ghost" disabled={!!props.busy} onClick={() => props.onAction!('json')} title="The presentation as JSON">Copy JSON</button>
          <Show when={props.busy}><span class="hint" role="status"><span class="spinner" aria-hidden="true" /> {props.busy}</span></Show>
        </Tools>
      </Show>
    </div>
  );
}
