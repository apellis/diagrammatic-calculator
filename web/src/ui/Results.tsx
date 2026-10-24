// How each kind of result is shown.
import { createEffect, createSignal, For, Match, on, onCleanup, Show, Switch } from 'solid-js';
import type { CellState, CellValue } from '../eval/evaluator';
import type { Outcome, Sizes } from '../engine/engine';
import type { Provenance, Summand, WorkspaceRunCell, WorkspaceTraceResponse } from '../engine/ops.gen';
import katex from 'katex';
import { KATEX_SAFE } from './tex';
import { Typeset, Value } from './Typeset';
import { proseHtml } from './prose';

type Ok = WorkspaceRunCell;

const ok = (v: CellValue | undefined): v is Ok => !!v && v.status === 'ok';

/** One line saying what a value is, for collapsed cells and the outline. */
export function summary(s: CellState): string {
  if (s.parseError) return 'error';
  const v = s.value;
  if (s.status === 'running') return 'computing…';
  if (!v) return '';
  if (v.status === 'exploded') return 'stopped';
  if (v.status === 'error') return v.message === 'cancelled' ? 'cancelled' : 'error';
  const r = v as Ok;
  const stale = s.status === 'stale' ? ' (stale)' : '';
  switch (r.kind) {
    case 'def':
      return `${r.terms ?? ''} ${r.terms === 1 ? 'term' : 'terms'}${stale}`;
    case 'watch': {
      if (!r.terms) return `0${stale}`;
      const degs = [...new Set((r.summands ?? []).map((x) => x.degree))].sort((a, b) => a - b);
      const d = degs.length === 1 ? `degree ${degs[0]}` : degs.length ? `degrees ${degs[0]} to ${degs.at(-1)}` : '';
      return `${r.terms} ${r.terms === 1 ? 'term' : 'terms'}${d ? ', ' + d : ''}${stale}`;
    }
    case 'check':
      return (r.holds ? 'holds' : 'fails') + stale;
    case 'solve':
      return solveSummary(r) + stale;
  }
  return '';
}

/** A clock that ticks every second while some component reads it. */
const [now, setNow] = createSignal(Date.now());
let ticking = 0;
let tick: ReturnType<typeof setInterval> | undefined;
function useClock() {
  if (ticking++ === 0) {
    setNow(Date.now());
    tick = setInterval(() => setNow(Date.now()), 1000);
  }
  onCleanup(() => {
    if (--ticking === 0) clearInterval(tick);
  });
  return now;
}

/** Seconds as m:ss, or s s under a minute. */
export function elapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** How long a running cell has been computing, ticking. */
function Elapsed(props: { since?: number }) {
  const clock = useClock();
  return <Show when={props.since}>{(t) => <span class="elapsed">{elapsed(clock() - t())}</span>}</Show>;
}

/**
 * The one-line summary of a cell's value, for collapsed cells and the outline: what the value is,
 * how long it has been computing, and a mark when the value rests on an open hypothesis.
 */
export function Summary(props: { state?: CellState; small?: boolean }) {
  const c = () => (props.state && ok(props.state.value) ? certainty(props.state.value) : null);
  const running = () => props.state?.status === 'running' || props.state?.status === 'stale';
  return (
    <span class="summary-wrap" classList={{ small: props.small }}>
      <span class="summary" data-testid="summary" classList={{ fails: !!props.state && ok(props.state.value) && props.state.value.kind === 'check' && !props.state.value.holds && props.state.status === 'fresh' }}>
        {props.state ? summary(props.state) : ''}
      </span>
      <Show when={running()}><Elapsed since={props.state?.since} /></Show>
      <Show when={c()?.mark}>{(m) => <span class="cond-mark" title={c()!.text}>{m()}</span>}</Show>
      <Show when={props.state && ok(props.state.value) && props.state.value.warnings?.length}>
        <span class="cond-mark warn-mark" data-testid="warn-mark" title={plain((props.state!.value as Ok).warnings!.join('\n'))}
          aria-label="see warning">{props.small ? '⚠' : 'see warning'}</span>
      </Show>
    </span>
  );
}

/** Text with $…$ formulas, as plain text for a tooltip. */
const plain = (t: string) => t.replace(/\$([^$]*)\$/g, '$1').replace(/\\nu\b/g, 'ν').replace(/\\Lambda\b/g, 'Λ');

/** What a result does not show, such as an idempotent outside the weight that makes a term 0. */
export function Warnings(props: { value: Ok }) {
  return (
    <For each={props.value.warnings ?? []}>
      {(w) => <div class="warning-line" data-testid="warning"><span aria-hidden="true">⚠</span> <NoteView text={w} inline /></div>}
    </For>
  );
}

/** How far a value can be trusted, read off its provenance (and, for a check, its verdict). */
export interface Certainty {
  /** proven, or resting on an open conjecture, an assumption, or nothing known. */
  level: 'proven' | 'conditional' | 'assumed' | 'unknown';
  /** A short mark for summaries ('' when proven). */
  mark: string;
  /** One line saying what the value rests on. */
  text: string;
}

/** The open conjecture a certificate names, such as "Nondegeneracy Conjecture". */
export function conjecture(p: Provenance): string | null {
  const m = /([A-Z][A-Za-z'-]*(?:[ -][A-Z][A-Za-z'-]*)* Conjecture)/.exec(`${p.certificate.reference} ${p.certificate.hypotheses}`);
  return m ? m[1] : null;
}

export function certainty(v: Ok): Certainty | null {
  const p = v.provenance;
  if (!p) return null;
  if ((v.kind === 'check' || v.kind === 'watch') && v.certified_by === 'computation') {
    return { level: 'proven', mark: '', text: 'proven by the computation alone: every step is a relation of the presentation' };
  }
  switch (p.certificate.status) {
    case 'lean':
      return { level: 'proven', mark: '', text: 'proven: the strategy is checked in Lean' };
    case 'literature':
      return { level: 'proven', mark: '', text: 'proven: the strategy rests on a published theorem' };
    case 'conditional': {
      const c = conjecture(p);
      const on = c ? `the ${c}` : 'an open conjecture';
      const what = v.kind === 'check' ? (v.holds ? 'that it holds' : 'that it fails') : 'that the terms of the normal form are independent';
      // The accepted working assumption is specifically BE2 nondegeneracy, not all conjectures.
      const be2 = c === 'Nondegeneracy Conjecture' && /\barXiv:1701\.04133(?:v\d+)?\b/.test(p.certificate.reference);
      return { level: 'conditional', mark: be2 ? 'Assuming nondegeneracy' : 'conditional', text: `conditional on ${on}: ${what} rests on it` };
    }
    case 'assumed':
      return { level: 'assumed', mark: 'assumed', text: 'rests on an assumption: the representation is assumed faithful' };
    default:
      return { level: 'unknown', mark: 'unproven', text: 'unproven: whether the representation is faithful is not known' };
  }
}

function solveSummary(r: Ok): string {
  switch (r.outcome) {
    case 'none': return 'no solution';
    case 'unique': return 'unique solution';
    case 'finite': return `${r.points?.length ?? 0} solutions`;
    case 'family': return `family of dimension ${r.dimension}`;
    case 'limit_reached': return 'limit reached';
  }
  return '';
}

const STATUS: Record<string, string> = {
  lean: 'proven in Lean',
  literature: 'proven in the literature',
  conditional: 'conditional',
  assumed: 'assumed',
  unknown: 'unknown',
};

/**
 * Quiet disclosure of the strategy, certificate and exact hypotheses. Accepted BE2
 * assumptions stay explicit without warning styling; other uncertainty and notes still warn.
 */
export function Badge(props: { p?: Provenance; value?: Ok; compact?: boolean }) {
  const [open, setOpen] = createSignal(false);
  const c = () => (props.value ? certainty(props.value) : null);
  const warn = () => !!props.p && ((c() ? c()!.level !== 'proven' && c()!.mark !== 'Assuming nondegeneracy' : ['conditional', 'assumed', 'unknown'].includes(props.p.certificate.status)) || (props.p.notes?.length ?? 0) > 0);
  return (
    <Show when={props.p}>
      {(p) => (
        <span class="badge-wrap" classList={{ compact: props.compact }}>
          <button class={`badge ${warn() ? 'warn' : ''}`} aria-expanded={open()} data-testid="badge" onClick={() => setOpen(!open())}
            title={open() ? 'Hide how this was computed' : 'How this was computed'}>
            How computed{' '}
            <span class="caret" aria-hidden="true">{open() ? '▴' : '▾'}</span>
          </button>
          {/* a check says it beside its verdict */}
          <Show when={c() && c()!.level !== 'proven' && props.value?.kind !== 'check'}>
            <span class="cond-text" data-testid="cond-text">{c()!.mark === 'Assuming nondegeneracy' ? c()!.mark : props.compact ? c()!.text.replace(/:.*$/, '') : c()!.text}</span>
          </Show>
          <Show when={!props.compact}>
            <For each={p().notes ?? []}>{(n) => <span class="hint note-line" data-testid="prov-note"><NoteView text={n} inline /></span>}</For>
          </Show>
          <Show when={open()}>
            <div classList={{ pop: props.compact, 'prov-pop': props.compact }}>
            <table class="kv provenance" data-testid="provenance">
              <tbody>
                <Show when={c()}>{(x) => <tr><td>this result</td><td>{x().text}</td></tr>}</Show>
                <tr><td>strategy</td><td>{p().strategy}</td></tr>
                <tr><td>method</td><td>{p().method}</td></tr>
                <tr><td>certificate</td><td>{STATUS[p().certificate.status] ?? p().certificate.status}</td></tr>
                <Show when={p().certificate.reference}><tr><td>reference</td><td class="prose" data-testid="cert-reference" innerHTML={proseHtml(p().certificate.reference)} /></tr></Show>
                <Show when={p().certificate.hypotheses}><tr><td>hypotheses</td><td class="prose" data-testid="cert-hypotheses" innerHTML={proseHtml(p().certificate.hypotheses)} /></tr></Show>
                <Show when={props.compact}><For each={p().notes ?? []}>{(n) => <tr><td>note</td><td><NoteView text={n} inline /></td></tr>}</For></Show>
              </tbody>
            </table>
            </div>
          </Show>
        </span>
      )}
    </Show>
  );
}

/** The badge of a computed value: a definition's, or the strategy and certificate of any other. */
export function ValueBadge(props: { value: Ok; compact?: boolean }) {
  return (
    <Show when={props.value.kind === 'def'} fallback={<Badge p={props.value.provenance} value={props.value} compact={props.compact} />}>
      <DefBadge />
    </Show>
  );
}

/** A definition's value is its term as written: no relation is applied, so no strategy is involved. */
function DefBadge() {
  const [open, setOpen] = createSignal(false);
  return (
    <span class="badge-wrap">
      <button class="badge" aria-expanded={open()} data-testid="badge" onClick={() => setOpen(!open())}>
        as written · no relations used<span class="caret" aria-hidden="true">{open() ? '▴' : '▾'}</span>
      </button>
      <Show when={open()}>
        <span class="hint">
          A definition's value is its term as written, with like terms collected; no relation is applied, so it rests on no
          certificate. Watch it to see its normal form.
        </span>
      </Show>
    </span>
  );
}

/** Asks which terms of the definitions produced a summand of a watch (`workspace.trace`). */
export type Tracer = (word: string, signal: AbortSignal) => Promise<Outcome<WorkspaceTraceResponse>>;

function SummandView(props: { s: Summand; cls?: string; first: boolean; onFocus?: (word: string) => void; onPick?: (word: string) => void; picked?: boolean }) {
  const sign = () => (props.s.negative ? '−' : props.first ? '' : '+');
  const hints = () => [props.onPick && 'click: where it comes from', props.onFocus && 'double-click: open the canvas'].filter(Boolean).join('; ');
  return (
    <span class={`summand ${props.cls ?? ''}`} classList={{ picked: !!props.picked }} title={hints() ? `${props.s.text}\n(${hints()})` : props.s.text} data-word={props.s.word}
      role={props.onPick ? 'button' : undefined} tabindex={props.onPick ? 0 : undefined} aria-pressed={props.onPick ? !!props.picked : undefined}
      onClick={() => props.onPick?.(props.s.word)}
      onKeyDown={(e) => { if (props.onPick && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); props.onPick(props.s.word); } }}
      onDblClick={() => props.onFocus?.(props.s.word)}>
      <Show when={sign() || props.s.coefficient}>
        <span class="c">{sign()}<Show when={props.s.coefficient}>{(c) => <>{sign() ? ' ' : ''}<Typeset text={c()} /></>}</Show></span>
      </Show>
      <span innerHTML={props.s.svg} />
    </span>
  );
}

/**
 * Where one summand of a watch comes from: the products of terms of the definitions that
 * contribute to it, each with its contribution; they add up to the summand.
 */
function TraceView(props: { word: string; text: string; trace: Tracer; onClose: () => void }) {
  const [res, setRes] = createSignal<{ state: 'running' } | { state: 'done'; r: WorkspaceTraceResponse } | { state: 'failed'; message: string }>({ state: 'running' });
  createEffect(on(() => props.word, (word) => {
    setRes({ state: 'running' });
    const abort = new AbortController();
    void props.trace(word, abort.signal).then((o) => {
      if (abort.signal.aborted) return;
      if (o.status === 'done') setRes({ state: 'done', r: o.response });
      else setRes({ state: 'failed', message: o.status === 'exploded' ? o.message : 'cancelled' });
    });
    onCleanup(() => abort.abort());
  }));
  const factor = (f: { name: string; term: number; text: string; power: number }) => `${f.name}, term ${f.term}${f.power > 1 ? ` (used ${f.power} times)` : ''}`;
  return (
    <div class="trace" data-testid="trace" role="region" aria-label="Where the summand comes from">
      <div class="row">
        <span>Where <Typeset text={props.text} /> comes from</span>
        <button class="small" onClick={() => props.onClose()} aria-label="Close" data-testid="trace-close">×</button>
      </div>
      <Switch>
        <Match when={res().state === 'running'}><div class="hint">Tracing…</div></Match>
        <Match when={res().state === 'failed' && res()}>{(x) => <div class="error">{(x() as { message: string }).message}</div>}</Match>
        <Match when={res().state === 'done' && (res() as { r: WorkspaceTraceResponse }).r}>
          {(r) => (
            <Show when={r().status === 'ok' && (r() as Extract<WorkspaceTraceResponse, { status: 'ok' }>)} fallback={<div class="error">{(r() as { message?: string }).message}</div>}>
              {(t) => (
                <ul class="trace-list">
                  <For each={t().sources}>
                    {(src) => (
                      <li data-testid="trace-source">
                        <Show when={src.factors.length} fallback={<span class="hint">the watch's own terms</span>}>
                          <For each={src.factors}>
                            {(f, i) => <>{i() > 0 ? ' · ' : ''}<span class="trace-factor" title={factor(f)}><Typeset text={f.text} /><Show when={f.power > 1}><sup>{f.power}</sup></Show> <span class="hint">({f.name} {f.term})</span></span></>}
                          </For>
                        </Show>
                        <span class="hint"> give </span><Typeset text={src.text} />
                      </li>
                    )}
                  </For>
                </ul>
              )}
            </Show>
          )}
        </Match>
      </Switch>
    </div>
  );
}

/** Whether values are read with their commuting dots gathered into monomials: one choice for every watch, kept in this browser. */
const MONOMIALS = 'diagcalc.monomials';
const [gathered, setGathered] = createSignal((() => {
  try {
    return localStorage.getItem(MONOMIALS) === '1';
  } catch {
    return false;
  }
})());
const gather = (on: boolean) => {
  setGathered(on);
  try {
    localStorage.setItem(MONOMIALS, on ? '1' : '0');
  } catch {
    // the choice then lasts for this page only
  }
};

/** A sum of diagrams, outlining summands that are new since the previous value and showing the vanished ones briefly. */
export function Sum(props: { r: Ok; previous?: CellValue; text: string; valueKey?: string; onFocus?: (word: string) => void; source?: string; trace?: Tracer }) {
  const [picked, setPicked] = createSignal<string | null>(null);
  // a new value closes the trace: its summand may be gone
  createEffect(on(() => props.valueKey, () => setPicked(null), { defer: true }));
  const pick = (w: string) => setPicked(picked() === w ? null : w);
  const prevSummands = () => (ok(props.previous) ? props.previous.summands ?? [] : []);
  const words = () => new Set((props.r.summands ?? []).map((s) => s.word));
  const prevWords = () => new Set(prevSummands().map((s) => s.word));
  const added = () => (props.previous && ok(props.previous) ? new Set([...words()].filter((w) => !prevWords().has(w))) : new Set<string>());
  const gone = () => prevSummands().filter((s) => !words().has(s.word));
  const recoef = () => (props.r.summands ?? []).filter((s) => prevSummands().some((p) => p.word === s.word && p.text !== s.text));
  const [fading, setFading] = createSignal(false);
  createEffect(on(() => props.valueKey, () => {
    if (!gone().length) return;
    setFading(true);
    const t = setTimeout(() => setFading(false), 2000);
    return () => clearTimeout(t);
  }));
  const terms = () => props.r.terms ?? 0;
  const shown = () => props.r.summands?.length ?? 0;
  const shownText = () => (gathered() && props.r.monomials ? props.r.monomials : props.text);
  return (
    <div class="result">
      <div class="sum" role="img" aria-label={props.text}>
        <Show when={terms() > 0} fallback={<span class="summand"><span class="c">0</span></span>}>
          <For each={props.r.summands}>{(s, i) => <SummandView s={s} first={i() === 0} cls={added().has(s.word) ? 'new' : ''} onFocus={props.onFocus} onPick={props.trace && pick} picked={picked() === s.word} />}</For>
        </Show>
        <Show when={fading()}>
          <For each={gone()}>{(s) => <SummandView s={s} first={false} cls="gone" />}</For>
        </Show>
      </div>
      <Show when={props.trace ? (props.r.summands ?? []).find((s) => s.word === picked()) : undefined}>
        {(s) => <TraceView word={s().word} text={s().text} trace={props.trace!} onClose={() => setPicked(null)} />}
      </Show>
      <Show when={shown() < terms()}>
        <div class="hint">Showing {shown()} of {terms()} terms; the text form below has all of them.</div>
      </Show>
      <Show when={props.previous && ok(props.previous) && (added().size || gone().length || recoef().length)}>
        <details class="changed">
          <summary>Changed: {added().size} new, {gone().length} gone, {recoef().length} with a new coefficient</summary>
          <div class="textform">
            <For each={(props.r.summands ?? []).filter((s) => added().has(s.word))}>{(s) => <div>new: <Typeset text={s.text} /></div>}</For>
            <For each={gone()}>{(s) => <div>gone: <Typeset text={s.text} /></div>}</For>
            <For each={recoef()}>{(s) => <div>new coefficient: <Typeset text={s.text} /></div>}</For>
          </div>
        </details>
      </Show>
      {/* the text form only when it says more than the cell's own text; optionally with the dots gathered */}
      <Show when={!props.source || same(props.source) !== same(props.text) || !!props.r.monomials}>
        <div class="nf">
          <div class="row">
            <span class="hint">{props.source ? (gathered() && props.r.monomials ? 'Normal form, dots gathered' : 'Normal form') : ''}</span>
            <Show when={props.r.monomials}>
              <button class="small" aria-pressed={gathered()} data-testid="monomials" title="Write commuting dots together as monomials (x₁²x₃) instead of in the canonical layering; the same element"
                onClick={() => gather(!gathered())}>{gathered() ? 'Canonical layering' : 'Gather dots'}</button>
            </Show>
          </div>
          <Value class="textform" testid="textform" text={shownText()} />
        </div>
      </Show>
    </div>
  );
}

/** Text compared up to spacing. */
const same = (t: string) => t.replace(/\s+/g, '');

function CheckView(props: { r: Ok }) {
  const c = () => certainty(props.r);
  // with a conditional certificate, say which verdicts are proven and which rest on the conjecture
  const qualifier = () => {
    const x = c();
    if (!x || props.r.provenance?.certificate.status === 'lean' || props.r.provenance?.certificate.status === 'literature') return '';
    return x.level === 'proven' ? 'unconditional' : x.mark === 'Assuming nondegeneracy' ? x.mark : x.text.replace(/:.*$/, '');
  };
  return (
    <div class="result">
      <div class="row">
        <span class={`badge ${props.r.holds ? 'good' : 'bad'}`} data-testid="verdict">{props.r.holds ? 'holds' : 'fails'}</span>
        <Show when={qualifier()}><span class="verdict-note" classList={{ warn: c()?.level !== 'proven' }} data-testid="verdict-note">{qualifier()}</span></Show>
      </div>
      <Show when={props.r.witness}>
        <div class="hint">Witness (should vanish):</div>
        <Value class="textform" text={props.r.witness!} />
      </Show>
    </div>
  );
}

function assignments(m?: Record<string, string>) {
  return Object.entries(m ?? {}).map(([k, v]) => `${k} = ${v}`).join(', ');
}

/** Assignments typeset, one set per line. */
function Assigned(props: { lines: string[] }) {
  return <For each={props.lines}>{(l) => <div><Value text={l} /></div>}</For>;
}

function SolveView(props: { r: Ok }) {
  const r = () => props.r;
  return (
    <div class="result">
      <table class="kv">
        <tbody>
          <tr><td>outcome</td><td data-testid="outcome">{solveSummary(r())}</td></tr>
          <tr><td>method</td><td>{r().tier === 1 ? 'linear system' : 'Gröbner basis'}, {r().equations} {r().equations === 1 ? 'equation' : 'equations'}</td></tr>
          <Show when={r().solution}><tr><td>solution</td><td><Assigned lines={[assignments(r().solution)]} /></td></tr></Show>
          <Show when={r().particular}><tr><td>particular</td><td><Assigned lines={[assignments(r().particular)]} /></td></tr></Show>
          <Show when={r().directions?.length}>
            <tr><td>directions</td><td><Assigned lines={r().directions!.map((d) => assignments(d))} /></td></tr>
          </Show>
          <Show when={r().points?.length}>
            <tr><td>points</td><td><Assigned lines={r().points!.map((p) => `(${(r().variables ?? []).map((v, i) => `${v} = ${p[i]}`).join(', ')})`)} /></td></tr>
          </Show>
        </tbody>
      </table>
    </div>
  );
}

/** A value in brief, on one line where it fits: for lists such as the value changes a branch makes. */
export function Brief(props: { s?: CellState }) {
  const v = () => props.s?.value;
  const r = () => (ok(v()) ? (v() as Ok) : null);
  return (
    <span class="brief" data-testid="brief">
      <Switch fallback={<span class="hint">computing…</span>}>
        <Match when={props.s?.parseError}><span class="error">does not parse</span></Match>
        <Match when={props.s?.status === 'running' || props.s?.status === 'stale'}><span class="hint">computing…</span></Match>
        <Match when={v()?.status === 'error'}><span class="error">{(v() as { message: string }).message === 'cancelled' ? 'cancelled' : 'error'}</span></Match>
        <Match when={v()?.status === 'exploded'}><span class="error">stopped at the time limit</span></Match>
        <Match when={r()?.kind === 'watch' && r()}>
          {(x) => <><Typeset text={x().normal_form ?? ''} /> <span class="hint">({summary(props.s!)})</span></>}
        </Match>
        <Match when={r()?.kind === 'def' && r()}>{(x) => <Typeset text={x().term ?? ''} />}</Match>
        <Match when={r()?.kind === 'check' && r()}>
          {(x) => <span class={`badge ${x().holds ? 'good' : 'bad'}`}>{x().holds ? 'holds' : 'fails'}</span>}
        </Match>
        <Match when={r()?.kind === 'solve' && r()}>
          {(x) => <>{solveSummary(x())}<Show when={x().solution}>{(m) => <>: <Value text={assignments(m())} /></>}</Show></>}
        </Match>
      </Switch>
    </span>
  );
}

/** A note's text, with $…$ and $$…$$ as mathematics; `inline` for one line in a cell's head. */
export function NoteView(props: { text: string; inline?: boolean }) {
  const part = (part: string) => {
    if (part.startsWith('$$')) return katex.renderToString(part.slice(2, -2), { ...KATEX_SAFE, displayMode: !props.inline, throwOnError: false });
    if (part.startsWith('$') && part.length > 1) return katex.renderToString(part.slice(1, -1), { ...KATEX_SAFE, throwOnError: false });
    return part.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  };
  const split = (t: string) => t.split(/(\$\$[^$]+\$\$|\$[^$]+\$)/).map(part).join('');
  if (props.inline) return <span class="note" innerHTML={split(props.text)} />;
  const html = () => props.text.split(/\n\s*\n/).map((para) => `<p>${split(para)}</p>`).join('');
  return <div class="note" innerHTML={html()} />;
}

/** A running cell: how long it has run, and a button that stops it. */
function Running(props: { s: CellState; onStop?: () => void }) {
  return (
    <div class="running row" data-testid="running">
      <span class="spinner" aria-hidden="true" />
      <span>{props.s.status === 'stale' ? 'Recomputing' : 'Computing'}</span>
      <Elapsed since={props.s.since} />
      <Show when={props.onStop}><button onClick={() => props.onStop!()} data-testid="stop">Stop</button></Show>
    </div>
  );
}

/** Thousands separated by thin spaces: 1 234 567. */
const big = (n: number) => n.toLocaleString('en-US').replace(/,/g, '\u2009');

/** The sizes a stopped job had reached, one item each (docs/spec.md, Long jobs). */
export function sizeLines(d: Sizes): string[] {
  const out = [`stage: ${d.stage}`, `${big(d.terms)} ${d.terms === 1 ? 'summand' : 'summands'} at most`];
  if (d.layers) out.push(`${big(d.layers)} generators in the longest summand`);
  if (d.dots) out.push(`${big(d.dots)} dots in one summand at most`);
  if (d.basis !== undefined) out.push(`largest basis of a morphism space: ${big(d.basis)}`);
  if (d.polynomial !== undefined) out.push(`largest polynomial: ${big(d.polynomial)} terms`);
  if (d.steps !== undefined) out.push(`${big(d.steps)} rewriting steps`);
  return out;
}

/** A cell stopped at the time limit: what happened, how big it had got, and ways to go on. */
function Exploded(props: { message: string; diagnostics?: Sizes; onRunLonger?: () => void; commandLine?: () => string | undefined }) {
  const [copied, setCopied] = createSignal(false);
  const copy = async () => {
    const c = props.commandLine?.();
    if (!c) return;
    await navigator.clipboard?.writeText(c);
    setCopied(true);
  };
  return (
    <div class="exploded" data-testid="exploded">
      <div class="error">{props.message}</div>
      <Show when={props.diagnostics}>
        {(d) => (
          <div class="diagnostics" data-testid="diagnostics">
            <span class="hint">Sizes reached:</span>
            <ul><For each={sizeLines(d())}>{(l) => <li>{l}</li>}</For></ul>
          </div>
        )}
      </Show>
      <div class="row" style={{ 'margin-top': '6px', 'flex-wrap': 'wrap' }}>
        <Show when={props.onRunLonger}><button onClick={() => props.onRunLonger!()}>Run for up to 10 minutes</button></Show>
        <Show when={props.commandLine?.()}><button onClick={copy}>Copy command-line command</button></Show>
        <span class="hint" role="status">{copied() ? 'Copied: the command-line tool has no time limit.' : 'The command-line tool has no time limit.'}</span>
      </div>
    </div>
  );
}

export function ResultView(props: {
  s: CellState;
  onFocus?: (word?: string) => void;
  /** Traces a summand of a watch back to the terms of the definitions. */
  trace?: Tracer;
  onStop?: () => void;
  onRunLonger?: () => void;
  commandLine?: () => string | undefined;
}) {
  /** The cell's own text after its kind word, which a definition's text form may just repeat. */
  const source = () => props.s.text.replace(/^(def|watch)\s+/, '');
  const v = () => props.s.value;
  return (
    <Switch>
      <Match when={props.s.parseError}><div class="error">{props.s.parseError}</div></Match>
      <Match when={props.s.parsed?.kind === 'note'}><NoteView text={props.s.parsed!.note ?? ''} /></Match>
      <Match when={props.s.status === 'running'}><Running s={props.s} onStop={props.onStop} /></Match>
      <Match when={v()?.status === 'error'}><div class="error">{(v() as { message: string }).message === 'cancelled' ? 'Stopped before it finished. Recompute to run it again.' : (v() as { message: string }).message}</div></Match>
      <Match when={v()?.status === 'exploded'}><Exploded message={(v() as { message: string }).message} diagnostics={(v() as { diagnostics?: Sizes }).diagnostics} onRunLonger={props.onRunLonger} commandLine={props.commandLine} /></Match>
      <Match when={ok(v()) && (v() as Ok)} keyed>
        {(res) => {
          return (
            <div class={props.s.status === 'stale' ? 'result stale' : 'result'} data-testid="result">
              <Show when={props.s.status === 'stale'}>
                <div class="hint">Stale: the inputs changed and the new value is being computed.</div>
                <Running s={props.s} onStop={props.onStop} />
              </Show>
              <Switch>
                <Match when={res.kind === 'watch'}><Sum r={res} previous={props.s.previous} text={res.normal_form ?? ''} valueKey={props.s.valueKey} onFocus={props.onFocus && (() => props.onFocus!())} source={source()} trace={props.s.status === 'fresh' ? props.trace : undefined} /></Match>
                <Match when={res.kind === 'def'}><Sum r={res} text={`${res.name} := ${res.term}`} valueKey={props.s.valueKey} onFocus={props.onFocus} source={source()} /></Match>
                <Match when={res.kind === 'check'}><CheckView r={res} /></Match>
                <Match when={res.kind === 'solve'}><SolveView r={res} /></Match>
              </Switch>
              <Warnings value={res} />
              <div class="row">
                <ValueBadge value={res} />
                <Show when={props.s.ms !== undefined}><span class="hint">{props.s.ms! < 1 ? '<1' : Math.round(props.s.ms!)} ms</span></Show>
              </div>
            </div>
          );
        }}
      </Match>
    </Switch>
  );
}
