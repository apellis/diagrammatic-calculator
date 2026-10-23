import { createSignal, For, Show } from 'solid-js';
import type { OpenDoc } from '../doc/store';
import type { Inputs } from '../doc/model';
import type { DocSync } from '../account/docsync';
import { api } from '../account/account';
import { cloudCheckpoint, recoverCloud } from '../account/recovery';

type Checkpoint = { id: string; time: number; author: string; note: string; size: number };
export function CloudRecovery(props: { o: OpenDoc; sync: DocSync | null; onBranch: (inputs: Inputs, name: string) => void }) {
  const [opened, setOpened] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal('');
  const [versions, setVersions] = createSignal<Checkpoint[]>([]);
  const [preview, setPreview] = createSignal<{ inputs: Inputs; version: Checkpoint }>();
  const link = () => props.sync?.link() ?? props.o.info.cloud;
  const legacy = () => !!link() && (link()!.generation ?? 0) === 0;
  const mismatch = () => props.sync?.status() === 'generation-mismatch';
  const owner = () => props.sync?.user && link()?.role === 'owner' && link()?.account === props.sync?.user?.id;
  const run = async (fn: () => Promise<void>) => {
    if (busy()) return;
    setBusy(true); setError('');
    try { await fn(); } catch (e) { setError(`${e instanceof Error ? e.message : String(e)}. Your original browser copy is preserved.`); }
    finally { setBusy(false); }
  };
  const recover = (mode: 'migrate' | 'reopen') => run(async () => {
    await props.sync?.stop();
    const result = await recoverCloud(props.o, api, mode, props.sync?.user?.id ?? null);
    location.hash = `#/doc/${result.id}`;
  });
  const showVersions = () => run(async () => {
    const c = link(); if (!c) return;
    setOpened(true); setPreview(undefined);
    setVersions((await api.versions(c.id, c.token)).versions);
  });
  return <>
    <Show when={legacy() || mismatch()}>
      <aside role="status" class="notice">
        <p>{mismatch() ? 'The account document has a newer generation.' : 'This document needs an owner upgrade for live editing.'} Recovery keeps your old browser copy and private checkpoints.</p>
        <Show when={legacy() && owner() && !mismatch()}><button disabled={busy()} onClick={() => void recover('migrate')}>Upgrade for live editing</button></Show>
        <button disabled={busy()} onClick={() => void recover('reopen')}>Reopen account document safely</button>
      </aside>
    </Show>
    <Show when={link()}><button disabled={busy()} onClick={() => void showVersions()}>Cloud checkpoints</button></Show>
    <Show when={error()}><p role="alert">{error()}</p></Show>
    <Show when={opened()}>
      <section role="dialog" aria-label="Cloud checkpoints" aria-modal="false" class="panel">
        <h2>Cloud checkpoints</h2>
        <p>Read-only previews. Open one as a personal branch; this does not replace the live document.</p>
        <button onClick={() => { setOpened(false); setPreview(undefined); }}>Close checkpoints</button>
        <Show when={!versions().length}><p>No retained cloud checkpoints.</p></Show>
        <For each={versions()}>{v => <div>
          <span>{new Date(v.time).toLocaleString()} · {v.author} · {v.note}</span>{' '}
          <button disabled={busy()} onClick={() => void run(async () => {
            const c = link(); if (!c) return;
            setPreview({ inputs: await cloudCheckpoint(api, c.id, v.id, c.token), version: v });
          })}>Preview checkpoint</button>
        </div>}</For>
        <Show when={preview()}>{p => <section aria-label="Read-only checkpoint preview">
          <h3>{p().version.note || 'Checkpoint'} · {p().version.author}</h3>
          <pre>{p().inputs.context}</pre>
          <For each={p().inputs.cells}>{cell => <pre>{cell.text}</pre>}</For>
          <button onClick={() => { props.onBranch(p().inputs, `Checkpoint: ${p().version.note || new Date(p().version.time).toLocaleString()}`); setOpened(false); }}>Open as personal branch</button>
        </section>}</Show>
      </section>
    </Show>
  </>;
}
