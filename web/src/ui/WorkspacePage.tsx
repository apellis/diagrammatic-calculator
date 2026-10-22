// A document's workspace: top bar, outline and cells, kept in step with the document and the
// evaluator.
import { createResource, createSignal, For, onCleanup, onMount, Show, type Signal } from 'solid-js';
import { openDoc, markExported, type OpenDoc } from '../doc/store';
import { kindOf, type CellData, type Comparison, type Doc, type Inputs } from '../doc/model';
import { Evaluator, type CellState } from '../eval/evaluator';
import { cache, download, engine, engineId, isMac, mod, pickFile } from '../app';
import { exportDch, exportDcw, fileName } from '../doc/dcw';
import { Cell } from './Cell';
import { Outline } from './Outline';
import { ContextChip } from './ContextChip';
import { Palette, type Command } from './Palette';
import { importText } from './DocList';
import { Canvas, type Generator, type Selection } from './Canvas';

export function WorkspacePage(props: { id: string }) {
  const [od] = createResource(() => props.id, openDoc);
  return (
    <Show when={od()} fallback={<div class="doclist">{od.error ? `Cannot open this document: ${od.error.message}` : 'Opening…'}</div>}>
      {(o) => <Workspace o={o()} />}
    </Show>
  );
}

/** Scratch branches, as the workspace view sees them. */
export interface Branching {
  /** The branch shown, or null for the document itself. */
  current: { id: string; name: string } | null;
  list: { id: string; name: string }[];
  /** Starts a branch from what is shown, or with the given inputs (a version opened as a branch). */
  create: (name?: string, inputs?: Inputs) => string;
  open: (id: string | null) => void;
  compare: () => Comparison | null;
  accept: () => Comparison | null;
  discard: () => void;
}

/** The document, or one of its branches, shown in the workspace view. */
function Workspace(props: { o: OpenDoc }) {
  const main = props.o.doc;
  const linked = createSignal<FileSystemFileHandle | null>(null);
  const [current, setCurrent] = createSignal<{ id: string; name: string; doc: Doc } | null>(null);
  const [list, setList] = createSignal([...main.branches.values()].map(({ id, name }) => ({ id, name })));
  const onBranches = () => setList([...main.branches.values()].map(({ id, name }) => ({ id, name })));
  main.branches.observe(onBranches);

  // a branch's changes are kept in the document shortly after each step
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let unwatch: (() => void) | undefined;
  const flush = () => {
    clearTimeout(saveTimer);
    const c = current();
    if (c && main.branches.has(c.id)) main.saveBranch(c.id, c.doc);
  };
  const show = (b: { id: string; name: string; doc: Doc } | null) => {
    flush();
    unwatch?.();
    unwatch = undefined;
    if (b) {
      const onUpdate = () => {
        clearTimeout(saveTimer);
        saveTimer = setTimeout(flush, 300);
      };
      b.doc.ydoc.on('update', onUpdate);
      unwatch = () => b.doc.ydoc.off('update', onUpdate);
    }
    setCurrent(b);
  };
  const branching: Branching = {
    get current() {
      const c = current();
      return c && { id: c.id, name: c.name };
    },
    get list() {
      return list();
    },
    create: (name, inputs) => {
      const base = current()?.doc ?? main;
      if (base !== main) flush();
      const b = main.createBranch(name ?? `what if ${main.branches.size + 1}`);
      // a branch of a branch starts from the branch's state but merges into the document
      if (base !== main) main.saveBranch(b.id, base);
      const d = main.openBranch(b.id)!;
      if (inputs) d.setInputsStep(inputs, `start from ${b.name}`);
      show({ id: b.id, name: b.name, doc: d });
      main.saveBranch(b.id, d);
      return b.id;
    },
    open: (id) => {
      const d = id ? main.openBranch(id) : null;
      show(id && d ? { id, name: main.branches.get(id)!.name, doc: d } : null);
    },
    compare: () => {
      const c = current();
      return c ? main.compareBranch(c.id, c.doc) : null;
    },
    accept: () => {
      const c = current();
      if (!c) return null;
      flush();
      show(null);
      return main.acceptBranch(c.id, c.doc);
    },
    discard: () => {
      const c = current();
      if (!c) return;
      show(null);
      main.discardBranch(c.id);
    },
  };
  onCleanup(() => {
    flush();
    unwatch?.();
    main.branches.unobserve(onBranches);
    props.o.close();
  });
  return (
    <Show when={current()?.doc ?? main} keyed>
      {(doc) => <DocView o={props.o} doc={doc} branching={branching} linked={linked} />}
    </Show>
  );
}

const K = isMac ? '⌘' : 'Ctrl+';

function DocView(props: { o: OpenDoc; doc: Doc; branching: Branching; linked: Signal<FileSystemFileHandle | null> }) {
  const doc = props.doc;
  const main = props.o.doc;
  const onBranch = () => props.branching.current !== null;
  const [cells, setCells] = createSignal<CellData[]>(doc.list());
  const [context, setContext] = createSignal(doc.context);
  const [name, setName] = createSignal(doc.name);
  const [versions, setVersions] = createSignal(doc.versions());
  const [snapshots, setSnapshots] = createSignal(main.snapshots.toArray());
  const [canUndo, setCanUndo] = createSignal(doc.undoManager.canUndo());
  const [canRedo, setCanRedo] = createSignal(doc.undoManager.canRedo());
  const [jobs, setJobs] = createSignal(0);
  const [expanded, setExpanded] = createSignal(new Set<string>());
  const [active, setActive] = createSignal<string | null>(null);
  const [adding, setAdding] = createSignal<number | null>(null);
  const [outline, setOutline] = createSignal(true);
  const [modal, setModal] = createSignal<null | 'palette' | 'keys' | 'branch' | { version: number } | { snapshot: number }>(null);
  const [linked, setLinked] = props.linked;
  const [message, setMessage] = createSignal('');
  const [canvas, setCanvas] = createSignal<{ id: string; word?: string } | null>(null);
  const [hintsVersion, setHintsVersion] = createSignal(0);
  const [, setSelection] = createSignal<Selection | null>(null);

  // cell states, one signal per cell so only changed cells re-render
  const states = new Map<string, Signal<CellState | undefined>>();
  const slot = (id: string) => {
    let s = states.get(id);
    if (!s) states.set(id, (s = createSignal<CellState | undefined>(undefined, { equals: false })));
    return s;
  };
  const stateOf = (id: string) => slot(id)[0]();
  const scalars = () => [...new Set(cells().flatMap((c) => stateOf(c.id)?.parsed?.unknowns ?? []))].sort();
  /** The context with the workspace's scalars, for operations on a single term. */
  const termContext = () => {
    const sc = scalars();
    return sc.length && !/\bscalars=/.test(context()) ? `${context()} scalars=${sc.join(',')}` : context();
  };
  const [described] = createResource(context, async (c) => {
    const o = await engine.call('context.describe', { context: c }, { lane: 'interactive' });
    if (o.status !== 'done' || o.response.status !== 'ok') return { generators: [] as Generator[], colours: [] as string[] };
    return { generators: o.response.generators, colours: o.response.colours.map((x) => x.name) };
  });

  let ev: Evaluator | undefined;
  const refresh = () => {
    // keep each unchanged cell's object, so its component stays as it is
    const old = new Map(cells().map((c) => [c.id, c]));
    setCells(doc.list().map((c) => (old.get(c.id)?.text === c.text ? old.get(c.id)! : c)));
    setContext(doc.context);
    setName(doc.name);
    setCanUndo(doc.undoManager.canUndo());
    setCanRedo(doc.undoManager.canRedo());
    void ev?.update(doc.context, doc.list());
    scheduleFileSave();
  };
  const onHistory = () => setVersions(doc.versions());
  const onSnapshots = () => setSnapshots(main.snapshots.toArray());
  const onHints = () => setHintsVersion(hintsVersion() + 1);
  doc.cells.observeDeep(refresh);
  doc.meta.observe(refresh);
  doc.history.observe(onHistory);
  main.snapshots.observe(onSnapshots);
  doc.hints.observe(onHints);
  engine.onLoad = setJobs;
  engineId.then((id) => {
    ev = new Evaluator(engine, cache, { engineId: id }, (s) => slot(s.id)[1](s));
    refresh();
  });
  if (!doc.cells.length) setAdding(0);

  // saving straight to a file on disk (Chrome, Edge)
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  function scheduleFileSave() {
    const h = linked();
    if (!h || onBranch()) return;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      const w = await h.createWritable();
      await w.write((await exportDcw(doc)).text);
      await w.close();
      void markExported(props.o.info.id);
    }, 800);
  }

  const toggle = (id: string) => {
    if (opened === id) opened = null;
    const s = new Set(expanded());
    if (s.has(id) && active() === id) {
      s.delete(id);
      setActive(null);
    } else {
      s.add(id);
      setActive(id);
    }
    setExpanded(s);
  };
  // cells opened by moving to them close again when the next one opens; cells opened by a click stay open
  let opened: string | null = null;
  const focusCell = (id: string) => {
    const s = new Set(expanded());
    if (opened && opened !== id) s.delete(opened);
    if (!s.has(id)) opened = id;
    s.add(id);
    setExpanded(s);
    setActive(id);
    requestAnimationFrame(() => document.getElementById(`cell-${id}`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }));
  };

  /** A bare term becomes a definition with a fresh name. */
  const normalize = (text: string): string => {
    const t = text.trim();
    if (!t || kindOf(t) || t.startsWith('/')) return t;
    const used = new Set(cells().map((c) => stateOf(c.id)?.parsed?.defines));
    let n = 1;
    while (used.has(`t${n}`)) n++;
    return `def t${n} := ${t}`;
  };
  const commitNew = (index: number, text: string) => {
    const t = normalize(text);
    setAdding(null);
    if (!t || t.startsWith('/')) return;
    const id = doc.addCell(t, index);
    focusCell(id);
  };
  const commit = (id: string, text: string) => {
    const t = normalize(text);
    if (t.startsWith('/')) return;
    if (!t) doc.removeCell(id);
    else doc.updateCell(id, t);
  };

  const pin = (id: string) => {
    const s = stateOf(id);
    if (!s?.value || !s.valueKey) return;
    const label = prompt('Name this snapshot', s.parsed?.defines ?? id);
    if (!label) return;
    main.pin({ name: label, cellId: id, cellText: s.text, context: context(), version: versions().length, key: s.valueKey, value: s.value, time: Date.now() });
    setMessage(`Pinned “${label}”.`);
  };
  const copyTikz = async (id: string) => {
    const v = stateOf(id)?.value as { kind: string; normal_form?: string; term?: string } | undefined;
    const term = v?.kind === 'watch' ? v.normal_form : v?.term;
    if (!term) return;
    const o = await engine.call('term.render', { context: termContext(), term, format: 'tikz' }, { lane: 'interactive' });
    if (o.status === 'done' && o.response.status === 'ok') {
      await navigator.clipboard?.writeText(o.response.output);
      setMessage('TikZ copied.');
    }
  };

  // the focus canvas edits a definition's or watch's term as written; a term that uses
  // definitions is drawn written out, and a gesture writes it out in the cell
  const canvasable = (id: string) => ['def', 'watch'].includes(stateOf(id)?.parsed?.kind ?? '');
  const openCanvas = (id: string, word?: string) => {
    if (!canvasable(id)) return setMessage('The canvas opens on a definition or a watch.');
    setActive(id);
    setCanvas({ id, word });
  };
  const usesDefinitions = (id: string) => {
    const defined = new Set(cells().map((c) => stateOf(c.id)?.parsed?.defines).filter(Boolean));
    return (stateOf(id)?.parsed?.uses ?? []).some((u) => defined.has(u));
  };
  const canvasTerm = (id: string): string | null => {
    const s = stateOf(id);
    if (!s?.parsed?.term) return null;
    if (!usesDefinitions(id)) return s.parsed.term;
    const v = s.value as { status: string; term?: string } | undefined;
    return s.status === 'fresh' && v?.status === 'ok' && v.term ? v.term : null;
  };
  const canvasTitle = (id: string) => {
    const p = stateOf(id)?.parsed;
    return p?.kind === 'def' ? `def ${p.defines}` : `watch (${id})`;
  };
  const writeCanvas = (id: string, term: string, note: string) => {
    const p = stateOf(id)?.parsed;
    if (!p) return;
    const head = p.kind === 'def' ? `def ${p.defines} := ${term}` : `watch ${term}`;
    doc.updateCell(id, head + Object.entries(p.options).map(([k, v]) => `  ${k}=${v}`).join(''), `${note} in ${p.defines ?? id}`);
  };

  const newBranch = () => {
    props.branching.create();
  };
  const discardBranch = () => {
    const b = props.branching.current;
    if (b && confirm(`Discard the branch “${b.name}”? Its changes are lost.`)) props.branching.discard();
  };

  const exportFile = async () => {
    const { text, canonical } = await exportDcw(doc);
    download(fileName(name(), 'dcw'), text);
    void markExported(props.o.info.id);
    setMessage(canonical ? 'Exported.' : 'Exported as entered: some cells do not parse.');
  };
  const importFile = async () => {
    const f = await pickFile('.dcw,text/plain');
    if (f) location.hash = `#/doc/${await importText(f.name, f.text)}`;
  };
  const linkFile = async () => {
    const pick = (window as unknown as { showSaveFilePicker?: (o: unknown) => Promise<FileSystemFileHandle> }).showSaveFilePicker;
    if (!pick) return setMessage('This browser cannot save straight to a file; use Export instead.');
    const h = await pick({ suggestedName: fileName(name(), 'dcw'), types: [{ description: 'Workspace', accept: { 'text/plain': ['.dcw'] } }] });
    setLinked(h);
    scheduleFileSave();
    setMessage(`Saving to ${h.name} after every step while this page is open.`);
  };

  const commands = (): Command[] => [
    ...['def', 'watch', 'solve', 'check', 'note'].map((k) => ({ name: `New ${k} cell`, run: () => setAdding(cells().length) })),
    { name: 'Undo', keys: `${K}Z`, run: () => doc.undo() },
    { name: 'Redo', keys: isMac ? '⇧⌘Z' : 'Ctrl+Y', run: () => doc.redo() },
    { name: 'Export workspace (.dcw)', keys: isMac ? '⇧⌘S' : 'Ctrl+Shift+S', run: exportFile },
    { name: 'Export history (.dch)', run: () => download(fileName(name(), 'dch'), exportDch(doc), 'application/json') },
    { name: 'Import a workspace as a new document', keys: `${K}O`, run: importFile },
    { name: 'Save to a file on disk after every step', run: linkFile },
    { name: 'Start a scratch branch', keys: `${K}B`, run: () => newBranch() },
    ...(onBranch()
      ? [
          { name: 'Compare and accept this branch', run: () => setModal('branch') },
          { name: 'Discard this branch', run: discardBranch },
          { name: 'Back to the document', run: () => props.branching.open(null) },
        ]
      : props.branching.list.map((b) => ({ name: `Open branch “${b.name}”`, run: () => props.branching.open(b.id) }))),
    { name: 'Cancel running computations', run: () => ev?.cancelAll() },
    { name: 'Clear the result cache', run: () => void cache.clear() },
    { name: 'Show or hide the outline', run: () => setOutline(!outline()) },
    { name: 'Open the active cell in the canvas', keys: 'F', run: () => active() && openCanvas(active()!) },
    { name: 'Keyboard shortcuts', keys: '?', run: () => setModal('keys') },
    { name: 'All documents', run: () => (location.hash = '#/') },
  ];

  const onKey = (e: KeyboardEvent) => {
    const inEditor = (e.target as HTMLElement).closest?.('.cm-editor, input, textarea');
    if (mod(e) && e.key.toLowerCase() === 'k') { e.preventDefault(); setModal(modal() === 'palette' ? null : 'palette'); return; }
    if (mod(e) && e.shiftKey && e.key.toLowerCase() === 's') { e.preventDefault(); void exportFile(); return; }
    if (mod(e) && e.key.toLowerCase() === 'o') { e.preventDefault(); void importFile(); return; }
    if (mod(e) && e.key.toLowerCase() === 'b') { e.preventDefault(); newBranch(); return; }
    if (inEditor) return;
    if (mod(e) && e.key.toLowerCase() === 'z') { e.preventDefault(); e.shiftKey ? doc.redo() : doc.undo(); return; }
    if (mod(e) && e.key.toLowerCase() === 'y') { e.preventDefault(); doc.redo(); return; }
    if (e.key === '?') setModal('keys');
    if ((e.key === 'f' || e.key === 'F') && !mod(e) && active() && !canvas()) { e.preventDefault(); openCanvas(active()!); return; }
    if (e.key === 'Escape') { setModal(null); if (!canvas()) setActive(null); }
  };
  onMount(() => window.addEventListener('keydown', onKey));
  const um = doc.undoManager;
  const onStack = () => { setCanUndo(um.canUndo()); setCanRedo(um.canRedo()); };
  um.on('stack-item-added', onStack);
  um.on('stack-item-popped', onStack);
  onCleanup(() => {
    window.removeEventListener('keydown', onKey);
    doc.cells.unobserveDeep(refresh);
    doc.meta.unobserve(refresh);
    doc.history.unobserve(onHistory);
    main.snapshots.unobserve(onSnapshots);
    doc.hints.unobserve(onHints);
    um.off('stack-item-added', onStack);
    um.off('stack-item-popped', onStack);
    ev?.cancelAll();
  });

  const reminder = () => {
    const i = props.o.info;
    const since = i.exported ?? i.created;
    return i.edited > since && Date.now() - since > 7 * 86400_000;
  };

  const NewCell = (p: { index: number }) => (
    <Cell id="new" text="" expanded active isNew onToggle={() => setAdding(null)}
      onCommit={(t) => commitNew(p.index, t)} onCancel={() => setAdding(null)} onRemove={() => setAdding(null)}
      onMove={() => {}} onPin={() => {}} onRerun={() => {}} onCopyTikz={() => {}} />
  );

  return (
    <>
      <header class="topbar">
        <button class="ghost" title="All documents" onClick={() => (location.hash = '#/')}>←</button>
        <button class="ghost" title="Show or hide the outline" onClick={() => setOutline(!outline())}>☰</button>
        <span class="name" title="Rename" onClick={() => { const n = prompt('Rename document', name()); if (n) doc.rename(n); }}>{name()}</span>
        <ContextChip context={context()} scalars={scalars()} onSet={(c) => doc.setContext(c)} />
        <Show when={props.branching.current} fallback={
          <Show when={props.branching.list.length}>
            <select class="branches" aria-label="Branches" onChange={(e) => { if (e.currentTarget.value) props.branching.open(e.currentTarget.value); }}>
              <option value="">{props.branching.list.length} {props.branching.list.length === 1 ? 'branch' : 'branches'}</option>
              <For each={props.branching.list}>{(b) => <option value={b.id}>{b.name}</option>}</For>
            </select>
          </Show>
        }>
          {(b) => (
            <span class="row branch" data-testid="branch">
              <span class="branch-name" title="A scratch branch: changes here leave the document alone until you accept them">⑂ {b().name}</span>
              <button class="primary" onClick={() => setModal('branch')}>Accept…</button>
              <button onClick={discardBranch}>Discard</button>
              <button class="ghost" title="Back to the document; the branch is kept" onClick={() => props.branching.open(null)}>Leave</button>
            </span>
          )}
        </Show>
        <div class="grow" />
        <Show when={message()}><span class="hint" onClick={() => setMessage('')}>{message()}</span></Show>
        <Show when={linked() && !onBranch()}><span class="hint" title="Saved after every step">↔ {linked()!.name}</span></Show>
        <button class="ghost" title={`Undo (${K}Z)`} disabled={!canUndo()} onClick={() => doc.undo()} aria-label="Undo">↶</button>
        <button class="ghost" title="Redo" disabled={!canRedo()} onClick={() => doc.redo()} aria-label="Redo">↷</button>
        <span class={`light ${jobs() ? 'busy' : ''}`} title={jobs() ? `${jobs()} computing; click to cancel` : 'Idle'} data-testid="light"
          onClick={() => jobs() && ev?.cancelAll()} />
        <button class="ghost" onClick={() => setModal('palette')} title={`Commands (${K}K)`}>⋯</button>
      </header>
      <div class="layout" classList={{ 'no-outline': !outline() }}>
        <Show when={outline()} fallback={<div />}>
          <Outline cells={cells()} state={stateOf} active={active()} versions={versions()} snapshots={snapshots()}
            onSelect={focusCell} onVersion={(n) => setModal({ version: n })} onSnapshot={(i) => setModal({ snapshot: i })} />
        </Show>
        <div class="stage">
        <main class="main">
          <div class="cells">
            <Show when={props.branching.current}>
              {(b) => <div class="banner branch-banner">You are on the scratch branch “{b().name}”. Changes here leave the document alone until you accept them.</div>}
            </Show>
            <Show when={reminder()}>
              <div class="banner">This document changed and has not been exported for a week. <a href="#" onClick={(e) => { e.preventDefault(); void exportFile(); }}>Export it</a> to keep a copy.</div>
            </Show>
            <For each={cells()}>
              {(c, i) => (
                <>
                  <Show when={adding() === i()}><NewCell index={i()} /></Show>
                  <div class="insert"><button onClick={() => setAdding(i())}>+ cell</button></div>
                  <Show when={c.text} keyed>
                    {(text) => (
                      <Cell id={c.id} text={text} state={stateOf(c.id)} expanded={expanded().has(c.id)} active={active() === c.id}
                        onToggle={() => toggle(c.id)} onCommit={(t) => commit(c.id, t)} onCancel={() => setActive(null)}
                        onRemove={() => doc.removeCell(c.id)} onMove={(d) => doc.moveCell(c.id, i() + d)}
                        onPin={() => pin(c.id)} onRerun={() => ev?.rerun(c.id)} onCopyTikz={() => copyTikz(c.id)}
                        onCanvas={(w) => openCanvas(c.id, w)} />
                    )}
                  </Show>
                </>
              )}
            </For>
            <Show when={adding() !== null && adding()! >= cells().length}><NewCell index={cells().length} /></Show>
            <div style={{ 'margin-top': '10px' }}>
              <button onClick={() => setAdding(cells().length)} data-testid="add-cell">+ New cell</button>
              <span class="hint" style={{ 'margin-left': '10px' }}>Type a term to define it, or <kbd>/</kbd> for a cell kind; <kbd>Enter</kbd> commits.</span>
            </div>
          </div>
        </main>
        <Show when={canvas()?.id} keyed>
          {(id) => (
            <Canvas doc={doc} engine={engine} cellId={id} title={canvasTitle(id)} context={termContext()}
              term={canvasTerm(id)} expanded={usesDefinitions(id)} generators={described()?.generators ?? []}
              colours={described()?.colours ?? []} hintsVersion={hintsVersion()} initialWord={canvas()?.word}
              onWrite={(t, note) => writeCanvas(id, t, note)} onSelection={setSelection}
              onClose={() => { setCanvas(null); focusCell(id); }} />
          )}
        </Show>
        </div>
      </div>
      <Show when={modal() === 'palette'}><Palette commands={commands()} onClose={() => setModal(null)} /></Show>
      <Show when={modal() === 'keys'}><Keys onClose={() => setModal(null)} /></Show>
      <Show when={modal() === 'branch' && props.branching.compare()} keyed>
        {(cmp) => (
          <BranchView name={props.branching.current?.name ?? ''} cmp={cmp} onClose={() => setModal(null)}
            onAccept={() => { setModal(null); const r = props.branching.accept(); if (r) setMessage(`Accepted: ${describe(r)}.`); }}
            onDiscard={() => { setModal(null); discardBranch(); }} />
        )}
      </Show>
      <Show when={(() => { const m = modal(); return m && typeof m === 'object' && 'version' in m ? m.version : null; })()} keyed>
        {(n) => <VersionView n={n} at={doc.at(n)} dcw={(s) => doc.toDcw(s)} onClose={() => setModal(null)}
          onRestore={(s) => { doc.replace(s.context, s.cells, `restore version ${n}`); setModal(null); }}
          onBranch={(s) => {
            setModal(null);
            props.branching.create(`version ${n}`, { ...s, hints: doc.inputs().hints });
          }} />}
      </Show>
      <Show when={(() => { const m = modal(); return m && typeof m === 'object' && 'snapshot' in m ? snapshots()[m.snapshot] : null; })()} keyed>
        {(s) => (
          <div class="modal-bg" onClick={() => setModal(null)}>
            <div class="modal" onClick={(e) => e.stopPropagation()}>
              <h3 style={{ margin: '0 0 8px' }}>{s.name}</h3>
              <table class="kv"><tbody>
                <tr><td>cell</td><td class="mono">{s.cellText}</td></tr>
                <tr><td>context</td><td class="mono">{s.context}</td></tr>
                <tr><td>version</td><td>{s.version}, {new Date(s.time).toLocaleString()}</td></tr>
                <tr><td>inputs</td><td class="mono">{s.key}</td></tr>
              </tbody></table>
              <pre class="textform" style={{ 'margin-top': '8px' }}>{JSON.stringify(stripSvg(s.value), null, 2)}</pre>
              <button onClick={() => setModal(null)}>Close</button>
            </div>
          </div>
        )}
      </Show>
    </>
  );
}

/** A snapshot's value without its drawings, for reading. */
function stripSvg(v: unknown): unknown {
  return JSON.parse(JSON.stringify(v, (k, x) => (k === 'svg' ? undefined : x)));
}

/** One line saying what accepting a branch changes. */
function describe(c: Comparison): string {
  const parts = [
    c.context && 'the context',
    c.changed.length && `${c.changed.length} changed`,
    c.added.length && `${c.added.length} added`,
    c.removed.length && `${c.removed.length} removed`,
    c.hints && `${c.hints} drawing${c.hints === 1 ? '' : 's'}`,
  ].filter(Boolean);
  return parts.length ? parts.join(', ') : 'no changes';
}

function BranchView(props: { name: string; cmp: Comparison; onClose: () => void; onAccept: () => void; onDiscard: () => void }) {
  const c = () => props.cmp;
  return (
    <div class="modal-bg" onClick={props.onClose}>
      <div class="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-label={`Branch ${props.name}`}>
        <h3 style={{ margin: '0 0 8px' }}>Accept “{props.name}” into the document?</h3>
        <p class="hint" style={{ margin: '0 0 8px' }}>Accepting is one step: undo takes it back. It changes {describe(c())}.</p>
        <Show when={c().conflicts.length}>
          <div class="banner">The document also changed {c().conflicts.join(', ')} since the branch was made; accepting takes the branch's text there.</div>
        </Show>
        <pre class="textform" data-testid="branch-diff">{[
          ...(c().context ? [`context ${c().context!.from} → ${c().context!.to}`] : []),
          ...c().changed.flatMap((x) => [`~ ${x.id}: ${x.from}`, `  → ${x.to}`]),
          ...c().added.map((x) => `+ ${x.text}`),
          ...c().removed.map((x) => `− ${x.text}`),
          ...(c().hints ? [`${c().hints} drawing ${c().hints === 1 ? 'hint' : 'hints'}`] : []),
        ].join('\n') || 'The branch changes nothing.'}</pre>
        <div class="row" style={{ 'margin-top': '10px' }}>
          <button class="primary" onClick={props.onAccept}>Accept</button>
          <button onClick={props.onClose}>Keep working on the branch</button>
          <div class="grow" />
          <button onClick={props.onDiscard}>Discard the branch</button>
        </div>
      </div>
    </div>
  );
}

function VersionView(props: {
  n: number;
  at: { context: string; cells: CellData[] } | null;
  dcw: (s: { context: string; cells: CellData[] }) => string;
  onClose: () => void;
  onRestore: (s: { context: string; cells: CellData[] }) => void;
  onBranch: (s: { context: string; cells: CellData[] }) => void;
}) {
  return (
    <div class="modal-bg" onClick={props.onClose}>
      <div class="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-label={`Version ${props.n}`}>
        <h3 style={{ margin: '0 0 8px' }}>Version {props.n} (read-only)</h3>
        <Show when={props.at} fallback={<div class="error">This version cannot be read.</div>}>
          {(s) => (
            <>
              <pre class="textform">{props.dcw(s())}</pre>
              <div class="row" style={{ 'margin-top': '10px' }}>
                <button class="primary" onClick={() => props.onRestore(s())}>Restore this version</button>
                <button onClick={() => props.onBranch(s())}>Open as a branch</button>
                <button onClick={props.onClose}>Close</button>
              </div>
            </>
          )}
        </Show>
      </div>
    </div>
  );
}

function Keys(props: { onClose: () => void }) {
  const rows: [string, string][] = [
    [`${K}Z, ${isMac ? '⇧⌘Z' : 'Ctrl+Shift+Z or Ctrl+Y'}`, 'Undo, redo (outside a cell editor)'],
    [`${K}K`, 'Command palette'],
    [`Enter or ${K}Enter`, 'Commit the cell being edited (Enter adds a line in a note)'],
    ['Esc', 'Stop editing, close a dialog'],
    [`${K}O, ${isMac ? '⇧⌘S' : 'Ctrl+Shift+S'}`, 'Import a workspace, export this one'],
    [`${K}B`, 'Start a scratch branch: try something, then accept or discard it'],
    ['/', 'At the start of a new cell: choose its kind'],
    ['F', 'Open the active definition or watch in the focus canvas (? there lists its gestures)'],
    ['?', 'This sheet'],
  ];
  return (
    <div class="modal-bg" onClick={props.onClose}>
      <div class="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Keyboard shortcuts">
        <h3 style={{ margin: '0 0 8px' }}>Keyboard shortcuts</h3>
        <table class="kv"><tbody>
          <For each={rows}>{([k, v]) => <tr><td><kbd>{k}</kbd></td><td>{v}</td></tr>}</For>
        </tbody></table>
      </div>
    </div>
  );
}
