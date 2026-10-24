import { createEffect, createSignal, For, onCleanup, Show, type JSX } from 'solid-js';
import { engine } from '../app';
import type { ContextDescribeResponse, TermLayoutLayout } from '../engine/ops.gen';
import { identityTerm, orientedIdentity, swapBoundary } from './diagram-start';
import './diagram-start.css';

type Description = Extract<ContextDescribeResponse, { status: 'ok' }>;
type Validated = { context: string; object: string; term: string; layout: TermLayoutLayout };
const explicit = (d?: Description) => !!d && !d.objects.length && d.colours.some((c) => c.oriented);

/** A private starting boundary. Only an engine-validated identity leaves this component. */
export function DiagramStart(props: { context: string; onStart: (term: string) => void; onChange?: () => void }): JSX.Element {
  const [description, setDescription] = createSignal<{ context: string; value: Description }>();
  const [selected, setSelectedValue] = createSignal<string>();
  const setSelected = (object: string | undefined) => {
    // Invalidate the composer's pending parse too, not only this component's layout.
    props.onChange?.();
    setSelectedValue(object);
  };
  const [validated, setValidated] = createSignal<Validated>();
  const [loading, setLoading] = createSignal(true);
  const [loadError, setLoadError] = createSignal('');
  const [validationError, setValidationError] = createSignal('');
  let disposed = false;
  onCleanup(() => { disposed = true; });
  const current = () => description()?.context === props.context ? description()?.value : undefined;
  const ready = () => {
    const v = validated();
    return !disposed && v?.context === props.context && v.object === selected() ? v : undefined;
  };

  createEffect(() => {
    const context = props.context;
    const controller = new AbortController();
    onCleanup(() => controller.abort());
    setDescription(undefined);
    setSelected(undefined);
    setLoading(true);
    setLoadError('');
    void (async () => {
      try {
        const out = await engine.call('context.describe', { context }, { lane: 'interactive', signal: controller.signal });
        if (controller.signal.aborted || disposed || props.context !== context) return;
        if (out.status !== 'done') {
          setLoadError(out.status === 'exploded' ? out.message : 'Boundary loading was cancelled. Change the context to try again.');
        } else if (out.response.status !== 'ok') setLoadError(out.response.message);
        else {
          setDescription({ context, value: out.response });
          setSelected(out.response.objects[0] ?? (explicit(out.response) ? '' : undefined));
        }
      } catch (error) {
        if (!controller.signal.aborted && !disposed && props.context === context) setLoadError(String(error));
      } finally {
        if (!controller.signal.aborted && !disposed && props.context === context) setLoading(false);
      }
    })();
  });

  createEffect(() => {
    const context = props.context;
    const object = selected();
    const term = explicit(current()) ? orientedIdentity(object, current()!.colours) : identityTerm(object, current()?.objects ?? []);
    const controller = new AbortController();
    onCleanup(() => controller.abort());
    setValidated(undefined);
    setValidationError('');
    if (term === undefined || object === undefined) return;
    void (async () => {
      try {
        const out = await engine.call('term.layout', { context, term }, { lane: 'interactive', signal: controller.signal });
        if (controller.signal.aborted || disposed || props.context !== context || selected() !== object) return;
        if (out.status !== 'done') {
          setValidationError(out.status === 'exploded' ? out.message : 'Boundary validation was cancelled. Select a boundary again.');
        } else if (out.response.status !== 'ok') setValidationError(out.response.message);
        else {
          const layout = out.response.summands[0];
          if (out.response.terms !== 1 || !layout || layout.source !== object || layout.target !== object || layout.pieces.length !== 0) {
            setValidationError('This boundary is not a drawable identity. Use text entry instead.');
          } else setValidated({ context, object, term, layout });
        }
      } catch (error) {
        if (!controller.signal.aborted && !disposed && props.context === context && selected() === object) setValidationError(String(error));
      }
    })();
  });

  const start = () => {
    const v = ready();
    if (v) props.onStart(v.term);
  };
  const colour = (name: string) => `var(--strand-${Math.max(0, current()?.colours.findIndex((c) => c.name === name) ?? 0) % 8})`;
  const swap = (left: number) => {
    const v = ready();
    return v ? swapBoundary(v.layout.boundaries[0]?.object ?? [], left, explicit(current()) ? undefined : current()?.objects ?? []) : undefined;
  };

  return <section class="diagram-start" aria-label="Starting boundary">
    <div class="diagram-start-heading"><strong>Starting strands</strong><span class="hint">Choose the sequence, then add dots and crossings.</span></div>
    <Show when={loading()}><p class="hint" role="status">Loading boundaries…</p></Show>
    <Show when={loadError()}><p class="error" role="alert">{loadError()}</p></Show>
    <Show when={current()}>{(d) => <>
      <Show when={d().objects.length > 0 || explicit(d())} fallback={<p class="hint" role="status">This presentation has no listed starting boundaries. Use text entry to specify a boundary.</p>}>
        <Show when={explicit(d())}>
          <div class="diagram-start-presets" role="group" aria-label="Add strands">
            <For each={d().colours}>{(c) => <>
              <button type="button" aria-label={`Add upward ${c.name} strand`} onClick={() => setSelected([selected(), c.name].filter(Boolean).join(' '))}>+ {c.name} ↑</button>
              <Show when={c.oriented}><button type="button" aria-label={`Add downward ${c.name} strand`} onClick={() => setSelected([selected(), `${c.name}*`].filter(Boolean).join(' '))}>+ {c.name} ↓</button></Show>
            </>}</For>
            <button type="button" disabled={!selected()} onClick={() => setSelected(selected()!.split(' ').slice(0, -1).join(' '))}>Remove last</button>
            <button type="button" disabled={!selected()} onClick={() => setSelected('')}>Clear</button>
          </div>
        </Show>
        <Show when={d().objects.length > 1}>
          <Show when={d().objects.length <= 8} fallback={
            <label class="diagram-start-select">Boundary sequence<select value={selected()} onChange={(e) => setSelected(e.currentTarget.value)}>
              <For each={d().objects}>{(object) => <option value={object}>{object || 'Empty boundary'}</option>}</For>
            </select></label>
          }>
            <div class="diagram-start-presets" role="group" aria-label="Boundary presets">
              <For each={d().objects}>{(object) => <button type="button" class="mono" aria-pressed={selected() === object} onClick={() => setSelected(object)}>{object || 'Empty boundary'}</button>}</For>
            </div>
          </Show>
        </Show>
        <div class="diagram-start-preview" aria-busy={!ready() && !validationError()}>
          <Show when={ready()} fallback={<p class="hint" role="status">{validationError() ? 'Preview unavailable.' : 'Checking boundary…'}</p>}>{(v) => {
            const strands = () => v().layout.strands;
            const width = () => Math.max(132, strands().length * 54 + 24);
            const x = (i: number) => width() / 2 + (i - (strands().length - 1) / 2) * 54;
            return <div class="diagram-start-strands" style={{ width: `${width()}px` }}>
              <svg width={width()} height="150" viewBox={`0 0 ${width()} 150`} role="img" aria-label={`Identity diagram: ${v().object || 'empty boundary'}`}>
                <For each={strands()}>{(strand, i) => <g>
                  <line x1={x(i())} x2={x(i())} y1="18" y2="111" stroke={colour(strand.colour)} stroke-width="3" stroke-linecap="round" />
                  <Show when={d().colours.find((c) => c.name === strand.colour)?.oriented}>
                    <path d={strand.up ? `M ${x(i()) - 5} 69 L ${x(i())} 62 L ${x(i()) + 5} 69` : `M ${x(i()) - 5} 62 L ${x(i())} 69 L ${x(i()) + 5} 62`} fill="none" stroke={colour(strand.colour)} stroke-width="2" />
                  </Show>
                  <text x={x(i())} y="136" text-anchor="middle" fill="currentColor">{strand.colour}{strand.up ? '' : '*'}</text>
                </g>}</For>
                <Show when={!strands().length}><text x={width() / 2} y="80" text-anchor="middle" fill="currentColor">Empty boundary</text></Show>
              </svg>
              <div class="diagram-start-swaps" role="group" aria-label="Reorder strands">
                <For each={strands().slice(0, -1)}>{(_, i) => <button type="button" style={{ left: `${x(i()) + 27}px` }} aria-label={`Swap strands ${i() + 1} and ${i() + 2}`} title={`Swap strands ${i() + 1} and ${i() + 2}`} disabled={swap(i()) === undefined} onClick={() => { const next = swap(i()); if (next !== undefined) setSelected(next); }}>↔</button>}</For>
              </div>
            </div>;
          }}</Show>
        </div>
        <Show when={validationError()}><p class="error" role="alert">{validationError()}</p></Show>
        <div class="diagram-start-footer"><span class="hint">{explicit(d()) ? 'Add strands above, or start empty to draw cups.' : d().objects.length > 1 ? 'Swap neighbours to reorder the strands.' : 'Strands from the context weight.'}</span><button type="button" class="primary" disabled={!ready()} onClick={start}>Start drawing</button></div>
      </Show>
    </>}</Show>
  </section>;
}
