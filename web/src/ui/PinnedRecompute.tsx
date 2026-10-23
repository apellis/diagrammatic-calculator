import { createSignal, onCleanup, Show } from 'solid-js';
import { engine } from '../app';
import type { Snapshot } from '../doc/model';
import type { WorkspaceRunCell } from '../engine/ops.gen';
import { comparePinned, recomputePinned } from '../eval/pinned';
import { ResultView } from './Results';

export function PinnedRecompute(props: { snapshot: Snapshot }) {
  const [busy, setBusy] = createSignal(false);
  const [result, setResult] = createSignal<WorkspaceRunCell>();
  const [error, setError] = createSignal('');
  const abort = new AbortController();
  onCleanup(() => abort.abort());
  const run = async () => {
    if (!props.snapshot.run || busy()) return;
    setBusy(true); setError(''); setResult(undefined);
    try {
      const value = await recomputePinned(engine, props.snapshot.run, abort.signal);
      if (!abort.signal.aborted) setResult(value);
    } catch (e) {
      if (!abort.signal.aborted) setError(e instanceof Error ? e.message : 'Recomputation failed');
    } finally { if (!abort.signal.aborted) setBusy(false); }
  };
  return <div data-testid="pinned-recompute">
    <button disabled={!props.snapshot.run || busy()} onClick={() => void run()}>{busy() ? 'Computing…' : 'Recompute'}</button>
    <Show when={error()}><p role="alert">{error()}</p></Show>
    <Show when={result()}>{value => <>
      <p role="status">{comparePinned(props.snapshot.value, value()) ? 'Matches the saved value and provenance.' : 'Differs from the saved value or provenance.'}</p>
      <ResultView s={{ id: props.snapshot.cellId, text: props.snapshot.cellText, status: 'fresh', value: value() }} />
    </>}</Show>
  </div>;
}
