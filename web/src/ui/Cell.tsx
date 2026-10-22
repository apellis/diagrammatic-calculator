// One cell: a one-line summary when collapsed; its text, result and tools when expanded. The text
// is shown typeset; clicking it (or Enter) opens it for editing in the fixed-width font.
import { createEffect, createSignal, For, on, Show } from 'solid-js';
import type { CellState } from '../eval/evaluator';
import { kindOf } from '../doc/model';
import { Editor } from './Editor';
import { NoteView, ResultView, summary } from './Results';
import { Typeset } from './Typeset';
import { toTex } from './tex';

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
  /** Opens the focus canvas, at a summand when one was double-clicked. */
  onCanvas?: (word?: string) => void;
}

export function Cell(props: CellProps) {
  const [draft, setDraft] = createSignal(props.text);
  let editor: { set: (t: string, c?: number) => void; focus: () => void } | undefined;
  const kind = () => props.state?.parsed?.kind ?? kindOf(props.text) ?? (props.text ? 'def' : 'new');
  const head = () => {
    const p = props.state?.parsed;
    if (p?.kind === 'note') return (p.note ?? '').split('\n')[0];
    if (kind() === 'comment') return props.text.replace(/^\s*#\s?/, '');
    return props.text.replace(/^(def|watch|solve|check)\s+/, '');
  };
  const slash = () => draft().startsWith('/');
  // editing when new or opened by a click; typeset otherwise, and again once the cell is left
  const [editing, setEditing] = createSignal(!!props.isNew);
  createEffect(on(() => props.active, (a) => !a && setEditing(!!props.isNew), { defer: true }));
  const edit = () => {
    setEditing(true);
    queueMicrotask(() => editor?.focus());
  };
  const canPin = () => props.state?.status === 'fresh' && props.state.value?.status === 'ok';
  const canTikz = () => canPin() && ['watch', 'def'].includes((props.state!.value as { kind: string }).kind);

  return (
    <div class={`cell ${props.active ? 'active' : ''}`} id={`cell-${props.id}`} data-cell={props.id} data-kind={kind()} data-status={props.state?.status ?? ''}>
      <div class="head" onClick={() => {
        if (!props.expanded) setEditing(true);
        props.onToggle();
      }}>
        <span class={`dot ${props.state?.status ?? ''}`} />
        <span class="kind">{kind()}</span>
        <span class="text grow" data-text={head()}>
          <Show when={head()} fallback={<span class="muted">empty cell</span>}>
            <Show when={kind() === 'note' || kind() === 'comment'} fallback={<Typeset text={head()} />}>
              <NoteView text={head()} inline />
            </Show>
          </Show>
        </span>
        <span class="summary" data-testid="summary">{props.state ? summary(props.state) : ''}</span>
      </div>
      <Show when={props.expanded}>
        <div class="body">
          <Show when={editing()} fallback={<CellText text={props.text} kind={kind()} onEdit={edit} />}>
            <Editor
              text={props.text}
              autofocus={props.active || editing()}
              placeholder="Type a term to define it, or / for a cell kind"
              onChange={setDraft}
              onCommit={(t) => {
                if (!props.isNew) setEditing(false);
                props.onCommit(t);
              }}
              onCancel={() => {
                if (!props.isNew) setEditing(false);
                props.onCancel();
              }}
              onLeave={() => !props.isNew && setEditing(false)}
              ref={(api) => (editor = api)}
            />
          </Show>
          <Show when={slash()}>
            <div class="kindmenu">
              <For each={TEMPLATES.filter(([k]) => k.startsWith(draft().slice(1).trim()))}>
                {([k, t, cursor]) => <button onMouseDown={(e) => { e.preventDefault(); editor?.set(t, cursor); }}>{k}</button>}
              </For>
            </div>
          </Show>
          <Show when={props.state && !props.isNew && ((kind() !== 'note' && kind() !== 'comment') || editing())}>
            <ResultView s={props.state!} onFocus={props.onCanvas} />
          </Show>
          <div class="tools">
            <button class="ghost" title="Move up" onClick={() => props.onMove(-1)}>↑</button>
            <button class="ghost" title="Move down" onClick={() => props.onMove(1)}>↓</button>
            <button class="ghost" onClick={props.onRerun} disabled={!props.state?.key}>Recompute</button>
            <button class="ghost" onClick={props.onPin} disabled={!canPin()}>Pin result</button>
            <button class="ghost" onClick={props.onCopyTikz} disabled={!canTikz()}>Copy TikZ</button>
            <Show when={props.onCanvas && ['def', 'watch'].includes(kind())}>
              <button class="ghost" onClick={() => props.onCanvas?.()} title="Open the focus canvas (F)" data-testid="open-canvas">Canvas</button>
            </Show>
            <div class="grow" />
            <span class="hint">{props.id}</span>
            <button class="ghost" onClick={props.onRemove}>Delete</button>
          </div>
        </div>
      </Show>
    </div>
  );
}

/** A cell's text typeset (its kind is in the head above); options on lines of their own. */
function CellText(props: { text: string; kind: string; onEdit: () => void }) {
  const lines = () => props.text.split('\n');
  const first = () => lines()[0].replace(/^(def|watch|solve|check)\s+/, '');
  const options = () => lines().slice(1).map((l) => l.trim()).filter(Boolean);
  const note = () => /^note\s*"""([\s\S]*?)("""\s*)?$/.exec(props.text.trim())?.[1] ?? '';
  return (
    <div class="celltext" data-testid="cell-display" data-text={props.text} role="button" tabindex={0} title="Click to edit"
      onClick={(e) => (e.target as HTMLElement).closest('a') || props.onEdit()} onKeyDown={(e) => e.key === 'Enter' && (e.preventDefault(), props.onEdit())}>
      <Show when={props.kind === 'note' || props.kind === 'comment'} fallback={
        <Show when={props.text.trim()} fallback={<span class="muted">empty cell</span>}>
          <Typeset text={first()} display />
          <For each={options()}>{(o) => <div class="opt"><Typeset text={o} tex={toTex(o.replace(/=/, ' = '))} /></div>}</For>
        </Show>
      }>
        <NoteView text={props.kind === 'comment' ? props.text.replace(/^\s*#\s?/, '') : note()} />
      </Show>
    </div>
  );
}
