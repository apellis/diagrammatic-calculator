// One cell: a one-line summary when collapsed; its text, result, and tools when expanded. The text
// is shown typeset; clicking it (or Enter) opens it for editing in the fixed-width font.
import { createEffect, createSignal, For, on, onCleanup, Show } from 'solid-js';
import type { CellState } from '../eval/evaluator';
import { kindOf } from '../doc/model';
import { Editor, type RemoteCursor } from './Editor';
import { NoteView, ResultView, Summary, summary, type Tracer } from './Results';
import { Typeset } from './Typeset';
import { toTex } from './tex';
import type { CellComment } from './AIPanel';

export const TEMPLATES: [string, string, number?][] = [
  ['def', 'def a := ', undefined],
  ['watch', 'watch ', undefined],
  ['solve', 'solve  = 0 for c1, c2', 6],
  ['check', 'check idempotent()', 17],
  ['note', 'note """"""', 8],
];

export interface CellProps {
  id: string;
  text: string;
  state?: CellState;
  expanded: boolean;
  active: boolean;
  isNew?: boolean;
  readOnly?: boolean;
  pinDisabled?: boolean;
  remoteCursors?: RemoteCursor[];
  onSelection?: (selection: { anchor: number; head: number } | null) => void;
  /** Private draft, never sent to the shared document. Null clears recovery state. */
  onDraft?: (text: string | null) => void;
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
  /** Why a text cannot be a cell (it does not parse), or null when it can. */
  validate?: (text: string) => Promise<string | null>;
  /** A new cell's starting text, with the cursor position in it (a template of a cell kind). */
  initial?: { text: string; cursor?: number };
  /** Opens a new cell below this one (Alt+Enter on the head). */
  onInsertBelow?: () => void;
  /** Stops this cell's computation. */
  onStop?: () => void;
  /** Computes the cell again with a longer time limit, after it was stopped at the limit. */
  onRunLonger?: () => void;
  /** The cell's workspace for the command-line tool, after it was stopped at the limit. */
  commandLine?: () => string | undefined;
  /** Traces a summand of a watch back to the terms of the definitions. */
  trace?: Tracer;
  /** The person's comments on this cell for the AI, and making one. */
  comments?: CellComment[];
  onComment?: (text: string) => void;
}

/** Moves the focus from a cell's head to the next or previous cell's head. */
function moveFocus(from: HTMLElement, delta: number) {
  const heads = [...document.querySelectorAll<HTMLElement>('.cells .cell[data-cell]:not([data-cell="new"]) > .head')];
  const i = heads.indexOf(from);
  heads[i + delta]?.focus();
}

export function Cell(props: CellProps) {
  const [draft, setDraft] = createSignal(props.text);
  const [commenting, setCommenting] = createSignal(false);
  const [commentDraft, setCommentDraft] = createSignal('');
  let commentBox: HTMLTextAreaElement | undefined;
  createEffect(on(commenting, (on) => on && queueMicrotask(() => commentBox?.focus())));
  const sendComment = () => {
    const t = commentDraft().trim();
    if (!t) return;
    props.onComment?.(t);
    setCommentDraft('');
    setCommenting(false);
  };
  let editor: { set: (t: string, c?: number, focus?: boolean) => void; focus: () => void } | undefined;
  let host!: HTMLDivElement;
  const restoreFocus = () => queueMicrotask(() => {
    if (document.activeElement === document.body) (host.querySelector<HTMLElement>('[data-testid="cell-display"]') ?? host.querySelector<HTMLElement>('.head'))?.focus();
  });
  let base = props.text;
  const [conflict, setConflict] = createSignal(false);
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
  createEffect(on(() => props.active, (a) => {
    if (!a && !conflict() && draft() === base) setEditing(!!props.isNew);
  }, { defer: true }));
  createEffect(on(() => props.text, (text) => {
    if (text === base) return;
    if (draft() !== base && draft() !== text) {
      setConflict(true);
      props.onDraft?.(draft());
      return;
    }
    base = text;
    setDraft(text);
    setConflict(false);
    props.onDraft?.(null);
    if (editing()) editor?.set(text, undefined, false);
  }, { defer: true }));
  const edit = () => {
    if (props.readOnly) return;
    setEditing(true);
    queueMicrotask(() => editor?.focus());
  };
  // the text is checked as it is typed (after a pause) and again on Enter: text that does not
  // parse is never written to the document; the editor stays open with the reason under it
  const [problem, setProblem] = createSignal('');
  let checks = 0;
  let revision = 0;
  let pause: ReturnType<typeof setTimeout> | undefined;
  const check = async (t: string) => {
    const n = ++checks;
    const why = props.validate && t.trim() && !t.trim().startsWith('/') ? await props.validate(t) : null;
    if (n === checks) setProblem(why ?? '');
    return why;
  };
  onCleanup(() => { clearTimeout(pause); revision++; checks++; });
  const changed = (t: string) => {
    revision++;
    checks++;
    setDraft(t);
    props.onDraft?.(t === props.text ? null : t);
    clearTimeout(pause);
    if (problem()) void check(t);
    else pause = setTimeout(() => void check(t), 400);
  };
  const commit = async (t: string, resolve = false) => {
    if (props.readOnly || (conflict() && !resolve)) return;
    clearTimeout(pause);
    const observed = props.text;
    const attempt = ++revision;
    const why = await check(t);
    // A cancelled or superseded edit must not commit (or steal focus) after validation.
    if (attempt !== revision) return;
    if (why) return;
    // A remote commit while validation runs is a new conflict.
    if (props.readOnly || observed !== props.text || (conflict() && !resolve)) return;
    const ownedFocus = host.contains(document.activeElement);
    base = t;
    setDraft(t);
    setConflict(false);
    if (!props.isNew) setEditing(false);
    props.onCommit(t);
    props.onDraft?.(null);
    if (ownedFocus) restoreFocus();
  };
  const takeTheirs = () => {
    revision++;
    checks++;
    clearTimeout(pause);
    base = props.text;
    setDraft(base);
    setConflict(false);
    setProblem('');
    props.onDraft?.(null);
    editor?.set(base, undefined, false);
  };
  const canPin = () => props.state?.status === 'fresh' && props.state.value?.status === 'ok';
  const canTikz = () => canPin() && ['watch', 'def'].includes((props.state!.value as { kind: string }).kind);

  return (
    <div ref={host} class={`cell ${props.active ? 'active' : ''}`} id={`cell-${props.id}`} data-cell={props.id} data-kind={kind()} data-status={props.state?.status ?? ''}>
      {/* the head is the cell's keyboard stop: Enter or Space opens or closes it, ↑ and ↓ move
          between cells, Alt+Enter inserts a cell below */}
      <div class="head" role="button" tabindex={0} aria-expanded={props.expanded} aria-controls={`body-${props.id}`}
        aria-label={`${kind()} cell ${props.id}: ${head() || 'empty'}${props.state ? `, ${summary(props.state)}` : ''}`}
        onClick={() => {
          if (!props.expanded && !props.readOnly) setEditing(true);
          props.onToggle();
        }}
        onKeyDown={(e) => {
          if (e.target !== e.currentTarget) return;
          if ((e.key === 'Enter' || e.key === ' ') && !e.altKey && !e.metaKey && !e.ctrlKey) {
            e.preventDefault();
            props.onToggle();
          } else if (e.key === 'Enter' && e.altKey) {
            e.preventDefault();
            e.stopPropagation();
            props.onInsertBelow?.();
          } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            moveFocus(e.currentTarget, e.key === 'ArrowDown' ? 1 : -1);
          }
        }}>
        <span aria-hidden="true" data-testid="cell-disclosure" style={{ width: '1em', "flex-shrink": 0 }}>{props.expanded ? '▾' : '▸'}</span>
        <span class={`dot ${props.state?.status ?? ''}`} />
        <span class="kind">{kind()}</span>
        {/* expanded, the text is shown once, below, where it is edited */}
        <span class="text grow" data-text={head()}>
          <Show when={!props.expanded}>
          <Show when={head()} fallback={<span class="muted">empty cell</span>}>
            <Show when={kind() === 'note' || kind() === 'comment'} fallback={<Typeset text={head()} />}>
              <NoteView text={head()} inline />
            </Show>
          </Show>
          </Show>
        </span>
        <Summary state={props.state} />
      </div>
      <Show when={props.expanded}>
        <div class="body" id={`body-${props.id}`}>
          <Show when={editing() && !props.readOnly} fallback={<CellText text={props.text} kind={kind()} onEdit={edit} />}>
            <Editor
              text={draft()} remoteCursors={draft() === props.text ? props.remoteCursors : []}
              onSelection={(selection) => props.onSelection?.(draft() === props.text ? selection : null)}
              autofocus={props.active || editing()}
              placeholder="Type a term to define it, or / for a cell kind"
              onChange={changed}
              invalid={!!problem()} errorId={`problem-${props.id}`}
              onCommit={(t) => void commit(t)}
              onCancel={() => {
                takeTheirs();
                setProblem('');
                if (!props.isNew) setEditing(false);
                props.onCancel();
                restoreFocus();
              }}
              onLeave={() => !props.isNew && !problem() && draft() === props.text && setEditing(false)}
              ref={(api) => {
                editor = api;
                const t = props.initial;
                if (t) queueMicrotask(() => api.set(t.text, t.cursor));
              }}
            />
          </Show>
          <Show when={conflict()}>
            <div class="banner draft-conflict" data-testid="draft-conflict" role="status">
              This cell changed elsewhere. Your draft has not been shared.
              <pre>{props.text}</pre>
              <button disabled={props.readOnly} onClick={() => void commit(draft(), true)}>Keep mine</button>
              <button onClick={takeTheirs}>Take theirs</button>
            </div>
          </Show>
          <Show when={editing() && problem()}>
            <div class="error small" data-testid="cell-problem" id={`problem-${props.id}`} role="status">{problem()}. Fix it, or press Esc to {props.isNew ? 'discard it' : 'keep the previous text'}.</div>
          </Show>
          <Show when={slash()}>
            <div class="kindmenu">
              <For each={TEMPLATES.filter(([k]) => k.startsWith(draft().slice(1).trim()))}>
                {([k, t, cursor]) => <button onMouseDown={(e) => { e.preventDefault(); editor?.set(t, cursor); }}>{k}</button>}
              </For>
            </div>
          </Show>
          <Show when={props.state && !props.isNew && ((kind() !== 'note' && kind() !== 'comment') || editing())}>
            <ResultView s={props.state!} onFocus={props.onCanvas} onStop={props.onStop} onRunLonger={props.onRunLonger} commandLine={props.commandLine} trace={props.trace} />
          </Show>
          {/* New cells have no saved result or position yet. */}
          <Show when={!props.isNew} fallback={
            <div class="tools"><div class="grow" /><button class="ghost" onClick={props.onRemove}>Discard (Esc)</button></div>
          }>
            <div class="tools">
              <button class="ghost" title="Move up" aria-label="Move the cell up" disabled={props.readOnly} onClick={() => props.onMove(-1)}>↑</button>
              <button class="ghost" title="Move down" aria-label="Move the cell down" disabled={props.readOnly} onClick={() => props.onMove(1)}>↓</button>
              <Show when={props.state?.status === 'failed' && props.state.value?.status === 'error' && 'message' in props.state.value && props.state.value.message === 'cancelled'}>
                <button class="ghost" onClick={props.onRerun}>Retry</button>
              </Show>
              <button class="ghost" onClick={props.onPin} disabled={props.readOnly || props.pinDisabled || !canPin()}>Pin result</button>
              <button class="ghost" onClick={props.onCopyTikz} disabled={!canTikz()}>Copy TikZ</button>
              <Show when={props.onCanvas && ['def', 'watch'].includes(kind())}>
                <button class="ghost" onClick={() => props.onCanvas?.()} title="Open the focus canvas (F)" data-testid="open-canvas">Canvas</button>
              </Show>
              <Show when={props.onComment}>
                <button class="ghost" onClick={() => setCommenting(!commenting())} aria-expanded={commenting()} title="Comment on this cell for the AI: an instruction it gets at once"
                  aria-label="Tell the AI: comment on this cell" data-testid="comment-ai">Tell the AI</button>
              </Show>
              <div class="grow" />
              <span class="hint">{props.id}</span>
              <button class="ghost" disabled={props.readOnly} onClick={props.onRemove}>Delete</button>
            </div>
            <Show when={props.comments?.length}>
              <ul class="cell-comments" data-testid="cell-comments">
                <For each={props.comments}>
                  {(c) => (
                    <li>
                      <span class="hint">For the AI ({c.state === 'waiting' ? 'waiting' : 'sent'}):</span> {c.text}
                    </li>
                  )}
                </For>
              </ul>
            </Show>
            <Show when={commenting()}>
              <div class="comment-box">
                <textarea ref={commentBox} rows={2} aria-label={`Comment on ${props.id} for the AI`} value={commentDraft()} onInput={(e) => setCommentDraft(e.currentTarget.value)}
                  placeholder="An instruction to the AI about this cell; Enter sends."
                  onKeyDown={(e) => {
                    // Enter and Esc belong to this box, not to the cell around it
                    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
                      e.preventDefault();
                      e.stopPropagation();
                      sendComment();
                    } else if (e.key === 'Escape') {
                      e.stopPropagation();
                      setCommenting(false);
                    }
                  }} />
                <div class="row">
                  <button class="primary" disabled={!commentDraft().trim()} onClick={sendComment} data-testid="comment-send">Send to the AI</button>
                  <button class="ghost" onClick={() => setCommenting(false)}>Cancel</button>
                  <span class="hint">It goes to the AI as your instruction about {props.id}, between its steps if it is busy.</span>
                </div>
              </div>
            </Show>
          </Show>
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
      onClick={(e) => (e.target as HTMLElement).closest('a') || props.onEdit()} onKeyDown={(e) => e.target === e.currentTarget && (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), props.onEdit())}>
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
