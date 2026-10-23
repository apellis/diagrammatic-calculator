import { createEffect, createResource, createSignal, For, on, Show } from 'solid-js';
import { createDoc, deleteDoc, duplicateDoc, isPersisted, listDocs, openDoc, type DocInfo } from '../doc/store';
import { parseDcw } from '../doc/dcw';
import { ago, pickFile } from '../app';
import { account, api, ours, syncAll, takeSignInOutcome, user } from '../account/account';
import { ApiError, type CloudDoc } from '../account/api';
import { AccountBar, SharedWithMe, signInMessage } from './Account';

const examples = import.meta.glob('../../../examples/*.dcw', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

const open = (id: string) => (location.hash = `#/doc/${id}`);

export async function importText(name: string, text: string): Promise<string> {
  const ws = await parseDcw(text);
  return createDoc(name.replace(/\.dcw$/, ''), (d) => d.replace(ws.context, ws.cells, `import ${name}`));
}

export function DocList() {
  const [docs, { refetch }] = createResource(listDocs);
  const [persisted] = createResource(isPersisted);
  const [query, setQuery] = createSignal('');
  const [error, setError] = createSignal('');
  const shown = () => (docs() ?? []).filter((d) => d.name.toLowerCase().includes(query().toLowerCase()));
  const [notice, setNotice] = createSignal(signInMessage(takeSignInOutcome()));
  const [shared, setShared] = createSignal<CloudDoc[]>([]);
  const [syncing, setSyncing] = createSignal(false);

  // signed in: save documents made here to the account, copy the account's documents here, and
  // list what others share
  createEffect(
    on(user, async (u) => {
      if (!u) return setShared([]);
      setSyncing(true);
      try {
        const r = await syncAll(u, refetch);
        if (r.uploaded) setNotice(`Saved ${r.uploaded} ${r.uploaded === 1 ? 'document' : 'documents'} from this browser to your account.`);
        if (r.failed) setError(`${r.failed} ${r.failed === 1 ? 'document' : 'documents'} could not be synced with your account; they are safe in this browser.`);
        setShared((await api.list()).shared);
      } catch {
        setError('Your account cannot be reached; documents are safe in this browser.');
      } finally {
        setSyncing(false);
        refetch();
      }
    }),
  );

  /** Where a document is kept, as a short tag beside its name. */
  const where = (d: DocInfo): string | null => {
    const u = user();
    if (!d.cloud) return null;
    if (u && !ours(d, u)) return 'other account';
    return d.cloud.role === 'owner' ? null : d.cloud.role === 'editor' ? 'shared, can edit' : 'shared, can view';
  };
  const inAccount = (d: DocInfo) => !!d.cloud && !!user() && ours(d, user()!);
  const remove = async (d: DocInfo) => {
    const owned = inAccount(d) && d.cloud!.role === 'owner';
    const text = owned
      ? `Delete “${d.name}” from your account and this browser? This cannot be undone.`
      : d.cloud && d.cloud.role !== 'owner'
        ? `Remove “${d.name}” from this browser? It stays shared with you.`
        : d.cloud
          ? `Remove “${d.name}” from this browser? It stays in the account it was saved to.`
          : `Delete “${d.name}”? This cannot be undone.`;
    if (!confirm(text)) return;
    if (owned) {
      try {
        await api.remove(d.cloud!.id);
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 404)) return setError(`“${d.name}” could not be deleted from your account; it is kept.`);
      }
    }
    await deleteDoc(d.id);
    refetch();
  };

  const create = async () => open(await createDoc('Untitled', (d) => d.setContext('klr type=A2 nu=2i+j ring=QQ')));
  const doImport = async () => {
    const f = await pickFile('.dcw,text/plain');
    if (!f) return;
    try {
      open(await importText(f.name, f.text));
    } catch (e) {
      setError(`${f.name}: ${(e as Error).message}`);
    }
  };
  const rename = async (id: string, old: string) => {
    const name = prompt('Rename document', old);
    if (!name) return;
    const d = await openDoc(id);
    d.doc.rename(name);
    await new Promise((r) => setTimeout(r, 50));
    d.close();
    refetch();
  };

  return (
    <div class="doclist">
      <h1>diagrammatic-calculator</h1>
      <p class="muted">
        <Show when={user()} fallback="Exact computations in diagrammatic algebras. Documents live in this browser; export them to keep a copy.">
          Exact computations in diagrammatic algebras. Documents are saved to your account and kept in this browser.
        </Show>
      </p>
      <Show when={notice()}>
        <div class="banner info" data-testid="notice" onClick={() => setNotice('')}>{notice()}</div>
      </Show>
      <Show when={persisted() === false && !user()}>
        <div class="banner">
          This browser may delete stored documents when it needs space (Safari does after about seven days without a visit). Installing the app or exporting documents keeps them safe.
        </div>
      </Show>
      <div class="row">
        <button class="primary" onClick={create}>New document</button>
        <button onClick={doImport}>Import .dcw…</button>
        <div class="grow" />
        <input placeholder="Search" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} aria-label="Search documents" />
        <AccountBar />
        <a class="button icon" href="#/settings" data-testid="open-settings" title="Settings" aria-label="Settings">
          <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
        </a>
      </div>
      <Show when={error()}><div class="error" style={{ 'margin-top': '8px' }}>{error()}</div></Show>
      <table>
        <thead><tr><th>Name<Show when={syncing()}> <span class="spinner" title="Syncing with your account" /></Show></th><th>Edited</th><th /></tr></thead>
        <tbody>
          <For each={shown()} fallback={<tr><td colspan="3" class="muted">No documents yet.</td></tr>}>
            {(d) => (
              <tr data-doc={d.id} data-cloud={d.cloud?.id}>
                <td class="open" onClick={() => open(d.id)}>
                  {d.name}
                  <Show when={where(d)}>{(w) => <span class="tag">{w()}</span>}</Show>
                </td>
                <td class="muted">{ago(d.edited)}</td>
                <td style={{ 'text-align': 'right' }}>
                  <button class="ghost" onClick={() => rename(d.id, d.name)}>Rename</button>
                  <button class="ghost" onClick={async () => { await duplicateDoc(d.id); refetch(); }}>Duplicate</button>
                  <button class="ghost" onClick={() => remove(d)}>Delete</button>
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <Show when={account().status === 'signed-in'}>
        <SharedWithMe docs={shared()} />
      </Show>
      <h3 style={{ 'margin-top': '28px' }}>Examples</h3>
      <div class="examples">
        <For each={Object.entries(examples)}>
          {([path, text]) => {
            const name = path.split('/').pop()!;
            return <button onClick={async () => open(await importText(name, text))}>{name.replace('.dcw', '')}</button>;
          }}
        </For>
      </div>
    </div>
  );
}
