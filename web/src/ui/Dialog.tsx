// A modal dialog: takes the focus when it opens and keeps it inside, closes on Esc, a click on the
// backdrop or its close button, and gives the focus back to where it was.
import { onCleanup, onMount, type JSX } from 'solid-js';

const FOCUSABLE = 'input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

export function Dialog(props: {
  /** The dialog's accessible name. */
  label: string;
  onClose: () => void;
  class?: string;
  /** Leave out the × in the corner (the dialog has its own way to close). */
  noClose?: boolean;
  children: JSX.Element;
}) {
  let box!: HTMLDivElement;
  const previous = document.activeElement as HTMLElement | null;
  onMount(() => {
    // the first field, else the first control that is not the close button, else the dialog itself
    const all = [...box.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((e) => !e.classList.contains('modal-close'));
    const first = all.find((e) => e.matches('input, select, textarea, [autofocus]')) ?? all[0];
    (first ?? box).focus();
  });
  // after the key that closed the dialog is done with, so it does not act on the restored focus
  onCleanup(() => setTimeout(() => previous?.isConnected && previous.focus()));
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      props.onClose();
      return;
    }
    if (e.key !== 'Tab') return;
    const all = [...box.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((x) => x.offsetParent !== null);
    if (!all.length) return;
    const [first, last] = [all[0], all[all.length - 1]];
    if (e.shiftKey && (document.activeElement === first || document.activeElement === box)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };
  return (
    <div class="modal-bg" onClick={props.onClose}>
      <div class={`modal ${props.class ?? ''}`} role="dialog" aria-modal="true" aria-label={props.label} tabindex={-1} ref={box}
        onClick={(e) => e.stopPropagation()} onKeyDown={onKey}>
        {!props.noClose && <button class="ghost modal-close" aria-label="Close" title="Close (Esc)" onClick={props.onClose}>×</button>}
        {props.children}
      </div>
    </div>
  );
}
