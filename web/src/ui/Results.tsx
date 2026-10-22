// How each kind of result is shown.
import { createEffect, createSignal, For, Match, on, Show, Switch } from 'solid-js';
import type { CellState, CellValue } from '../eval/evaluator';
import type { Provenance, Summand, WorkspaceRunCell } from '../engine/ops.gen';
import katex from 'katex';

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

/** The equality strategy and certificate behind a result; a warning colour when it rests on an assumption or a truncation. */
export function Badge(props: { p?: Provenance }) {
  const warn = () => !!props.p && (['assumed', 'unknown'].includes(props.p.certificate.status) || (props.p.notes?.length ?? 0) > 0);
  return (
    <Show when={props.p}>
      {(p) => (
        <span class="badge-wrap">
          <span class={`badge ${warn() ? 'warn' : ''}`} title={[p().method, p().certificate.reference, p().certificate.hypotheses].filter(Boolean).join('\n')}>
            {p().strategy} · {p().certificate.status}
          </span>
          <For each={p().notes ?? []}>{(n) => <span class="hint note-line">{n}</span>}</For>
        </span>
      )}
    </Show>
  );
}

function SummandView(props: { s: Summand; cls?: string; first: boolean; onFocus?: (word: string) => void }) {
  const lead = () => (props.s.negative ? '− ' : props.first ? '' : '+ ') + (props.s.coefficient ?? '');
  return (
    <span class={`summand ${props.cls ?? ''}`} title={props.onFocus ? `${props.s.text}\n(double-click to open the canvas)` : props.s.text} data-word={props.s.word}
      onDblClick={() => props.onFocus?.(props.s.word)}>
      <Show when={lead()}><span class="c">{lead()}</span></Show>
      <span innerHTML={props.s.svg} />
    </span>
  );
}

/** A sum of diagrams, outlining summands that are new since the previous value and showing the vanished ones briefly. */
export function Sum(props: { r: Ok; previous?: CellValue; text: string; valueKey?: string; onFocus?: (word: string) => void }) {
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
  return (
    <div class="result">
      <div class="sum" role="img" aria-label={props.text}>
        <Show when={terms() > 0} fallback={<span class="summand"><span class="c">0</span></span>}>
          <For each={props.r.summands}>{(s, i) => <SummandView s={s} first={i() === 0} cls={added().has(s.word) ? 'new' : ''} onFocus={props.onFocus} />}</For>
        </Show>
        <Show when={fading()}>
          <For each={gone()}>{(s) => <SummandView s={s} first={false} cls="gone" />}</For>
        </Show>
      </div>
      <Show when={shown() < terms()}>
        <div class="hint">Showing {shown()} of {terms()} terms; the text form below has all of them.</div>
      </Show>
      <Show when={props.previous && ok(props.previous) && (added().size || gone().length || recoef().length)}>
        <details class="changed">
          <summary>Changed: {added().size} new, {gone().length} gone, {recoef().length} with a new coefficient</summary>
          <div class="textform">
            {[...(props.r.summands ?? []).filter((s) => added().has(s.word)).map((s) => `+ ${s.text}`), ...gone().map((s) => `− ${s.text}`), ...recoef().map((s) => `~ ${s.text}`)].join('\n')}
          </div>
        </details>
      </Show>
      <div class="textform" data-testid="textform">{props.text}</div>
    </div>
  );
}

function CheckView(props: { r: Ok }) {
  return (
    <div class="result">
      <div class="row">
        <span class={`badge ${props.r.holds ? 'good' : 'bad'}`} data-testid="verdict">{props.r.holds ? 'holds' : 'fails'}</span>
      </div>
      <Show when={props.r.witness}>
        <div class="hint">Witness (should vanish):</div>
        <div class="textform">{props.r.witness}</div>
      </Show>
    </div>
  );
}

function assignments(m?: Record<string, string>) {
  return Object.entries(m ?? {}).map(([k, v]) => `${k} = ${v}`).join(', ');
}

function SolveView(props: { r: Ok }) {
  const r = () => props.r;
  return (
    <div class="result">
      <table class="kv">
        <tbody>
          <tr><td>outcome</td><td data-testid="outcome">{solveSummary(r())}</td></tr>
          <tr><td>method</td><td>{r().tier === 1 ? 'linear system' : 'Gröbner basis'}, {r().equations} equations</td></tr>
          <Show when={r().solution}><tr><td>solution</td><td class="mono">{assignments(r().solution)}</td></tr></Show>
          <Show when={r().particular}><tr><td>particular</td><td class="mono">{assignments(r().particular)}</td></tr></Show>
          <Show when={r().directions?.length}>
            <tr><td>directions</td><td class="mono">{r().directions!.map((d) => assignments(d)).join('\n')}</td></tr>
          </Show>
          <Show when={r().points?.length}>
            <tr><td>points</td><td class="mono" style={{ 'white-space': 'pre' }}>{r().points!.map((p) => `(${(r().variables ?? []).map((v, i) => `${v} = ${p[i]}`).join(', ')})`).join('\n')}</td></tr>
          </Show>
        </tbody>
      </table>
    </div>
  );
}

/** A note's text, with $…$ and $$…$$ as mathematics. */
export function NoteView(props: { text: string }) {
  const html = () =>
    props.text
      .split(/\n\s*\n/)
      .map((para) => `<p>${para.split(/(\$\$[^$]+\$\$|\$[^$]+\$)/).map((part) => {
        if (part.startsWith('$$')) return katex.renderToString(part.slice(2, -2), { displayMode: true, throwOnError: false });
        if (part.startsWith('$') && part.length > 1) return katex.renderToString(part.slice(1, -1), { throwOnError: false });
        return part.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
      }).join('')}</p>`)
      .join('');
  return <div class="note" innerHTML={html()} />;
}

export function ResultView(props: { s: CellState; onFocus?: (word?: string) => void }) {
  const v = () => props.s.value;
  return (
    <Switch>
      <Match when={props.s.parseError}><div class="error">{props.s.parseError}</div></Match>
      <Match when={props.s.parsed?.kind === 'note'}><NoteView text={props.s.parsed!.note ?? ''} /></Match>
      <Match when={props.s.status === 'running'}><div class="hint">Computing…</div></Match>
      <Match when={v()?.status === 'error'}><div class="error">{(v() as { message: string }).message}</div></Match>
      <Match when={v()?.status === 'exploded'}><div class="error">Stopped: {(v() as { message: string }).message}</div></Match>
      <Match when={ok(v()) && (v() as Ok)} keyed>
        {(res) => {
          return (
            <div class={props.s.status === 'stale' ? 'result stale' : 'result'} data-testid="result">
              <Show when={props.s.status === 'stale'}><div class="hint">Stale: the inputs changed and the new value is being computed.</div></Show>
              <Switch>
                <Match when={res.kind === 'watch'}><Sum r={res} previous={props.s.previous} text={res.normal_form ?? ''} valueKey={props.s.valueKey} onFocus={props.onFocus && (() => props.onFocus!())} /></Match>
                <Match when={res.kind === 'def'}><Sum r={res} text={`${res.name} := ${res.term}`} valueKey={props.s.valueKey} onFocus={props.onFocus} /></Match>
                <Match when={res.kind === 'check'}><CheckView r={res} /></Match>
                <Match when={res.kind === 'solve'}><SolveView r={res} /></Match>
              </Switch>
              <div class="row">
                <Badge p={res.provenance} />
                <Show when={props.s.ms !== undefined}><span class="hint">{props.s.ms! < 1 ? '<1' : Math.round(props.s.ms!)} ms</span></Show>
              </div>
            </div>
          );
        }}
      </Match>
    </Switch>
  );
}
