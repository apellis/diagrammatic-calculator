// A text cell's editor: CodeMirror with one line per cell (notes may span lines).
import { createEffect, onCleanup, onMount } from 'solid-js';
import { EditorState, StateEffect, StateField } from '@codemirror/state';
import { Decoration, EditorView, WidgetType, keymap, placeholder as cmPlaceholder } from '@codemirror/view';
import type { DecorationSet } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';

export interface RemoteCursor { anchor: number; head: number; name: string; colour: string }
const remotePositions = StateEffect.define<RemoteCursor[]>();
class CollaboratorCaret extends WidgetType {
  constructor(readonly name: string, readonly colour: string) { super(); }
  eq(other: CollaboratorCaret) { return this.name === other.name && this.colour === other.colour; }
  toDOM() {
    const caret = document.createElement('span'); caret.className = 'remote-caret';
    caret.style.borderColor = this.colour; caret.setAttribute('aria-label', `${this.name}'s cursor`);
    const label = document.createElement('span'); label.textContent = this.name; label.style.backgroundColor = this.colour;
    caret.append(label); return caret;
  }
}
const remoteDecoration = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, transaction) {
    let result = value.map(transaction.changes);
    for (const effect of transaction.effects) if (effect.is(remotePositions)) {
      const ranges = effect.value.flatMap((c) => {
        if (!Number.isInteger(c.anchor) || !Number.isInteger(c.head) || Math.min(c.anchor, c.head) < 0 || Math.max(c.anchor, c.head) > transaction.newDoc.length) return [];
        const colour = /^#[0-9a-f]{6}$/i.test(c.colour) ? c.colour : '#537bc4';
        const caret = Decoration.widget({ widget: new CollaboratorCaret(c.name, colour), side: 1 }).range(c.head);
        return c.anchor === c.head ? [caret] : [Decoration.mark({ attributes: { style: `background-color:${colour}30` } }).range(Math.min(c.anchor, c.head), Math.max(c.anchor, c.head)), caret];
      });
      result = Decoration.set(ranges, true);
    }
    return result;
  },
  provide: (field) => EditorView.decorations.from(field),
});

export interface EditorProps {
  remoteCursors?: RemoteCursor[];
  onSelection?: (selection: { anchor: number; head: number } | null) => void;
  text: string;
  placeholder?: string;
  autofocus?: boolean;
  onCommit: (text: string) => void;
  onCancel: () => void;
  onChange?: (text: string) => void;
  /** The text does not parse: drawn with a warning border. */
  invalid?: boolean;
  errorId?: string;
  /** Existing cells save on leave; new drafts require explicit submission. */
  commitOnBlur?: boolean;
  /** Focus left the cell (after committing any change). */
  onLeave?: () => void;
  /** Gives the parent a way to replace the text, e.g. from the cell-kind menu. */
  ref?: (api: { set: (t: string, cursor?: number, focus?: boolean) => void; focus: () => void }) => void;
}

export function Editor(props: EditorProps) {
  let host!: HTMLDivElement;
  let view: EditorView;
  let committed = props.text;
  const text = () => view.state.doc.toString();
  const commit = () => {
    committed = text();
    props.onCommit(committed);
    return true;
  };
  const multiline = () => text().trimStart().startsWith('note');

  onMount(() => {
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: props.text,
        extensions: [
          history(), remoteDecoration,
          keymap.of([
            { key: 'Mod-Enter', run: commit },
            { key: 'Enter', run: () => (multiline() ? false : commit()) },
            { key: 'Escape', run: () => (props.onCancel(), true) },
            ...historyKeymap,
            ...defaultKeymap,
          ]),
          EditorView.lineWrapping,
          cmPlaceholder(props.placeholder ?? ''),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) props.onChange?.(text());
            if (u.selectionSet || u.docChanged || u.focusChanged) props.onSelection?.(u.view.hasFocus ? { anchor: u.state.selection.main.anchor, head: u.state.selection.main.head } : null);
          }),
          EditorView.domEventHandlers({
            blur: (e) => {
              // leaving for another part of the same cell (its tools) does not commit
              const to = (e as FocusEvent).relatedTarget as Node | null;
              if (to && host.closest('.cell')?.contains(to)) return false;
              if (props.commitOnBlur !== false && text() !== committed) commit();
              props.onLeave?.();
              return false;
            },
          }),
          EditorView.contentAttributes.of({ 'aria-label': 'Cell text', spellcheck: 'false' }),
        ],
      }),
    });
    props.ref?.({
      set: (t, cursor, focus = true) => {
        if (!focus) committed = t;
        const anchor = Math.min(cursor ?? (focus ? t.length : view.state.selection.main.anchor), t.length);
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: t }, selection: { anchor } });
        if (focus) view.focus();
      },
      focus: () => view.focus(),
    });
    // a cell rebuilt by a canvas gesture or the AI leaves the focus where it is
    if (props.autofocus && !document.activeElement?.closest('[data-keeps-focus]')) {
      view.focus();
      view.dispatch({ selection: { anchor: view.state.doc.length } });
    }
  });
  createEffect(() => {
    const invalid = !!props.invalid;
    const errorId = props.errorId;
    if (!view) return;
    view.contentDOM.setAttribute('aria-invalid', String(invalid));
    if (invalid && errorId) view.contentDOM.setAttribute('aria-describedby', errorId);
    else view.contentDOM.removeAttribute('aria-describedby');
  });
  createEffect(() => {
    const cursors = props.remoteCursors ?? [];
    if (view) view.dispatch({ effects: remotePositions.of(cursors) });
  });
  onCleanup(() => { props.onSelection?.(null); view?.destroy(); });
  return <div class="editor" classList={{ invalid: !!props.invalid }} ref={host} />;
}
