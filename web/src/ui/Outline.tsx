// The outline: every cell by name with its status, the pinned snapshots, and the version history.
import { createSignal, For, Show } from 'solid-js';
import { kindOf, type CellData, type Snapshot, type Version } from '../doc/model';
import type { CellState } from '../eval/evaluator';
import { NoteView, Summary, summary } from './Results';
import { Typeset } from './Typeset';
import { ContextText } from './ContextChip';
import { ago } from '../app';

export function label(c: CellData, s?: CellState): string {
  const p = s?.parsed;
  if (p?.kind === 'def') return `${p.defines} := ${p.term ?? ''}`.trimEnd();
  if (p?.kind === 'note') return (p.note ?? '').split('\n')[0] || 'note';
  if (p?.kind === 'check') return c.text.replace(/^check\s+/, '');
  if (kindOf(c.text) === 'comment') return c.text.replace(/^\s*#\s?/, '');
  return c.text.replace(/^(watch|solve)\s+/, '');
}

/** A cell's label typeset; a note's first line as written. */
function Label(props: { c: CellData; s?: CellState }) {
  return (
    <Show when={props.s?.parsed?.kind === 'note' || kindOf(props.c.text) === 'comment'} fallback={<Typeset text={label(props.c, props.s)} />}>
      <NoteView text={label(props.c, props.s)} inline />
    </Show>
  );
}

export function Outline(props: {
  cells: CellData[];
  state: (id: string) => CellState | undefined;
  active: string | null;
  versions: Version[];
  snapshots: Snapshot[];
  onSelect: (id: string) => void;
  onVersion: (n: number) => void;
  onSnapshot: (i: number) => void;
  authorLabel?: (id?: string, kind?: 'person' | 'ai') => string;
  /** Watches pinned as measures of progress, and the term counts each has had. */
  pinned?: { id: string; trend: number[] }[];
}) {
  const [tab, setTab] = createSignal<'cells' | 'snapshots' | 'history'>('cells');
  const [q, setQ] = createSignal('');
  const shown = () => props.cells.filter((c) => c.text.toLowerCase().includes(q().toLowerCase()));
  return (
    <nav class="outline" aria-label="Outline">
      <div class="tabs" role="tablist" aria-label="Outline">
        <For each={TABS}>
          {([t, title]) => (
            <button role="tab" id={`otab-${t}`} aria-selected={tab() === t} aria-controls="opanel" tabindex={tab() === t ? 0 : -1}
              classList={{ on: tab() === t }} onClick={() => setTab(t)}
              onKeyDown={(e) => {
                // arrows move between the tabs, as in any tab list
                const i = TABS.findIndex(([x]) => x === t);
                const d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
                if (!d) return;
                e.preventDefault();
                const [next] = TABS[(i + d + TABS.length) % TABS.length];
                setTab(next);
                document.getElementById(`otab-${next}`)?.focus();
              }}>
              {title}
            </button>
          )}
        </For>
      </div>
      <div id="opanel" role="tabpanel" aria-labelledby={`otab-${tab()}`}>
      <Show when={tab() === 'cells'}>
        <Show when={props.pinned?.length}>
          <div class="pinned" data-testid="pinned">
            <For each={props.pinned}>
              {(p) => {
                const c = () => props.cells.find((x) => x.id === p.id);
                return (
                  <Show when={c()}>
                    <button class="oitem" onClick={() => props.onSelect(p.id)} title={`${c()!.text}\nterm counts: ${p.trend.join(', ')}`}
                      aria-label={`${label(c()!, props.state(p.id))}${props.state(p.id) ? `, ${summary(props.state(p.id)!)}` : ''}`}>
                      <span class={`dot ${props.state(p.id)?.status ?? ''}`} />
                      <span class="t grow" data-text={label(c()!, props.state(p.id))}><Label c={c()!} s={props.state(p.id)} /></span>
                      <Trend values={p.trend} />
                      <Summary state={props.state(p.id)} small />
                    </button>
                  </Show>
                );
              }}
            </For>
          </div>
        </Show>
        <input placeholder="Search cells" value={q()} onInput={(e) => setQ(e.currentTarget.value)} aria-label="Search cells" />
        <For each={shown()}>
          {(c) => (
            <button class="oitem" classList={{ on: props.active === c.id }} aria-current={props.active === c.id ? 'true' : undefined}
              onClick={() => props.onSelect(c.id)} title={c.text}
              aria-label={`${label(c, props.state(c.id))}${props.state(c.id) ? `, ${summary(props.state(c.id)!)}` : ''}`}>
              <span class={`dot ${props.state(c.id)?.status ?? ''}`} />
              <span class="t grow" data-text={label(c, props.state(c.id))}><Label c={c} s={props.state(c.id)} /></span>
              <Summary state={props.state(c.id)} small />
            </button>
          )}
        </For>
      </Show>
      <Show when={tab() === 'snapshots'}>
        <For each={props.snapshots} fallback={<div class="hint">Pin a result to freeze it here.</div>}>
          {(s, i) => (
            <button class="oitem" onClick={() => props.onSnapshot(i())}>
              <span class="grow">{s.name}</span>
              <span class="muted" style={{ 'font-size': '11.5px' }}>v{s.version}</span>
            </button>
          )}
        </For>
      </Show>
      <Show when={tab() === 'history'}>
        <For each={[...props.versions].reverse()} fallback={<div class="hint">No changes yet.</div>}>
          {(v) => (
            <button class="oitem" onClick={() => props.onVersion(v.n)} title={new Date(v.time).toLocaleString()} data-note={v.note}
              aria-label={`Version ${v.n}: ${v.note}, ${ago(v.time)}`}>
              <span class="muted" style={{ width: '3ch', flex: 'none' }}>{v.n}</span>
              <span class="t grow"><span class="hint">{props.authorLabel?.(v.author, v.authorKind) ?? v.author}: </span><HistoryNote note={v.note} /></span>
              <span class="muted" style={{ 'font-size': '11px' }}>{ago(v.time)}</span>
            </button>
          )}
        </For>
      </Show>
      </div>
    </nav>
  );
}

const TABS: ['cells' | 'snapshots' | 'history', string][] = [
  ['cells', 'Cells'],
  ['snapshots', 'Snapshots'],
  ['history', 'History'],
];

/**
 * A version's note: the words as written, a cell's text after them typeset ("add def e" and the
 * term). Notes are written by the app as "<verb> <cell text>", so the cell text is what follows the
 * first word and an optional cell kind.
 */
function HistoryNote(props: { note: string }) {
  const m = () => /^(add|edit|remove|update) (def \S+ :=|def|watch|solve|check|note|#) ?(.*)$/s.exec(props.note);
  const ctx = () => /^context (\S.*)$/.exec(props.note)?.[1];
  return (
    <Show when={m() && m()![3] && m()![2] !== 'note' && m()![2] !== '#'} fallback={
      <Show when={ctx()} fallback={<>{props.note}</>}>{(c) => <>context <ContextText context={c()} /></>}</Show>
    }>
      <Show when={/^def \S+ :=$/.test(m()![2])} fallback={<>{m()![1]} {m()![2]} <Typeset text={m()![3]} /></>}>
        <>{m()![1]} def <Typeset text={`${m()![2].slice(4)} ${m()![3]}`} /></>
      </Show>
    </Show>
  );
}

/** A small line of a watch's term counts over time. */
function Trend(props: { values: number[] }) {
  const pts = () => {
    const v = props.values.slice(-20);
    const max = Math.max(1, ...v);
    return v.map((x, i) => `${(i / Math.max(1, v.length - 1)) * 40},${12 - (x / max) * 11}`).join(' ');
  };
  return (
    <Show when={props.values.length > 1}>
      <svg width="40" height="13" viewBox="-1 0 42 13" aria-hidden="true"><polyline points={pts()} fill="none" stroke="var(--accent)" stroke-width="1.2" /></svg>
    </Show>
  );
}
