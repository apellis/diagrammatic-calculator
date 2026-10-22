// The outline: every cell by name with its status, the pinned snapshots, and the version history.
import { createSignal, For, Show } from 'solid-js';
import type { CellData, Snapshot, Version } from '../doc/model';
import type { CellState } from '../eval/evaluator';
import { summary } from './Results';
import { ago } from '../app';

export function label(c: CellData, s?: CellState): string {
  const p = s?.parsed;
  if (p?.kind === 'def') return `${p.defines} :=`;
  if (p?.kind === 'note') return (p.note ?? '').split('\n')[0] || 'note';
  if (p?.kind === 'check') return c.text.replace(/^check\s+/, '');
  return c.text.replace(/^(watch|solve)\s+/, '');
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
}) {
  const [tab, setTab] = createSignal<'cells' | 'snapshots' | 'history'>('cells');
  const [q, setQ] = createSignal('');
  const shown = () => props.cells.filter((c) => c.text.toLowerCase().includes(q().toLowerCase()));
  return (
    <nav class="outline" aria-label="Outline">
      <div class="tabs" role="tablist">
        <button classList={{ on: tab() === 'cells' }} onClick={() => setTab('cells')}>Cells</button>
        <button classList={{ on: tab() === 'snapshots' }} onClick={() => setTab('snapshots')}>Snapshots</button>
        <button classList={{ on: tab() === 'history' }} onClick={() => setTab('history')}>History</button>
      </div>
      <Show when={tab() === 'cells'}>
        <input placeholder="Search cells" value={q()} onInput={(e) => setQ(e.currentTarget.value)} aria-label="Search cells" />
        <For each={shown()}>
          {(c) => (
            <div class="oitem" classList={{ on: props.active === c.id }} onClick={() => props.onSelect(c.id)} title={c.text}>
              <span class={`dot ${props.state(c.id)?.status ?? ''}`} />
              <span class="t grow">{label(c, props.state(c.id))}</span>
              <span class="muted" style={{ 'font-size': '11.5px' }}>{props.state(c.id) ? summary(props.state(c.id)!) : ''}</span>
            </div>
          )}
        </For>
      </Show>
      <Show when={tab() === 'snapshots'}>
        <For each={props.snapshots} fallback={<div class="hint">Pin a result to freeze it here.</div>}>
          {(s, i) => (
            <div class="oitem" onClick={() => props.onSnapshot(i())}>
              <span class="grow">{s.name}</span>
              <span class="muted" style={{ 'font-size': '11.5px' }}>v{s.version}</span>
            </div>
          )}
        </For>
      </Show>
      <Show when={tab() === 'history'}>
        <For each={[...props.versions].reverse()} fallback={<div class="hint">No changes yet.</div>}>
          {(v) => (
            <div class="oitem" onClick={() => props.onVersion(v.n)} title={new Date(v.time).toLocaleString()}>
              <span class="muted" style={{ width: '3ch' }}>{v.n}</span>
              <span class="t grow">{v.note}</span>
              <span class="muted" style={{ 'font-size': '11px' }}>{ago(v.time)}</span>
            </div>
          )}
        </For>
      </Show>
    </nav>
  );
}
