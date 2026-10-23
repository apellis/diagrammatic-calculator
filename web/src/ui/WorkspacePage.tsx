// A document's workspace: top bar, outline and cells, kept in step with the document and the
// evaluator.
import { createEffect, createResource, createSignal, For, on, onCleanup, onMount, Show, type Signal } from 'solid-js';
import { createCleanCopy, openDoc, markExported, type OpenDoc } from '../doc/store';
import { kindOf, type CellData, type Comparison, type Doc, type Inputs } from '../doc/model';
import { Evaluator, type CellState } from '../eval/evaluator';
import { cache, download, engine, engineId, isMac, mod, pickFile } from '../app';
import { exportDch, exportDcw, fileName } from '../doc/dcw';
import { Cell, TEMPLATES } from './Cell';
import { Outline } from './Outline';
import { ContextChip, ContextText } from './ContextChip';
import { Typeset } from './Typeset';
import { ResultView } from './Results';
import { Dialog } from './Dialog';
import { Palette, type Command } from './Palette';
import { importText } from './DocList';
import { Canvas, type Generator, type Selection } from './Canvas';
import { AIPanel } from './AIPanel';
import type { Proposal, SelectionRef, ToolHost } from '../ai/tools';
import { isSetUp, loadKey, loadSettings } from '../ai/providers';
import { account, api, user } from '../account/account';
import { DocSync } from '../account/docsync';
import { ShareDialog, SyncBadge } from './Account';

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

/** What the workspace view offers the AI and the page around it. */
export interface ViewApi {
  /** Cell states once every cell is computed (or after a time limit). */
  settled: () => Promise<Map<string, CellState>>;
  termContext: () => string;
}

/** The document, or one of its branches, shown in the workspace view, with the AI panel beside it. */
function Workspace(props: { o: OpenDoc }) {
  const main = props.o.doc;
  const linked = createSignal<FileSystemFileHandle | null>(null);
  // the AI panel shows by default once a key is entered
  const [aiOpen, setAiOpen] = createSignal(isSetUp(loadSettings(), loadKey(loadSettings().provider).key));
  const [selection, setSelection] = createSignal<Selection | null>(null);
  const [proposals, setProposals] = createSignal<Proposal[]>([]);
  let view: ViewApi | null = null;
  let focusAI: (() => void) | undefined;
  let aiBusy = false;
  const [current, setCurrent] = createSignal<{ id: string; name: string; doc: Doc } | null>(null);
  const [list, setList] = createSignal(main.branchList().map(({ id, name }) => ({ id, name })));
  const onBranches = () => setList(main.branchList().map(({ id, name }) => ({ id, name })));
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
    // a branch opened during an AI request takes part in its single undo step
    if (aiBusy) b?.doc.beginAI();
    else current()?.doc.endAI();
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
      show(id && d ? { id, name: main.branch(id)!.name, doc: d } : null);
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
  // signed in, the document is kept in step with the account (and someone who opened it by its
  // link without signing in gets others' changes)
  const [sync, setSync] = createSignal<DocSync | null>(null);
  createEffect(
    on(account, (a) => {
      const u = a.status === 'signed-in' ? a.user : null;
      const s = sync();
      if (s && s.user?.id !== u?.id) {
        void s.stop();
        setSync(null);
      }
      if (sync() || a.status === 'loading' || a.status === 'unavailable') return;
      if (!u && !props.o.info.cloud?.token) return;
      const next = new DocSync(props.o, u);
      next.start();
      setSync(next);
    }),
  );
  onCleanup(() => {
    flush();
    unwatch?.();
    main.branches.unobserve(onBranches);
    void sync()?.stop();
    props.o.close();
  });

  let nextProposal = 1;
  const applies = new Map<number, () => void>();
  // a document someone else owns: the AI's changes to it wait for the person (see ToolHost.foreign)
  const foreign = () => !!props.o.info.cloud && props.o.info.cloud.role !== 'owner';
  const host: ToolHost = {
    engine,
    main,
    doc: () => current()?.doc ?? main,
    branch: () => branching.current,
    createBranch: (name) => branching.create(name),
    openBranch: (id) => branching.open(id),
    compareBranch: () => branching.compare(),
    discardBranch: () => branching.discard(),
    settled: async () => (view ? view.settled() : new Map()),
    termContext: () => view?.termContext() ?? (current()?.doc ?? main).context,
    selection: (): SelectionRef | null => {
      const s = selection();
      const d = current()?.doc ?? main;
      return s && { version: d.versions().length, cell: s.cellId, summand: s.summand, word: s.word, text: s.text, layers: s.layers, rectangle: s.rectangle };
    },
    propose: (p, apply) => {
      const full: Proposal = { ...p, id: nextProposal++, state: 'open' };
      if (apply) applies.set(full.id, apply);
      setProposals([...proposals(), full]);
      return full;
    },
    foreign,
  };
  const decide = (p: Proposal, accept: boolean) => {
    setProposals(proposals().map((x) => (x.id === p.id ? { ...x, state: accept ? 'accepted' : 'rejected' } : x)));
    const apply = applies.get(p.id);
    applies.delete(p.id);
    if (!foreign()) main.log({ kind: 'proposal', author: 'you', text: `${accept ? 'accepted' : 'rejected'}: ${p.note}` });
    if (!accept) return;
    if (p.kind === 'write') apply?.();
    else if (p.kind === 'remove') host.doc().removeCell(p.target, `remove ${p.target} (proposed by the AI: ${p.note})`);
    else {
      if (branching.current?.id !== p.target) branching.open(p.target);
      branching.accept();
    }
  };
  const onBusy = (busy: boolean) => {
    aiBusy = busy;
    const d = current()?.doc;
    if (busy) {
      main.beginAI();
      d?.beginAI();
    } else {
      main.endAI();
      d?.endAI();
    }
  };
  const toggleAI = () => setAiOpen(!aiOpen());
  const ask = () => {
    setAiOpen(true);
    queueMicrotask(() => focusAI?.());
  };

  return (
    <div class="shell" classList={{ 'ai-open': aiOpen() }}>
      <div class="docpane">
        <Show when={current()?.doc ?? main} keyed>
          {(doc) => (
            <DocView o={props.o} doc={doc} branching={branching} linked={linked} aiOpen={aiOpen()} onToggleAI={toggleAI} onAsk={ask}
              onSelection={setSelection} api={(a) => (view = a)} sync={sync()} />
          )}
        </Show>
      </div>
      <Show when={aiOpen()}>
        <AIPanel host={host} main={main} proposals={proposals()} onDecide={decide} onBusy={onBusy} onClose={() => setAiOpen(false)}
          owner={props.o.info.cloud?.owner?.name || props.o.info.cloud?.owner?.email}
          focusRef={(f) => (focusAI = f)} />
      </Show>
    </div>
  );
}

const K = isMac ? '⌘' : 'Ctrl+';

function DocView(props: {
  o: OpenDoc;
  doc: Doc;
  branching: Branching;
  linked: Signal<FileSystemFileHandle | null>;
  aiOpen: boolean;
  onToggleAI: () => void;
  onAsk: () => void;
  onSelection: (s: Selection | null) => void;
  api: (a: ViewApi) => void;
  /** The document's sync with the account, when there is one. */
  sync: DocSync | null;
}) {
  const doc = props.doc;
  const main = props.o.doc;
  const onBranch = () => props.branching.current !== null;
  const [cells, setCells] = createSignal<CellData[]>(doc.list());
  const [context, setContext] = createSignal(doc.context);
  const [name, setName] = createSignal(doc.name);
  const [versions, setVersions] = createSignal(doc.versions());
  const [snapshots, setSnapshots] = createSignal(main.snapshotList());
  const [canUndo, setCanUndo] = createSignal(doc.undoManager.canUndo());
  const [canRedo, setCanRedo] = createSignal(doc.undoManager.canRedo());
  const [jobs, setJobs] = createSignal(0);
  const [expanded, setExpanded] = createSignal(new Set<string>());
  const [active, setActive] = createSignal<string | null>(null);
  const [adding, setAdding] = createSignal<number | null>(null);
  // on a narrow screen the outline is a drawer over the cells, closed at first
  const narrow = () => window.matchMedia?.('(max-width: 760px)').matches ?? false;
  const [outline, setOutline] = createSignal(!narrow());
  const [template, setTemplate] = createSignal<{ text: string; cursor?: number } | undefined>(undefined);
  const [renaming, setRenaming] = createSignal(false);
  const [branchName, setBranchName] = createSignal<string | null>(null);
  const [modal, setModal] = createSignal<null | 'palette' | 'keys' | 'branch' | 'share' | 'discard' | { version: number } | { snapshot: number } | { pin: string }>(null);
  const [linked, setLinked] = props.linked;
  const [message, setMessage] = createSignal('');
  const [canvas, setCanvas] = createSignal<{ id: string; word?: string } | null>(null);
  const [hintsVersion, setHintsVersion] = createSignal(0);

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
  props.api({
    termContext,
    settled: async () => {
      // give the evaluator a moment to see the latest step, then wait for every cell
      await new Promise((r) => setTimeout(r, 30));
      const deadline = Date.now() + 60_000;
      for (;;) {
        const list = doc.list();
        const done = list.every((c) => {
          const st = stateOf(c.id);
          return st && st.text === c.text && st.status !== 'running' && st.status !== 'stale';
        });
        if (done || Date.now() > deadline) return new Map(list.map((c) => [c.id, stateOf(c.id)!]));
        await new Promise((r) => setTimeout(r, 50));
      }
    },
  });
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
  // malformed shared content, skipped when reading (rechecked whenever the cells or history change)
  const problems = () => {
    cells();
    versions();
    return doc.problems();
  };
  const onSnapshots = () => setSnapshots(main.snapshotList());
  const onHints = () => setHintsVersion(hintsVersion() + 1);
  // pinned watches (kept in the document itself) and the term counts each has had this session
  const [pinnedIds, setPinnedIds] = createSignal(main.pinned);
  const onPinned = () => setPinnedIds(main.pinned);
  main.meta.observe(onPinned);
  onCleanup(() => main.meta.unobserve(onPinned));
  const trends = new Map<string, number[]>();
  const pinned = () =>
    pinnedIds().map((id) => {
      const v = stateOf(id)?.value as { status?: string; terms?: number } | undefined;
      const t = trends.get(id) ?? [];
      if (v?.status === 'ok' && typeof v.terms === 'number' && t.at(-1) !== v.terms) t.push(v.terms);
      trends.set(id, t);
      return { id, trend: [...t] };
    });
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
  /** Why a cell's text does not parse, as the engine reads it; null when it does. */
  const validate = async (text: string): Promise<string | null> => {
    const t = normalize(text);
    if (!t || t.startsWith('/') || !ev) return null;
    const p = await ev.parse(t);
    return 'error' in p ? p.error : null;
  };
  const commitNew = (index: number, text: string) => {
    const t = normalize(text);
    setAdding(null);
    setTemplate(undefined);
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
    if (s?.value && s.valueKey) setModal({ pin: id });
  };
  const pinAs = (id: string, label: string) => {
    setModal(null);
    const s = stateOf(id);
    if (!s?.value || !s.valueKey || !label.trim()) return;
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

  /** A new cell at `index`, starting from the template of a cell kind when one is given. */
  const addAt = (index: number, kind?: string) => {
    const t = TEMPLATES.find(([k]) => k === kind);
    setTemplate(t ? { text: t[1], cursor: t[2] } : undefined);
    setAdding(index);
  };
  const insertBelow = (id: string | null) => {
    const i = id ? cells().findIndex((c) => c.id === id) : -1;
    addAt(i < 0 ? cells().length : i + 1);
  };
  const stop = (id: string) => ev?.cancel(id);
  const runLonger = (id: string) => ev?.rerun(id, 600_000);
  const commandLine = (id: string) => {
    const ws = ev?.workspaceOf(id);
    return ws && `cat > cell.dcw <<'EOF'\n${ws}EOF\ndiagcalc run cell.dcw\n`;
  };
  const rename = (n: string) => {
    setRenaming(false);
    const t = n.trim();
    if (t && t !== name()) doc.rename(t);
  };

  const newBranch = (name?: string) => {
    setBranchName(null);
    props.branching.create(name?.trim() || undefined);
  };
  const discardBranch = () => {
    if (props.branching.current) setModal('discard');
  };

  const exportFile = async () => {
    const { text, canonical } = await exportDcw(doc);
    download(fileName(name(), 'dcw'), text);
    void markExported(props.o.info.id);
    setMessage(canonical ? 'Exported.' : 'Exported as entered: some cells do not parse.');
    // a copy of the file is kept with the document in the account
    const s = props.sync;
    if (s?.canEdit && s.cloudId && !onBranch()) {
      try {
        await api.putFile(s.cloudId, fileName(name(), 'dcw'), text);
        setMessage(`${canonical ? 'Exported' : 'Exported as entered'}; a copy is kept with the document in your account.`);
      } catch {
        // the download itself succeeded
      }
    }
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
    ...['def', 'watch', 'solve', 'check', 'note'].map((k) => ({ name: `New ${k} cell`, run: () => addAt(cells().length, k) })),
    { name: 'Insert a cell below the active cell', keys: isMac ? '⌥Enter' : 'Alt+Enter', run: () => insertBelow(active()) },
    { name: 'Rename document', run: () => setRenaming(true) },
    { name: 'Undo', keys: `${K}Z`, run: () => doc.undo() },
    { name: 'Redo', keys: isMac ? '⇧⌘Z' : 'Ctrl+Y', run: () => doc.redo() },
    { name: 'Export workspace (.dcw)', keys: isMac ? '⇧⌘S' : 'Ctrl+Shift+S', run: exportFile },
    { name: 'Export history (.dch)', run: () => download(fileName(name(), 'dch'), exportDch(doc), 'application/json') },
    { name: 'Import a workspace as a new document', keys: `${K}O`, run: importFile },
    { name: 'Save to a file on disk after every step', run: linkFile },
    { name: 'Start a scratch branch', keys: `${K}B`, run: () => setBranchName(defaultBranchName()) },
    ...(props.sync?.cloudId && user() ? [{ name: 'Share this document', run: () => setModal('share') }] : []),
    ...(onBranch()
      ? [
          { name: 'Compare and accept this branch', run: () => setModal('branch') },
          { name: 'Discard this branch', run: discardBranch },
          { name: 'Back to the document', run: () => props.branching.open(null) },
        ]
      : props.branching.list.map((b) => ({ name: `Open branch “${b.name}”`, run: () => props.branching.open(b.id) }))),
    { name: 'Cancel running computations', run: () => ev?.cancelAll() },
    { name: 'Clear the result cache', run: async () => { await cache.clear(); setMessage('Result cache cleared; results are computed again as needed.'); } },
    { name: 'Show or hide the outline', run: () => setOutline(!outline()) },
    { name: 'Open the active cell in the canvas', keys: 'F', run: () => active() && openCanvas(active()!) },
    { name: 'Show or hide the AI panel', keys: `${K}J`, run: props.onToggleAI },
    { name: 'Ask the AI about the selection', keys: `${K}L`, run: props.onAsk },
    { name: 'Keyboard shortcuts', keys: '?', run: () => setModal('keys') },
    { name: 'All documents', run: () => (location.hash = '#/') },
  ];

  const onKey = (e: KeyboardEvent) => {
    const inEditor = (e.target as HTMLElement).closest?.('.cm-editor, input, textarea');
    if (mod(e) && e.key.toLowerCase() === 'k') { e.preventDefault(); setModal(modal() === 'palette' ? null : 'palette'); return; }
    if (mod(e) && e.shiftKey && e.key.toLowerCase() === 's') { e.preventDefault(); void exportFile(); return; }
    if (mod(e) && e.key.toLowerCase() === 'o') { e.preventDefault(); void importFile(); return; }
    if (mod(e) && e.key.toLowerCase() === 'b') { e.preventDefault(); setBranchName(defaultBranchName()); return; }
    if (mod(e) && e.key.toLowerCase() === 'j') { e.preventDefault(); props.onToggleAI(); return; }
    if (mod(e) && e.key.toLowerCase() === 'l') { e.preventDefault(); props.onAsk(); return; }
    if (inEditor) return;
    if (mod(e) && e.key.toLowerCase() === 'z') { e.preventDefault(); e.shiftKey ? doc.redo() : doc.undo(); return; }
    if (mod(e) && e.key.toLowerCase() === 'y') { e.preventDefault(); doc.redo(); return; }
    if (e.key === '?') setModal('keys');
    if ((e.key === 'f' || e.key === 'F') && !mod(e) && active() && !canvas()) { e.preventDefault(); openCanvas(active()!); return; }
    if (e.key === 'Enter' && e.altKey && !mod(e)) { e.preventDefault(); insertBelow(active()); return; }
    if (e.key === 'Escape') { setModal(null); setBranchName(null); if (!canvas()) setActive(null); }
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
    <Cell id="new" text="" expanded active isNew initial={template()} onToggle={() => setAdding(null)}
      onCommit={(t) => commitNew(p.index, t)} onCancel={() => setAdding(null)} onRemove={() => setAdding(null)}
      onMove={() => {}} onPin={() => {}} onRerun={() => {}} onCopyTikz={() => {}} validate={validate} />
  );
  const defaultBranchName = () => `what if ${main.branches.size + 1}`;

  return (
    <>
      <header class="topbar">
        {/* inside a canvas, back means the document; from the document, the list of documents */}
        <Show when={canvas()} fallback={<button class="ghost" title="All documents" aria-label="All documents" onClick={() => (location.hash = '#/')}>←</button>}>
          {(c) => <button class="ghost" title="Back to the document (Esc)" aria-label="Back to the document" data-testid="back-to-doc" onClick={() => { const id = c().id; setCanvas(null); focusCell(id); }}>←</button>}
        </Show>
        <button class="ghost" title="Show or hide the outline" aria-label={outline() ? 'Hide the outline' : 'Show the outline'} aria-expanded={outline()} onClick={() => setOutline(!outline())}>☰</button>
        <Show when={canvas()} fallback={
          <Show when={renaming()} fallback={
            <button class="ghost name" title="Rename" aria-label={`Rename the document ${name()}`} onClick={() => setRenaming(true)}>{name()}</button>
          }>
            <input class="name-input" value={name()} aria-label="Document name" ref={(el) => queueMicrotask(() => { el.focus(); el.select(); })}
              onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); rename(e.currentTarget.value); } if (e.key === 'Escape') { e.stopPropagation(); setRenaming(false); } }}
              onBlur={(e) => renaming() && rename(e.currentTarget.value)} />
          </Show>
        }>
          {(c) => (
            <span class="crumbs" data-testid="crumbs">
              <button class="ghost name" title="Back to the document (Esc)" onClick={() => { const id = c().id; setCanvas(null); focusCell(id); }}>{name()}</button>
              <span class="hint">›</span>
              <span class="crumb">{canvasTitle(c().id)}</span>
            </span>
          )}
        </Show>
        <ContextChip context={context()} scalars={scalars()} onSet={(c) => c !== context() && doc.setContext(c)} />
        <Show when={props.branching.current} fallback={<>
          <span class="branch-start" style={{ position: 'relative' }}>
            <button class="ghost" title={`Try something on a scratch branch, then accept or discard it (${K}B)`} aria-expanded={branchName() !== null}
              data-testid="branch-start" onClick={() => setBranchName(branchName() === null ? defaultBranchName() : null)}>⑂ Branch</button>
            <Show when={branchName() !== null}>
              <div class="pop branch-pop" role="dialog" aria-label="Start a scratch branch">
                <label class="hint" for="branch-name">Try something without changing the document; accept or discard it afterwards.</label>
                <div class="row" style={{ 'margin-top': '6px' }}>
                  <input id="branch-name" class="grow" value={branchName() ?? ''} aria-label="Branch name" ref={(el) => queueMicrotask(() => { el.focus(); el.select(); })}
                    onInput={(e) => setBranchName(e.currentTarget.value)}
                    onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); newBranch(e.currentTarget.value); } if (e.key === 'Escape') { e.stopPropagation(); setBranchName(null); } }} />
                  <button class="primary" onClick={() => newBranch(branchName() ?? '')}>Start</button>
                </div>
              </div>
            </Show>
          </span>
          <Show when={props.branching.list.length}>
            <select class="branches" aria-label="Branches" onChange={(e) => { if (e.currentTarget.value) props.branching.open(e.currentTarget.value); }}>
              <option value="">{props.branching.list.length} {props.branching.list.length === 1 ? 'branch' : 'branches'}</option>
              <For each={props.branching.list}>{(b) => <option value={b.id}>{b.name}</option>}</For>
            </select>
          </Show>
        </>}>
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
        <span class="hint topbar-msg" role="status" onClick={() => setMessage('')}>{message()}</span>
        <Show when={linked() && !onBranch()}><span class="hint" title="Saved after every step">↔ {linked()!.name}</span></Show>
        <Show when={props.sync}>
          {(s) => (
            <>
              <SyncBadge sync={s()} />
              <Show when={s().cloudId && user()}>
                <button class="ghost" onClick={() => setModal('share')} data-testid="share">Share</button>
              </Show>
            </>
          )}
        </Show>
        <button class="ghost" title={`Undo (${K}Z)`} disabled={!canUndo()} onClick={() => doc.undo()} aria-label="Undo"><TurnArrow /></button>
        <button class="ghost" title="Redo" disabled={!canRedo()} onClick={() => doc.redo()} aria-label="Redo"><TurnArrow redo /></button>
        <button class={`light ${jobs() ? 'busy' : ''}`} disabled={!jobs()} data-testid="light"
          title={jobs() ? `${jobs()} computing; click to cancel` : 'Idle'}
          aria-label={jobs() ? `Cancel ${jobs()} ${jobs() === 1 ? 'computation' : 'computations'}` : 'No computations running'}
          onClick={() => jobs() && ev?.cancelAll()} />
        <button class="ghost" classList={{ on: props.aiOpen }} onClick={props.onToggleAI} title={`AI panel (${K}J)`} data-testid="ai-toggle">AI</button>
        <button class="ghost" onClick={() => setModal('palette')} title={`Commands (${K}K)`} aria-label="Commands">⋯</button>
      </header>
      <div class="layout" classList={{ 'no-outline': !outline() }}>
        <Show when={outline()}>
          <Outline cells={cells()} state={stateOf} active={active()} versions={versions()} snapshots={snapshots()} pinned={pinned()}
            onSelect={(id) => { if (narrow()) setOutline(false); focusCell(id); }}
            onVersion={(n) => setModal({ version: n })} onSnapshot={(i) => setModal({ snapshot: i })} />
        </Show>
        <div class="stage">
        <main class="main">
          <div class="cells">
            <Show when={props.branching.current}>
              {(b) => <div class="banner branch-banner">You are on the scratch branch “{b().name}”. Changes here leave the document alone until you accept them.</div>}
            </Show>
            <Show when={problems().length}>
              <div class="banner" data-testid="doc-problems">
                Parts of this document could not be read and are left out ({problems().join(', ')}): someone with edit access wrote content in a form this app does not write.{' '}
                <a href="#" onClick={async (e) => { e.preventDefault(); location.hash = `#/doc/${await createCleanCopy(doc, `${name()} (repaired)`)}`; }}>Make a repaired copy</a>
              </div>
            </Show>
            <Show when={reminder()}>
              <div class="banner">This document changed and has not been exported for a week. <a href="#" onClick={(e) => { e.preventDefault(); void exportFile(); }}>Export it</a> to keep a copy.</div>
            </Show>
            <For each={cells()}>
              {(c, i) => (
                <>
                  <Show when={adding() === i()}><NewCell index={i()} /></Show>
                  {/* a pointer shortcut; the keyboard inserts with Alt+Enter or the palette */}
                  <div class="insert"><button tabindex={-1} aria-hidden="true" onClick={() => addAt(i())}>+ cell</button></div>
                  <Show when={c.text} keyed>
                    {(text) => (
                      <Cell id={c.id} text={text} state={stateOf(c.id)} expanded={expanded().has(c.id)} active={active() === c.id}
                        onToggle={() => toggle(c.id)} onCommit={(t) => commit(c.id, t)} onCancel={() => setActive(null)}
                        onRemove={() => doc.removeCell(c.id)} onMove={(d) => doc.moveCell(c.id, i() + d)}
                        onPin={() => pin(c.id)} onRerun={() => ev?.rerun(c.id)} onCopyTikz={() => copyTikz(c.id)}
                        onCanvas={(w) => openCanvas(c.id, w)} validate={validate} onInsertBelow={() => insertBelow(c.id)}
                        onStop={() => stop(c.id)} onRunLonger={() => runLonger(c.id)} commandLine={() => commandLine(c.id)} />
                    )}
                  </Show>
                </>
              )}
            </For>
            <Show when={adding() !== null && adding()! >= cells().length}><NewCell index={cells().length} /></Show>
            <div style={{ 'margin-top': '10px' }}>
              <button onClick={() => addAt(cells().length)} data-testid="add-cell">+ New cell</button>
              <span class="hint" style={{ 'margin-left': '10px' }}>Type a term to define it, or <kbd>/</kbd> for a cell kind; <kbd>Enter</kbd> commits.</span>
            </div>
          </div>
        </main>
        <Show when={canvas()?.id} keyed>
          {(id) => (
            <Canvas doc={doc} engine={engine} cellId={id} title={canvasTitle(id)} context={termContext()}
              term={canvasTerm(id)} expanded={usesDefinitions(id)} generators={described()?.generators ?? []}
              colours={described()?.colours ?? []} hintsVersion={hintsVersion()} initialWord={canvas()?.word} value={stateOf(id)?.value}
              onWrite={(t, note) => writeCanvas(id, t, note)} onSelection={props.onSelection} onAsk={props.onAsk}
              onClose={() => { setCanvas(null); focusCell(id); }} />
          )}
        </Show>
        </div>
      </div>
      <Show when={modal() === 'palette'}><Palette commands={commands()} onClose={() => setModal(null)} /></Show>
      <Show when={modal() === 'keys'}><Keys onClose={() => setModal(null)} /></Show>
      <Show when={modal() === 'share' && props.sync?.cloudId}>
        {(id) => <ShareDialog cloudId={id()} name={name()} role={props.sync!.role ?? 'viewer'} onClose={() => setModal(null)}
          onCleanCopy={async () => { setModal(null); location.hash = `#/doc/${await createCleanCopy(main, `${name()} (clean copy)`)}`; }} />}
      </Show>
      <Show when={modal() === 'branch' && props.branching.compare()} keyed>
        {(cmp) => (
          <BranchView name={props.branching.current?.name ?? ''} cmp={cmp} onClose={() => setModal(null)}
            onAccept={() => { setModal(null); const r = props.branching.accept(); if (r) setMessage(`Accepted: ${describe(r)}.`); }}
            onDiscard={() => { setModal(null); discardBranch(); }} />
        )}
      </Show>
      <Show when={(() => { const m = modal(); return m && typeof m === 'object' && 'pin' in m ? m.pin : null; })()} keyed>
        {(id) => {
          const [label, setLabel] = createSignal(stateOf(id)?.parsed?.defines ?? id);
          return (
            <Dialog label="Pin the result" class="small" onClose={() => setModal(null)}>
              <h3 style={{ margin: '0 0 8px' }}>Pin the result</h3>
              <p class="hint" style={{ margin: '0 0 8px' }}>A snapshot keeps this value as it is now, in the outline under Snapshots.</p>
              <div class="row">
                <input class="grow" aria-label="Snapshot name" value={label()} onInput={(e) => setLabel(e.currentTarget.value)}
                  onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); pinAs(id, label()); } }} />
                <button class="primary" disabled={!label().trim()} onClick={() => pinAs(id, label())}>Pin</button>
              </div>
            </Dialog>
          );
        }}
      </Show>
      <Show when={modal() === 'discard' && props.branching.current}>
        {(b) => (
          <Dialog label="Discard the branch" class="small" onClose={() => setModal(null)}>
            <h3 style={{ margin: '0 0 8px' }}>Discard the branch “{b().name}”?</h3>
            <p class="hint" style={{ margin: '0 0 10px' }}>Its changes are lost; the document stays as it is.</p>
            <div class="row">
              <button class="danger" onClick={() => { setModal(null); props.branching.discard(); }}>Discard</button>
              <button onClick={() => setModal(null)}>Keep the branch</button>
            </div>
          </Dialog>
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
          <Dialog label={`Snapshot ${s.name}`} onClose={() => setModal(null)}>
              <h3 style={{ margin: '0 0 8px' }}>{s.name}</h3>
              <table class="kv"><tbody>
                <tr><td>cell</td><td><span class="kind">{kindOf(s.cellText) ?? 'def'}</span> <Typeset text={s.cellText.replace(/^(def|watch|solve|check)\s+/, '')} /></td></tr>
                <tr><td>context</td><td><ContextText context={s.context} /></td></tr>
                <tr><td>version</td><td>{s.version}, {new Date(s.time).toLocaleString()}</td></tr>
                <tr><td>inputs</td><td class="hint">{s.key}</td></tr>
              </tbody></table>
              <div style={{ 'margin-top': '10px' }}>
                <ResultView s={{ id: 'snapshot', text: s.cellText, status: 'fresh', value: s.value as CellState['value'] }} />
              </div>
              <details class="raw" style={{ 'margin-top': '8px' }}>
                <summary class="hint">Raw value</summary>
                <pre class="textform">{JSON.stringify(stripSvg(s.value), null, 2)}</pre>
              </details>
              <button style={{ 'margin-top': '8px' }} onClick={() => setModal(null)}>Close</button>
          </Dialog>
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
  const cells = (n: number) => `${n} ${n === 1 ? 'cell' : 'cells'}`;
  const counts: [number, string][] = [
    [c.changed.length, 'changed'],
    [c.added.length, 'added'],
    [c.removed.length, 'removed'],
  ];
  // "1 cell changed, 2 added": the noun once, on the first count
  const parts: string[] = counts.filter(([n]) => n).map(([n, what], i) => (i === 0 ? `${cells(n)} ${what}` : `${n} ${what}`));
  if (c.context) parts.unshift('the context changed');
  if (c.hints) parts.push(`${c.hints} ${c.hints === 1 ? 'drawing' : 'drawings'} moved`);
  return parts.length ? parts.join(', ') : 'no changes';
}

function BranchView(props: { name: string; cmp: Comparison; onClose: () => void; onAccept: () => void; onDiscard: () => void }) {
  const c = () => props.cmp;
  return (
    <Dialog label={`Branch ${props.name}`} onClose={props.onClose}>
        <h3 style={{ margin: '0 0 8px' }}>Accept “{props.name}” into the document?</h3>
        <p class="hint" style={{ margin: '0 0 8px' }}>Accepting is one step: undo takes it back. In this branch: {describe(c())}.</p>
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
    </Dialog>
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
    <Dialog label={`Version ${props.n}`} onClose={props.onClose}>
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
    </Dialog>
  );
}

function Keys(props: { onClose: () => void }) {
  // what each action is, its keys, and its pointer gesture when there is one
  const rows: [string, string, string][] = [
    ['Move between cells', 'Tab, ↑ ↓ on a cell', ''],
    ['Open or close a cell', 'Enter or Space on its head', 'click its head'],
    ['Edit a cell: its typeset text opens as plain text in a fixed-width font', 'Enter on the text', 'click the text'],
    ['Commit the cell being edited (Enter adds a line in a note)', `Enter or ${K}Enter`, ''],
    ['Stop editing, close a dialog', 'Esc', ''],
    ['Insert a cell below', isMac ? '⌥Enter' : 'Alt+Enter', 'hover between cells, + cell'],
    ['At the start of a new cell: choose its kind', '/', ''],
    ['Show a result’s text, selected, to copy', '', 'click the result'],
    ['Undo, redo (outside a cell editor)', `${K}Z, ${isMac ? '⇧⌘Z' : 'Ctrl+Shift+Z or Ctrl+Y'}`, ''],
    ['Command palette', `${K}K`, '⋯'],
    ['Import a workspace, export this one', `${K}O, ${isMac ? '⇧⌘S' : 'Ctrl+Shift+S'}`, ''],
    ['Start a scratch branch: try something, then accept or discard it', `${K}B`, '⑂ Branch'],
    ['Show or hide the AI panel; ask the AI about the selection', `${K}J, ${K}L`, 'AI'],
    ['Open the active definition or watch in the focus canvas (? there lists its gestures)', 'F', 'Canvas'],
    ['This sheet', '?', ''],
  ];
  return (
    <Dialog label="Keyboard shortcuts" onClose={props.onClose}>
        <h3 style={{ margin: '0 0 8px' }}>Keyboard shortcuts</h3>
        <table class="kv keys">
          <thead><tr><th>Action</th><th>Keys</th><th>Pointer</th></tr></thead>
          <tbody>
            <For each={rows}>{([action, keys, pointer]) => (
              <tr><td>{action}</td><td><Show when={keys}><kbd>{keys}</kbd></Show></td><td class="hint">{pointer}</td></tr>
            )}</For>
          </tbody>
        </table>
    </Dialog>
  );
}

/** The undo arrow (redo mirrored): an even arch whose two ends sit at the same height. */
function TurnArrow(props: { redo?: boolean }) {
  return (
    <svg class="turn" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" style={props.redo ? { transform: 'scaleX(-1)' } : undefined}>
      <path d="M5 16 C5 10.5 8.2 7.5 12 7.5 C15.8 7.5 19 10.5 19 16" />
      <path d="M2.2 13.2 L5 16.2 L7.8 13.2" />
    </svg>
  );
}
