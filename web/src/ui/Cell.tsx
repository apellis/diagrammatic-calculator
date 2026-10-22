// One cell: a one-line summary when collapsed; its editor, result and tools when expanded.
import { createSignal, For, Show } from 'solid-js';
import type { CellState } from '../eval/evaluator';
import { kindOf } from '../doc/model';
import { Editor } from './Editor';
import { ResultView, summary } from './Results';

export const TEMPLATES: [string, string, number?][] = [
  ['def', 'def a := ', undefined],
  ['watch', 'watch ', undefined],
  ['solve', 'solve  = 0 for c1, c2', 6],
  ['check', 'check idempotent()', 16],
  ['note', 'note """"""', 8],
];

export interface CellProps {
  id: string;
  text: string;
  state?: CellState;
  expanded: boolean;
  active: boolean;
  isNew?: boolean;
  onToggle: () => void;
  onCommit: (text: string) => void;
  onCancel: () => void;
  onRemove: () => void;
  onMove: (delta: number) => void;
  onPin: () => void;
  onRerun: () => void;
  onCopyTikz: () => void;
}

export function Cell(props: CellProps) {
  const [draft, setDraft] = createSignal(props.text);
  let editor: { set: (t: string, c?: number) => void; focus: () => void } | undefined;
  const kind = () => props.state?.parsed?.kind ?? kindOf(props.text) ?? (props.text ? 'def' : 'new');
  const head = () => {
    const p = props.state?.parsed;
    if (p?.kind === 'note') return (p.note ?? '').split('\n')[0];
    return props.text.replace(/^(def|watch|solve|check)\s+/, '');
  };
  const slash = () => draft().startsWith('/');
  const canPin = () => props.state?.status === 'fresh' && props.state.value?.status === 'ok';
  const canTikz = () => canPin() && ['watch', 'def'].includes((props.state!.value as { kind: string }).kind);

  return (
    <div class={`cell ${props.active ? 'active' : ''}`} id={`cell-${props.id}`} data-cell={props.id} data-kind={kind()} data-status={props.state?.status ?? ''}>
      <div class="head" onClick={props.onToggle}>
        <span class={`dot ${props.state?.status ?? ''}`} />
        <span class="kind">{kind()}</span>
        <span class="text grow">{head() || <span class="muted">empty cell</span>}</span>
        <span class="summary" data-testid="summary">{props.state ? summary(props.state) : ''}</span>
      </div>
      <Show when={props.expanded}>
        <div class="body">
          <Editor
            text={props.text}
            autofocus={props.active}
            placeholder="Type a term to define it, or / for a cell kind"
            onChange={setDraft}
            onCommit={props.onCommit}
            onCancel={props.onCancel}
            ref={(api) => (editor = api)}
          />
          <Show when={slash()}>
            <div class="kindmenu">
              <For each={TEMPLATES.filter(([k]) => k.startsWith(draft().slice(1).trim()))}>
                {([k, t, cursor]) => <button onMouseDown={(e) => { e.preventDefault(); editor?.set(t, cursor); }}>{k}</button>}
              </For>
            </div>
          </Show>
          <Show when={props.state && !props.isNew}>
            <ResultView s={props.state!} />
          </Show>
          <div class="tools">
            <button class="ghost" title="Move up" onClick={() => props.onMove(-1)}>↑</button>
            <button class="ghost" title="Move down" onClick={() => props.onMove(1)}>↓</button>
            <button class="ghost" onClick={props.onRerun} disabled={!props.state?.key}>Recompute</button>
            <button class="ghost" onClick={props.onPin} disabled={!canPin()}>Pin result</button>
            <button class="ghost" onClick={props.onCopyTikz} disabled={!canTikz()}>Copy TikZ</button>
            <div class="grow" />
            <span class="hint">{props.id}</span>
            <button class="ghost" onClick={props.onRemove}>Delete</button>
          </div>
        </div>
      </Show>
    </div>
  );
}
