// Typeset text: the reading font for display, the fixed-width font only where text is entered.
import { createSignal, Show } from 'solid-js';
import { render, toTex } from './tex';

/** Text typeset as mathematics (from `tex` if given, else read from `text`); the text itself if it cannot be. */
export function Typeset(p: { text: string; tex?: string; class?: string; display?: boolean }) {
  const html = () => render(p.tex ?? toTex(p.text), p.display);
  return (
    <Show when={html()} fallback={<span class={`typeset ${p.class ?? ''}`}>{p.text}</span>}>
      {(h) => <span class={`typeset ${p.class ?? ''}`} innerHTML={h()} />}
    </Show>
  );
}

/**
 * A result shown typeset. Clicking it shows its text in the fixed-width font, selected, to copy or
 * read exactly; clicking again (or leaving it) typesets it again.
 */
export function Value(p: { text: string; class?: string; testid?: string }) {
  const [raw, setRaw] = createSignal(false);
  let el: HTMLDivElement | undefined;
  const show = () => {
    setRaw(true);
    queueMicrotask(() => {
      if (!el) return;
      el.focus();
      const r = document.createRange();
      r.selectNodeContents(el);
      const s = window.getSelection();
      s?.removeAllRanges();
      s?.addRange(r);
    });
  };
  return (
    <Show when={raw()} fallback={
      <div class={`value ${p.class ?? ''}`} data-testid={p.testid} data-text={p.text} title="Click for the text" onClick={show}>
        <Typeset text={p.text} />
      </div>
    }>
      <div class={`value raw ${p.class ?? ''}`} data-testid={p.testid} data-text={p.text} tabindex={0} ref={el} onBlur={() => setRaw(false)}
        onClick={() => window.getSelection()?.isCollapsed && setRaw(false)}>{p.text}</div>
    </Show>
  );
}
