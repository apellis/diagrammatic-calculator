// The visible entry point for a cell. Drafts stay local until Add cell or Start drawing.
import { createEffect, createSignal, For, on, onCleanup, Show, type JSX } from 'solid-js';
import { kindOf } from '../doc/model';
import { Editor } from './Editor';
import { TEMPLATES } from './Cell';
import { Typeset } from './Typeset';
import './new-cell.css';

const TYPES = [
  ['def', 'Element', 'Name an element to use in other cells.'],
  ['watch', 'Watch', 'Simplify an expression; it updates when its definitions change.'],
  ['check', 'Check', 'Test an equality or property, such as idempotency.'],
  ['solve', 'Solve', 'Find scalar coefficients that satisfy an equation.'],
  ['note', 'Note', 'Write a note with Markdown and mathematics.'],
] as const;

export function NewCell(props: {
  context: string;
  position: number;
  name: string;
  initial?: { text: string; cursor?: number };
  validate: (text: string) => Promise<string | null>;
  onCommit: (text: string, draw: boolean, restoreFocus: boolean) => void;
  onCancel: () => void;
  /** Whether the composer holds text of the person's that closing it would lose. */
  onDirty?: (dirty: boolean) => void;
  drawing?: (onStart: (term: string) => void, onChange: () => void) => JSX.Element;
}) {
  const [draft, setDraft] = createSignal(props.initial?.text ?? '');
  const [kind, setKind] = createSignal<string>(kindOf(draft()) ?? 'def');
  const [name, setName] = createSignal(props.name);
  const [mode, setMode] = createSignal<'text' | 'draw'>('text');
  const [drawingOpened, setDrawingOpened] = createSignal(false);
  const [problem, setProblem] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const drafts = new Map<string, string>();
  const elementDraft = () => kind() === 'def' ? draft() : drafts.get('def') ?? '';
  let editor: { set: (t: string, c?: number) => void; focus: () => void } | undefined;
  let host!: HTMLElement;
  let incompleteCheck = props.initial?.text === 'check idempotent()';
  let revision = 0;
  const invalidate = () => { revision++; setProblem(''); setBusy(false); };
  onCleanup(() => { revision++; props.onDirty?.(false); });
  createEffect(on(() => [props.context, props.position] as const, invalidate, { defer: true }));
  const changed = (text: string) => {
    invalidate();
    props.onDirty?.(text.trim() !== (props.initial?.text ?? '').trim());
    if (kind() === 'check' && text !== 'check idempotent()') incompleteCheck = false;
    setDraft(text);
    const k = kindOf(text);
    if (k && TYPES.some(([key]) => key === k)) setKind(k);
  };
  const choose = (k: string) => {
    if (k === kind()) return editor?.focus();
    drafts.set(kind(), draft());
    invalidate();
    setKind(k);
    setMode('text');
    const template = TEMPLATES.find(([key]) => key === k);
    const text = drafts.get(k) ?? (k === 'def' ? '' : template?.[1] ?? '');
    if (k === 'check' && !drafts.has(k)) incompleteCheck = true;
    setDraft(text);
    queueMicrotask(() => editor?.set(text, drafts.has(k) ? undefined : template?.[2]));
  };
  const cancel = () => { invalidate(); props.onCancel(); };
  const asCell = (text: string) => kindOf(text.trim()) ? text : `def ${name().trim()} := ${text}`;
  const submit = async (text: string, draw = false) => {
    if (busy()) return;
    if (!text.trim()) return cancel();
    if (text.startsWith('/')) return;
    if (incompleteCheck && text === 'check idempotent()') {
      setProblem('Add an element inside idempotent(...), for example idempotent(a) if a is defined above.');
      return;
    }
    const source = asCell(text);
    if (draw && kindOf(source) !== 'def') return setProblem('Choose Element to edit a drawing. Watches only display results.');
    const attempt = ++revision;
    const context = props.context;
    const ownedFocus = host.contains(document.activeElement);
    setBusy(true);
    try {
      const why = await props.validate(source);
      if (revision !== attempt || context !== props.context) return;
      setBusy(false);
      if (why) return setProblem(why);
      props.onCommit(source, draw, ownedFocus && (host.contains(document.activeElement) || document.activeElement === document.body));
    } catch (e) {
      if (revision !== attempt) return;
      setBusy(false);
      setProblem(e instanceof Error ? e.message : 'Could not validate this cell. Try again.');
    }
  };
  const switchMode = (m: 'text' | 'draw') => { invalidate(); if (m === 'draw') setDrawingOpened(true); setMode(m); };
  return (
    <section ref={host} class="cell active new-cell" data-cell="new" aria-label="New cell"
      onKeyDown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); cancel(); } }}>
      <div class="new-cell-heading"><strong>New cell</strong><button class="ghost" onClick={cancel} aria-label="Discard new cell" title="Discard (Esc)">×</button></div>
      <div class="new-cell-types" role="group" aria-label="Cell type">
        <For each={TYPES}>{([k, title, hint]) => <button aria-pressed={kind() === k} title={hint} onClick={() => choose(k)}>{title}</button>}</For>
      </div>
      <p class="hint new-cell-description">{TYPES.find(([k]) => k === kind())?.[2]}</p>
      <div class="body">
        <Show when={kind() === 'def'}>
          <div class="new-cell-entry">
            <div class="entry-modes" role="group" aria-label="Create element with">
              <button aria-pressed={mode() === 'text'} onClick={() => switchMode('text')}>Type</button>
              <button aria-pressed={mode() === 'draw'} onClick={() => switchMode('draw')}>Draw</button>
            </div>
            <Show when={!kindOf(draft())}>
              <label class="element-name">Name <input aria-label="Element name" value={name()} onInput={(e) => { invalidate(); setName(e.currentTarget.value); }} /></label>
            </Show>
          </div>
        </Show>
        <Show when={drawingOpened()}>
          <div hidden={mode() !== 'draw'}>
            <Show when={elementDraft().trim()} fallback={props.drawing?.((term) => void submit(term, true), invalidate)}>
              <div class="draw-from-text"><Typeset text={elementDraft()} /><button class="primary" disabled={busy()} onClick={() => void submit(elementDraft(), true)}>Open on canvas</button></div>
            </Show>
          </div>
        </Show>
        <Show when={mode() === 'text'}>
          <Editor text={draft()} autofocus commitOnBlur={false} placeholder="Enter an expression, or paste a cell command"
            ref={(api) => { editor = api; if (props.initial) queueMicrotask(() => api.set(draft(), props.initial?.cursor)); }}
            invalid={!!problem()} errorId="new-cell-problem" onChange={changed} onCommit={(t) => void submit(t)} onCancel={cancel} />
          <Show when={draft().startsWith('/')}>
            <div class="kindmenu">
              <For each={TEMPLATES.filter(([k]) => k.startsWith(draft().slice(1).trim()))}>
                {([k, text, cursor]) => <button onClick={() => { setKind(k); incompleteCheck = k === 'check'; editor?.set(text, cursor); }}>{k}</button>}
              </For>
            </div>
          </Show>
          <div class="new-cell-actions">
            <button class="primary" disabled={busy() || !draft().trim()} onClick={() => void submit(draft())}>Add cell</button>
            <span class="hint">{kind() === 'note' ? 'Ctrl / ⌘ + Enter' : 'Enter'} to add</span>
          </div>
        </Show>
        <Show when={problem()}><div class="error" id="new-cell-problem" data-testid="cell-problem" role="status">{problem()} Edit to fix, or Esc to discard.</div></Show>
      </div>
    </section>
  );
}
