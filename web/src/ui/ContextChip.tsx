// The context chip: the presentation, type, weight and ring; click to change them.
import { createResource, createSignal, For, Show } from 'solid-js';
import { engine } from '../app';
import { contextTex, ringTex, toTex } from './tex';
import { Typeset } from './Typeset';

export function ContextChip(props: { context: string; scalars: string[]; onSet: (c: string) => void }) {
  const [open, setOpen] = createSignal(false);
  const [draft, setDraft] = createSignal('');
  const [error, setError] = createSignal('');
  const [info] = createResource(
    () => (open() ? props.context : null),
    async (c) => {
      const o = await engine.call('context.describe', { context: c }, { lane: 'interactive' });
      return o.status === 'done' && o.response.status === 'ok' ? o.response : null;
    },
  );
  const apply = async () => {
    const o = await engine.call('context.describe', { context: draft() }, { lane: 'interactive' });
    if (o.status !== 'done') return;
    const r = o.response;
    if (r.status !== 'ok') return setError(r.message);
    setError('');
    setOpen(false);
    props.onSet(r.context);
  };
  return (
    <span style={{ position: 'relative' }}>
      <span class="chip" role="button" tabIndex={0} title="Change the context" data-testid="context"
        onClick={() => { setDraft(props.context); setError(''); setOpen(!open()); }}
        onKeyDown={(e) => e.key === 'Enter' && (setDraft(props.context), setOpen(true))}>
        <ContextText context={props.context} />
      </span>
      <Show when={open()}>
        <div class="pop" style={{ top: '30px', left: 0 }}>
          <div class="row">
            <input class="grow mono" value={draft()} aria-label="Context" autofocus
              onInput={(e) => setDraft(e.currentTarget.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') void apply(); if (e.key === 'Escape') setOpen(false); }} />
            <button class="primary" onClick={apply}>Set</button>
          </div>
          <Show when={error()}><div class="error" style={{ 'margin-top': '8px' }}>{error()}</div></Show>
          <div class="hint" style={{ 'margin-top': '6px' }}>
            For example <code>klr type=B2 nu=i+2j ring=GF(5)</code>, <code>nilhecke nu=3</code>, or a finite quotient with <code>Lambda=2i</code>, <code>dotcap=3</code> or <code>degcap=4</code>.
          </div>
          <Show when={info()}>
            {(d) => (
              <table class="kv" style={{ 'margin-top': '8px' }}>
                <tbody>
                  <tr><td>presentation</td><td>{d().title}</td></tr>
                  <tr><td>ring</td><td><Typeset text={d().ring} tex={ringTex(d().ring)} /></td></tr>
                  <tr><td>objects</td><td><Typeset text={d().objects.join('  ')} tex={d().objects.map(toTex).join(',\\quad ')} /></td></tr>
                  <tr><td>generators</td><td><Typeset text={d().generators.map((g) => g.name).join(', ')} /></td></tr>
                  <tr><td>relations</td><td style={{ 'font-size': '12px' }}>{d().relations.join(', ')}</td></tr>
                  <tr><td>computed by</td><td>{d().provenance.strategy}, {d().provenance.method} ({d().provenance.certificate.status})</td></tr>
                  <Show when={props.scalars.length}><tr><td>unknowns</td><td><Typeset text={props.scalars.join(', ')} /></td></tr></Show>
                  <For each={d().provenance.notes ?? []}>{(n) => <tr><td /><td class="hint">{n}</td></tr>}</For>
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
