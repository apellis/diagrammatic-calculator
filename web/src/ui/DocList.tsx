import { createEffect, createResource, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { createDoc, deleteDoc, duplicateDoc, isPersisted, listDocs, markExported, openDoc, type DocInfo } from '../doc/store';
import { exportDcw, fileName, fillFromHistory, isDch, parseDch, parseDcw } from '../doc/dcw';
import { ago, download, engine, pickFile } from '../app';
import { account, api, ours, syncAll, takeSignInOutcome, user } from '../account/account';
import { ApiError, type CloudDoc } from '../account/api';
import { AccountBar, SharedWithMe, signInMessage } from './Account';
import { NoteView } from './Results';
import { Dialog } from './Dialog';
import { ContextText } from './ContextChip';
import { ContextForm } from './ContextForm';
import './entry-ux.css';

const examples = import.meta.glob('../../../examples/*.dcw', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

const open = (id: string) => (location.hash = `#/doc/${id}`);

/** An example's name, context, and first line of description (its first comment or note). */
function describeExample(path: string, text: string) {
  const name = path.split('/').pop()!.replace(/\.dcw$/, '');
  const context = /^context (.*)$/m.exec(text)?.[1] ?? '';
  const first = /^(?:# (.*)|note """([^\n]*?)(?:"""|$))/m.exec(text);
  const about = (first?.[1] ?? first?.[2] ?? '').replace(/\.$/, '');
  return { name, context, about };
}

/** The storage warning can be put away for a week. */
const DISMISS_KEY = 'diagcalc.storage-note-dismissed';
const dismissedRecently = () => {
  try {
    return Date.now() - Number(localStorage.getItem(DISMISS_KEY) ?? 0) < 7 * 86400_000;
  } catch {
    return false;
  }
};

/**
 * Makes a document from a file: a workspace (.dcw), or a history (.dch), whose versions, snapshots,
 * and AI timeline come back as they were. Returns the new document's ID, and a note when part of a
 * history could not be read.
 */
export async function importText(name: string, text: string): Promise<{ id: string; note?: string }> {
  if (isDch(text)) {
    const h = await parseDch(text);
    const id = await createDoc(h.name ?? name.replace(/\.dch$/, ''), (d) => fillFromHistory(d, h));
    const n = h.skipped;
    return { id, note: n ? `${n} ${n === 1 ? 'entry' : 'entries'} of the history could not be read and ${n === 1 ? 'was' : 'were'} left out.` : undefined };
  }
  const ws = await parseDcw(text);
  return { id: await createDoc(name.replace(/\.dcw$/, ''), (d) => d.replace(ws.context, ws.cells, `import ${name}`)) };
}

/** What an import has to say about the document it made, shown when the document opens. */
export const importNotes = new Map<string, string>();

/** The files the import buttons offer. */
export const IMPORTABLE = '.dcw,.dch,text/plain,application/json';

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
  const [removing, setRemoving] = createSignal<DocInfo | null>(null);
  const [renaming, setRenaming] = createSignal<string | null>(null);
  const removeText = (d: DocInfo) => {
    const owned = inAccount(d) && d.cloud!.role === 'owner';
    return owned
      ? `Delete “${d.name}” from your account and this browser? This cannot be undone.`
      : d.cloud && d.cloud.role !== 'owner'
        ? `Remove “${d.name}” from this browser? It stays shared with you.`
        : d.cloud
          ? `Remove “${d.name}” from this browser? It stays in the account it was saved to.`
          : `Delete “${d.name}”? This cannot be undone.`;
  };
  const remove = async (d: DocInfo) => {
    setRemoving(null);
    const owned = inAccount(d) && d.cloud!.role === 'owner';
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

  /** The new-document dialog: a name and a context, checked by the engine before the document is made. */
  const [creating, setCreating] = createSignal<{ name: string; line: string } | null>(null);
  const [creatingBusy, setCreatingBusy] = createSignal(false);
  let disposed = false;
  onCleanup(() => { disposed = true; });
  const create = async () => {
    const c = creating();
    if (!c || creatingBusy()) return;
    setCreatingBusy(true);
    try {
      const o = await engine.call('context.describe', { context: c.line }, { lane: 'interactive' });
      if (disposed || creating() !== c || o.status !== 'done' || o.response.status !== 'ok') return; // the form shows why
      const context = o.response.context;
      setCreating(null);
      open(await createDoc(c.name.trim() || 'Untitled', (d) => d.setContext(context)));
    } finally {
      if (!disposed) setCreatingBusy(false);
    }
  };
  /** An example opened again gets a numbered name, so two copies are told apart. */
  const openExample = async (name: string, text: string) => {
    const names = new Set((docs() ?? []).map((d) => d.name));
    let n = name;
    for (let k = 2; names.has(n); k++) n = `${name} (${k})`;
    const ws = await parseDcw(text);
    open(await createDoc(n, (d) => d.replace(ws.context, ws.cells, `import ${name}.dcw`)));
  };
  const [storageDismissed, setStorageDismissed] = createSignal(dismissedRecently());
  const dismissStorage = () => {
    setStorageDismissed(true);
    try {
      localStorage.setItem(DISMISS_KEY, String(Date.now()));
    } catch {
      /* storage unavailable: dismissed for this visit */
    }
  };
  const doImport = async () => {
    const f = await pickFile(IMPORTABLE);
    if (!f) return;
    try {
      const r = await importText(f.name, f.text);
      if (r.note) importNotes.set(r.id, r.note);
      open(r.id);
    } catch (e) {
      setError(`${f.name}: ${(e as Error).message}`);
    }
  };
  const [exporting, setExporting] = createSignal<string | null>(null);
  const exportWorkspace = async (info: DocInfo) => {
    if (exporting()) return;
    setExporting(info.id);
    try {
      const opened = await openDoc(info.id);
      try {
        const { text, canonical } = await exportDcw(opened.doc);
        download(fileName(opened.doc.name, 'dcw'), text);
        await markExported(info.id);
        setNotice(canonical ? 'Workspace exported.' : 'Exported as entered: some cells do not parse.');
      } finally {
        opened.close();
      }
    } catch (e) {
      setError(`Could not export ${info.name}: ${(e as Error).message}`);
    } finally {
      setExporting(null);
    }
  };
  const rename = async (id: string, old: string, name: string) => {
    setRenaming(null);
    if (!name.trim() || name === old) return;
    const d = await openDoc(id);
    d.doc.rename(name);
    await new Promise((r) => setTimeout(r, 50));
    d.close();
    refetch();
  };

  return (
    <div class="doclist">
      <div class="row titlebar">
        <h1>diagrammatic-calculator</h1>
        <div class="grow" />
        <AccountBar
          onSignedOut={(r) => {
            refetch();
            if (!r.cleared) return;
            const n = (k: number) => `${k} ${k === 1 ? 'document' : 'documents'}`;
            setNotice(`Removed ${n(r.removed)} of the account from this browser.${r.kept ? ` Kept ${n(r.kept)} with changes the account does not have; export or delete ${r.kept === 1 ? 'it' : 'them'} if this computer is shared.` : ''}`);
          }}
        />
        <a class="button icon" href="#/about" data-testid="open-about" title="About" aria-label="About">
          <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="9.5" />
            <path d="M12 11v6" />
            <circle cx="12" cy="7.5" r="0.6" fill="currentColor" />
          </svg>
        </a>
        <a class="button icon" href="#/settings" data-testid="open-settings" title="Settings" aria-label="Settings">
          <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
        </a>
      </div>
      <p class="muted">
        <Show when={user()} fallback="Exact computations in diagrammatic algebras. Documents live in this browser; export them to keep a copy.">
          Exact computations in diagrammatic algebras. Documents are saved to your account and kept in this browser.
        </Show>
      </p>
      <details class="storage-help" data-testid="storage-help">
        <summary>{user() ? 'Saved to your account and kept in this browser' : 'Stored in this browser'} · where documents are kept</summary>
        <ul>
          <li>Every document has a working copy in this browser, so it opens at once and works offline.</li>
          <li>
            When you are signed in, each of your documents also has a copy in your account. The two copies are merged edit by edit, so
            neither overwrites the other: when the document list opens, shortly after each change, and every minute while a document
            is open. Other devices you sign in on get the same documents.
          </li>
          <li>
            Documents made while signed out are saved to your account the next time you sign in. The “Saved … from this browser”
            notice appears only when that happens; a visit with nothing new to upload shows no notice.
          </li>
          <li>
            Signing out, or removing a document from this browser, keeps the account copy. Deleting a document you own removes both
            copies. On a shared computer, Sign out can also remove the account's documents from this browser.
          </li>
          <li>Without an account, documents live only in this browser; export them to keep a copy elsewhere.</li>
          <li data-testid="install-help">
            Some browsers delete a site's data when they need space or when it has not been visited for a while (Safari after about
            seven days). Installed as an app, the calculator keeps its documents: in Chrome or Edge, the install button at the right
            of the address bar (or Install in the browser's menu); in Safari on a Mac, File › Add to Dock; on an iPhone or iPad, Share ›
            Add to Home Screen. Firefox does not install sites; export documents there to keep a copy.
          </li>
        </ul>
      </details>
      <Show when={notice()}>
        <div class="banner info" data-testid="notice" onClick={() => setNotice('')}>{notice()}</div>
      </Show>
      <Show when={persisted() === false && !user() && (docs()?.length ?? 0) > 0 && !storageDismissed()}>
        <div class="banner row" data-testid="storage-banner">
          <span class="grow">
            This browser may delete stored documents when it needs space (Safari does after about seven days without a visit). Export
            them, or install the calculator as an app (how: “where documents are kept” above), to keep them.
          </span>
          <button class="ghost" onClick={dismissStorage} aria-label="Dismiss for a week" title="Dismiss for a week">×</button>
        </div>
      </Show>
      <div class="row toolbar">
        <button class="primary" onClick={() => setCreating({ name: 'Untitled', line: 'klr type=A2 nu=2i+j ring=QQ' })}>New document</button>
        <button onClick={doImport} title="A workspace (.dcw), or a history (.dch) with its versions">Import .dcw or .dch…</button>
        <div class="grow" />
        <input placeholder="Search" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} aria-label="Search documents" />
      </div>
      <Show when={error()}><div class="error" style={{ 'margin-top': '8px' }}>{error()}</div></Show>
      <table class="documents">
        <thead><tr><th>Name<Show when={syncing()}> <span class="spinner" title="Syncing with your account" /></Show></th><th>Edited</th><th /></tr></thead>
        <tbody>
          <For each={shown()} fallback={
            <tr><td colspan="3" class="muted">
              <Show when={query()} fallback={
                <div class="empty" data-testid="empty">
                  <strong>No documents yet.</strong> Start a new document, import a <code>.dcw</code> workspace, or open one of the examples
                  below and change it.
                </div>
              }>
                No document matches “{query()}”.
              </Show>
            </td></tr>
          }>
            {(d) => (
              <tr data-doc={d.id} data-cloud={d.cloud?.id}>
                <Show when={renaming() === d.id} fallback={
                  <td class="open" onClick={(e) => (e.target as HTMLElement).closest('a') || open(d.id)}>
                    <a href={`#/doc/${d.id}`}>{d.name}</a>
                    <Show when={where(d)}>{(w) => <span class="tag">{w()}</span>}</Show>
                  </td>
                }>
                  <td>
                    <input value={d.name} aria-label="Document name" ref={(el) => queueMicrotask(() => { el.focus(); el.select(); })}
                      onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void rename(d.id, d.name, e.currentTarget.value); } if (e.key === 'Escape') setRenaming(null); }}
                      onBlur={(e) => renaming() === d.id && void rename(d.id, d.name, e.currentTarget.value)} />
                  </td>
                </Show>
                <td class="muted">{ago(d.edited)}</td>
                <td class="actions">
                  <button class="ghost" onClick={() => setRenaming(d.id)}>Rename</button>
                  <button class="ghost" onClick={async () => { await duplicateDoc(d.id); refetch(); }}>Duplicate</button>
                  <button class="ghost" disabled={!!exporting()} title="Download workspace (.dcw); for full history, open the document and use Commands" onClick={() => void exportWorkspace(d)}>{exporting() === d.id ? 'Exporting...' : 'Export'}</button>
                  <button class="ghost" onClick={() => setRemoving(d)}>Delete</button>
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <Show when={account().status === 'signed-in'}>
        <SharedWithMe docs={shared()} />
      </Show>
      <Show when={removing()}>
        {(d) => (
          <Dialog label="Delete the document" class="small" onClose={() => setRemoving(null)}>
            <h3 style={{ margin: '0 0 8px' }}>Delete “{d().name}”?</h3>
            <p class="hint" style={{ margin: '0 0 10px' }}>{removeText(d())}</p>
            <div class="row">
              <button class="danger" onClick={() => void remove(d())}>Delete</button>
              <button onClick={() => setRemoving(null)}>Keep it</button>
            </div>
          </Dialog>
        )}
      </Show>
      <Show when={creating()}>
        {(c) => (
          <Dialog label="New document" class="new-doc" onClose={() => setCreating(null)}>
            <h3 style={{ margin: '0 0 10px' }}>New document</h3>
            <label class="ctx-name-field">
              <span>Name</span>
              <input value={c().name} aria-label="Name" onInput={(e) => setCreating({ ...c(), name: e.currentTarget.value })}
                onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void create(); } }} />
            </label>
            <ContextForm line={c().line} onLine={(line) => setCreating({ ...c(), line })} onSubmit={() => void create()} />
            <details class="ctx-advanced">
              <summary>Context line</summary>
              <input class="mono" value={c().line} aria-label="Context line" onInput={(e) => setCreating({ ...c(), line: e.currentTarget.value })}
                onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void create(); } }} />
              <span class="hint">The presentation, then its options, as in a <code>.dcw</code> file; for example <code>be2 type=A1 odd=i mu=-1</code>, or a finite quotient with <code>dotcap=3</code> or <code>degcap=4</code>.</span>
            </details>
            <div class="row" style={{ 'margin-top': '12px', 'justify-content': 'flex-end' }}>
              <button onClick={() => setCreating(null)}>Cancel</button>
              <button class="primary" disabled={creatingBusy()} onClick={() => void create()}>Create</button>
            </div>
          </Dialog>
        )}
      </Show>
      <h3 style={{ 'margin-top': '28px' }}>Start from an example</h3>
      <p class="muted">Each example opens as your own editable copy. The original stays unchanged. <a href="#/about">Getting started</a></p>
      <div class="examples">
        <For each={Object.entries(examples)}>
          {([path, text]) => {
            const ex = describeExample(path, text);
            return (
              <button class="example" onClick={() => openExample(ex.name, text)} data-example={ex.name}>
                <span class="example-name">{ex.name}</span>
                <span class="example-ctx"><ContextText context={ex.context} /></span>
                <Show when={ex.about}><span class="example-about"><NoteView text={ex.about} inline /></span></Show>
              </button>
            );
          }}
        </For>
      </div>
    </div>
  );
}
