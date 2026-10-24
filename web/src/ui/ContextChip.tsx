// The context chip: the presentation, type, weight and ring; click to change them.
import { createEffect, createResource, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { engine } from '../app';
import { contextTex, ringTex, toTex } from './tex';
import { Typeset } from './Typeset';
import { NoteView } from './Results';
import { ContextForm, PresentationTitle } from './ContextForm';

export function ContextChip(props: { context: string; scalars: string[]; readOnly?: boolean; onSet: (c: string) => void }) {
  const [open, setOpen] = createSignal(false);
  const [draft, setDraft] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [problem, setProblem] = createSignal('');
  let revision = 0;
  const invalidate = () => { revision++; setBusy(false); setProblem(''); };
  const change = (line: string) => { invalidate(); setDraft(line); };
  onCleanup(() => revision++);
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
    invalidate();
    setOpen(false);
    if (refocus) chip?.focus();
  };
  createEffect(on(() => [props.context, props.readOnly] as const, () => close(), { defer: true }));
  // a click outside closes the popover
  createEffect(() => {
    if (!open()) return;
    const onDown = (e: PointerEvent) => !wrap?.contains(e.target as Node) && close();
    document.addEventListener('pointerdown', onDown);
    onCleanup(() => document.removeEventListener('pointerdown', onDown));
  });
  const apply = async () => {
    if (props.readOnly || !open() || busy()) return;
    const attempt = ++revision;
    const source = draft();
    setBusy(true);
    setProblem('');
    try {
      const o = await engine.call('context.describe', { context: source }, { lane: 'interactive' });
      if (attempt !== revision || !open() || props.readOnly) return;
      setBusy(false);
      if (o.status !== 'done') return setProblem('Validation stopped. Apply again to retry.');
      const r = o.response;
      if (r.status !== 'ok') return setProblem(r.message);
      close(!!wrap?.contains(document.activeElement));
      props.onSet(r.context);
    } catch (error) {
      if (attempt !== revision) return;
      setBusy(false);
      setProblem(error instanceof Error ? error.message : 'Could not validate the context. Try again.');
    }
  };
  return (
    <span style={{ position: 'relative', 'min-width': '0' }} ref={wrap}>
      <button class="chip" disabled={props.readOnly} title="Change the context" data-testid="context" aria-expanded={open()} aria-haspopup="dialog" ref={chip}
        onClick={() => { if (open()) close(); else { change(props.context); setOpen(true); } }}>
        <ContextText context={props.context} />
      </button>
      <Show when={open()}>
        <div class="pop context-options" style={{ top: '30px', left: 0 }} role="dialog" aria-label="Presentation and options"
          onKeyDown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); close(true); } }}>
          <ContextForm line={draft()} onLine={change} onSubmit={() => void apply()} />
          <label class="ctx-line">
            <span class="hint">Context line</span>
            <span class="row">
              <input class="grow mono" value={draft()} aria-label="Context" ref={(el) => queueMicrotask(() => el.focus())}
                onInput={(e) => change(e.currentTarget.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void apply(); } }} />
            </span>
          </label>
          <div class="row" style={{ 'margin-top': '10px' }}>
            <button class="primary" disabled={busy()} onClick={apply}>{busy() ? 'Checking…' : 'Apply'}</button>
            <button class="ghost" onClick={() => close(true)}>Cancel</button>
          </div>
          <Show when={problem()}><div class="error" role="status">{problem()}</div></Show>
          <Show when={info()}>
            {(d) => (
              <details class="context-details">
              <summary>Current context details</summary>
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
              </details>
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
