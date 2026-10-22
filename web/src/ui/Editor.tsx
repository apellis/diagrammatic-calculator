// A text cell's editor: CodeMirror with one line per cell (notes may span lines).
import { onCleanup, onMount } from 'solid-js';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap, placeholder as cmPlaceholder } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';

export interface EditorProps {
  text: string;
  placeholder?: string;
  autofocus?: boolean;
  onCommit: (text: string) => void;
  onCancel: () => void;
  onChange?: (text: string) => void;
  /** The text does not parse: drawn with a warning border. */
  invalid?: boolean;
  /** Focus left the cell (after committing any change). */
  onLeave?: () => void;
  /** Gives the parent a way to replace the text, e.g. from the cell-kind menu. */
  ref?: (api: { set: (t: string, cursor?: number) => void; focus: () => void }) => void;
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
          history(),
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
          }),
          EditorView.domEventHandlers({
            blur: (e) => {
              // leaving for another part of the same cell (its tools) does not commit
              const to = (e as FocusEvent).relatedTarget as Node | null;
              if (to && host.closest('.cell')?.contains(to)) return false;
              if (text() !== committed) commit();
              props.onLeave?.();
              return false;
            },
          }),
          EditorView.contentAttributes.of({ 'aria-label': 'Cell text', spellcheck: 'false' }),
        ],
      }),
    });
    props.ref?.({
      set: (t, cursor) => {
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: t }, selection: { anchor: cursor ?? t.length } });
        view.focus();
      },
      focus: () => view.focus(),
    });
    // a cell rebuilt by a canvas gesture or the AI leaves the focus where it is
    if (props.autofocus && !document.activeElement?.closest('[data-keeps-focus]')) {
      view.focus();
      view.dispatch({ selection: { anchor: view.state.doc.length } });
    }
  });
  onCleanup(() => view?.destroy());
  return <div class="editor" classList={{ invalid: !!props.invalid }} ref={host} />;
}
