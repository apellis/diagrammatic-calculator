import { createResource, createSignal, For, Show } from 'solid-js';
import { createDoc, deleteDoc, duplicateDoc, isPersisted, listDocs, openDoc } from '../doc/store';
import { parseDcw } from '../doc/dcw';
import { ago, pickFile } from '../app';

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
      <p class="muted">Exact computations in diagrammatic algebras. Documents live in this browser; export them to keep a copy.</p>
      <Show when={persisted() === false}>
        <div class="banner">
          This browser may delete stored documents when it needs space (Safari does after about seven days without a visit). Installing the app or exporting documents keeps them safe.
        </div>
      </Show>
      <div class="row">
        <button class="primary" onClick={create}>New document</button>
        <button onClick={doImport}>Import .dcw…</button>
        <div class="grow" />
        <input placeholder="Search" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} aria-label="Search documents" />
        <a class="button icon" href="#/settings" data-testid="open-settings" title="Settings" aria-label="Settings">
          <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
        </a>
      </div>
      <Show when={error()}><div class="error" style={{ 'margin-top': '8px' }}>{error()}</div></Show>
      <table>
        <thead><tr><th>Name</th><th>Edited</th><th /></tr></thead>
        <tbody>
          <For each={shown()} fallback={<tr><td colspan="3" class="muted">No documents yet.</td></tr>}>
            {(d) => (
              <tr data-doc={d.id}>
                <td class="open" onClick={() => open(d.id)}>{d.name}</td>
                <td class="muted">{ago(d.edited)}</td>
                <td style={{ 'text-align': 'right' }}>
                  <button class="ghost" onClick={() => rename(d.id, d.name)}>Rename</button>
                  <button class="ghost" onClick={async () => { await duplicateDoc(d.id); refetch(); }}>Duplicate</button>
                  <button class="ghost" onClick={async () => { if (confirm(`Delete “${d.name}”? This cannot be undone.`)) { await deleteDoc(d.id); refetch(); } }}>Delete</button>
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
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
