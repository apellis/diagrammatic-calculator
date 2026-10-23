// The context chip: the presentation, type, weight and ring; click to change them.
import { createEffect, createResource, createSignal, For, onCleanup, Show } from 'solid-js';
import { engine } from '../app';
import { contextTex, ringTex, toTex } from './tex';
import { Typeset } from './Typeset';
import { NoteView } from './Results';
import { ContextForm, PresentationTitle } from './ContextForm';

export function ContextChip(props: { context: string; scalars: string[]; readOnly?: boolean; onSet: (c: string) => void }) {
  const [open, setOpen] = createSignal(false);
  const [draft, setDraft] = createSignal('');
  const [info] = createResource(
    () => (open() ? props.context : null),
    async (c) => {
      const o = await engine.call('context.describe', { context: c }, { lane: 'interactive' });
      return o.status === 'done' && o.response.status === 'ok' ? o.response : null;
    },
  );
  let wrap: HTMLSpanElement | undefined;
  let chip: HTMLButtonElement | undefined;
  const close = (refocus = false) => {
    setOpen(false);
    if (refocus) chip?.focus();
  };
  // a click outside closes the popover
  createEffect(() => {
    if (!open()) return;
    const onDown = (e: PointerEvent) => !wrap?.contains(e.target as Node) && close();
    document.addEventListener('pointerdown', onDown);
    onCleanup(() => document.removeEventListener('pointerdown', onDown));
  });
  const apply = async () => {
    if (props.readOnly) return;
    const o = await engine.call('context.describe', { context: draft() }, { lane: 'interactive' });
    if (o.status !== 'done') return;
    const r = o.response;
    if (r.status !== 'ok') return; // the form shows the engine's message
    // focus goes back to the chip only from the popover: if the person has moved on (a click in a
    // cell while the engine was checking the line), it stays where they put it
    close(!!wrap?.contains(document.activeElement));
    if (!props.readOnly) props.onSet(r.context);
  };
  return (
    <span style={{ position: 'relative', 'min-width': '0' }} ref={wrap}>
      <button class="chip" disabled={props.readOnly} title="Change the context" data-testid="context" aria-expanded={open()} aria-haspopup="dialog" ref={chip}
        onClick={() => { setDraft(props.context); setOpen(!open()); }}>
        <ContextText context={props.context} />
      </button>
      <Show when={open()}>
        <div class="pop" style={{ top: '30px', left: 0 }} role="dialog" aria-label="Presentation and options"
          onKeyDown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); close(true); } }}>
          <ContextForm line={draft()} onLine={setDraft} onSubmit={() => void apply()} />
          <label class="ctx-line">
            <span class="hint">Context line</span>
            <span class="row">
              <input class="grow mono" value={draft()} aria-label="Context" ref={(el) => queueMicrotask(() => el.focus())}
                onInput={(e) => setDraft(e.currentTarget.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void apply(); } }} />
              <button class="primary" onClick={apply}>Set</button>
            </span>
          </label>
          <Show when={info()}>
            {(d) => (
              <table class="kv" style={{ 'margin-top': '8px' }}>
                <tbody>
                  <tr><td>presentation</td><td><PresentationTitle text={d().title} /></td></tr>
                  <tr><td>ring</td><td><Typeset text={d().ring} tex={ringTex(d().ring)} /></td></tr>
                  <Show when={d().objects.length}>
                    <tr><td>objects</td><td><Typeset text={d().objects.join('  ')} tex={d().objects.map(toTex).join(',\\quad ')} /></td></tr>
                  </Show>
                  <tr><td>generators</td><td><Typeset text={d().generators.map((g) => g.name).join(', ')} /></td></tr>
                  <tr><td>relations</td><td style={{ 'font-size': '12px' }}>{d().relations.join(', ')}</td></tr>
                  <tr><td>computed by</td><td>{d().provenance.strategy}, {d().provenance.method} ({d().provenance.certificate.status})</td></tr>
                  <Show when={props.scalars.length}><tr><td>unknowns</td><td><Typeset text={props.scalars.join(', ')} /></td></tr></Show>
                  <For each={d().provenance.notes ?? []}>{(n) => <tr><td /><td class="hint"><NoteView text={n} inline /></td></tr>}</For>
                </tbody>
              </table>
            )}
          </Show>
        </div>
      </Show>
    </span>
  );
}

/** A context typeset: the presentation's name, then its options as mathematics. */
export function ContextText(props: { context: string }) {
  const name = () => props.context.trim().split(/\s+/)[0] ?? '';
  const options = () => props.context.trim().slice(name().length);
  return (
    <span class="ctx" data-text={props.context}>
      <span class="ctx-name">{name()}</span>{' '}
      <Typeset text={options()} tex={contextTex(options())} />
    </span>
  );
}
