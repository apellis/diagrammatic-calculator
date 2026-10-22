// The context chip: the presentation, type, weight and ring; click to change them.
import { createResource, createSignal, For, Show } from 'solid-js';
import { engine } from '../app';

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
        {props.context}
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
                  <tr><td>ring</td><td class="mono">{d().ring}</td></tr>
                  <tr><td>objects</td><td class="mono">{d().objects.join('  ')}</td></tr>
                  <tr><td>generators</td><td class="mono">{d().generators.map((g) => g.name).join(', ')}</td></tr>
                  <tr><td>relations</td><td class="mono" style={{ 'font-size': '12px' }}>{d().relations.join(', ')}</td></tr>
                  <tr><td>computed by</td><td>{d().provenance.strategy}, {d().provenance.method} ({d().provenance.certificate.status})</td></tr>
                  <Show when={props.scalars.length}><tr><td>unknowns</td><td class="mono">{props.scalars.join(', ')}</td></tr></Show>
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
