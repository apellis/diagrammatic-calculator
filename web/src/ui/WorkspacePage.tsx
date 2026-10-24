// A document's workspace: top bar, outline, and cells, kept in step with the document and the
// evaluator.
import { createEffect, createResource, createSignal, For, on, onCleanup, onMount, Show, type Signal } from 'solid-js';
import { createCleanCopy, loadLink, openDoc, markExported, saveLink, type OpenDoc } from '../doc/store';
import { kindOf, type CellData, type Comparison, type Doc, type Inputs, type Snapshot } from '../doc/model';
import { Evaluator, type CellState } from '../eval/evaluator';
import { cache, download, engine, engineId, isMac, mod, pickFile } from '../app';
import { exportDch, exportDcw, parseDcw, fileName } from '../doc/dcw';
import { Cell, TEMPLATES } from './Cell';
import { NewCell as CellComposer } from './NewCell';
import { DiagramStart } from './DiagramStart';
import { Outline } from './Outline';
import { ContextChip, ContextText } from './ContextChip';
import { Typeset } from './Typeset';
import { ResultView, Brief } from './Results';
import { Dialog } from './Dialog';
import { Palette, type Command } from './Palette';
import { IMPORTABLE, importNotes, importText } from './DocList';
import { Canvas, type Generator, type Selection } from './Canvas';
import { AIPanel, type CellComment } from './AIPanel';
import { isAction, keysOf, type ActionId } from './shortcuts';
import { applyProposedWrite, type Proposal, type SelectionRef, type ToolHost } from '../ai/tools';
import * as Y from 'yjs';
import { prepareDecision, sharedProposals, stateHash } from '../ai/proposals';
import { isSetUp, loadKey, loadSettings } from '../ai/providers';
import { account, api, user } from '../account/account';
import { DocSync } from '../account/docsync';
import { FilesDialog, ShareDialog, SyncBadge } from './Account';
import { CloudRecovery } from './CloudRecovery';
import { PersonalBranches } from '../account/recovery';
import { PinnedRecompute } from './PinnedRecompute';
import { useAuthors } from './authors';
import { loadViewMode, saveViewMode } from './viewmode';

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
  /** The document now and as accepting the branch would make it, with the comparison. */
  preview: () => { before: Inputs; after: Inputs; comparison: Comparison } | null;
  accept: () => Comparison | null;
  discard: () => void;
}

/**
 * Who may change what is shown. One notion for every control, key, and AI tool: `readOnly` is
 * what they all check.
 */
export interface Access {
  /** The person cannot change the shared document (a view-only recipient, or not known yet). */
  viewOnly: () => boolean;
  /** View mode, switched on by the person viewing (kept in this browser, never shared). */
  viewing: () => boolean;
  setViewing: (on: boolean) => void;
  /** What is shown cannot be changed: view mode, or a view-only document (not a personal branch of it). */
  readOnly: () => boolean;
}

/** What the workspace view offers the AI and the page around it. */
export interface ViewApi {
  /** Cell states once every cell is computed (or after a time limit). */
  settled: () => Promise<Map<string, CellState>>;
  termContext: () => string;
  snapshotRun: (id: string) => Snapshot['run'];
  /** Opens a snapshot (by its key) in the snapshot dialog. */
  showSnapshot: (key: string) => void;
}

/** The document, or one of its branches, shown in the workspace view, with the AI panel beside it. */
function Workspace(props: { o: OpenDoc }) {
  const main = props.o.doc;
  const linked = createSignal<FileSystemFileHandle | null>(null);
  // the AI panel shows by default once a key is entered
  const [aiOpen, setAiOpen] = createSignal(isSetUp(loadSettings(), loadKey(loadSettings().provider).key));
  const [selection, setSelection] = createSignal<Selection | null>(null);
  let privateProposals: Proposal[] = [];
  const [proposals, setProposals] = createSignal<Proposal[]>(sharedProposals(main));
  const [proposalError, setProposalError] = createSignal('');
  const refreshProposals = () => setProposals([...sharedProposals(main), ...privateProposals]);
  main.proposals.observe(refreshProposals);
  onCleanup(() => main.proposals.unobserve(refreshProposals));
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
      const b = main.createBranch(name ?? `what if ${main.branches.size + 1}`, base !== main ? current()?.id : undefined);
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
    preview: () => {
      const c = current();
      return c ? main.previewBranch(c.id, c.doc) : null;
    },
    accept: () => {
      if (access.viewOnly() || access.viewing()) return null;
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
  const authorLabel = useAuthors(sync);
  const [viewing, setViewing] = createSignal(loadViewMode(props.o.info.id));
  const access: Access = {
    viewOnly: () => !!(sync()?.link() ?? props.o.info.cloud) &&
      (sync() ? !sync()!.canEdit : props.o.info.cloud?.role === 'viewer' || !user()),
    viewing,
    setViewing: (on) => {
      setViewing(on);
      saveViewMode(props.o.info.id, on);
    },
    readOnly: () => viewing() || (!current() && access.viewOnly()),
  };
  createEffect(
    on(account, (a) => {
      const u = a.status === 'signed-in' ? a.user : null;
      main.setIdentity(u?.id);
      current()?.doc.setIdentity(u?.id);
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

  createEffect(on(() => `${sync()?.cloudId ?? ''}:${sync()?.link()?.generation ?? 0}:${user()?.id ?? ''}`, () => {
    const link = sync()?.link(), u = user();
    if (!link || !u || link.account !== u.id || (link.generation ?? 0) === 0) return;
    const branches = new PersonalBranches(main, api, link.id, u.id, (error) => {
      if (!(error instanceof Error && /404|503|no such document|unavailable/.test(error.message)))
        setProposalError(`Personal branches could not sync: ${error instanceof Error ? error.message : 'network error'}. They remain in this browser.`);
    });
    onCleanup(branches.start());
  }));

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
    snapshotRun: (id) => view?.snapshotRun(id),
    termContext: () => view?.termContext() ?? (current()?.doc ?? main).context,
    selection: (): SelectionRef | null => {
      const s = selection();
      const d = current()?.doc ?? main;
      return s && { version: d.versions().length, cell: s.cellId, summand: s.summand, word: s.word, text: s.text, layers: s.layers, rectangle: s.rectangle };
    },
    propose: (p, apply) => {
      const full: Proposal = { ...p, id: nextProposal++, state: 'open' };
      const personal = p.branch || p.operation?.name.startsWith('branch_') || !sync()?.cloudId;
      if (personal) {
        if (apply) applies.set(full.id, apply);
        privateProposals.push(full);
        refreshProposals();
      } else {
        if (!sync()?.canEdit) throw new Error('This document is view only.');
        full.key = crypto.randomUUID();
        full.author = user()?.id ?? main.localOrigin.author;
        main.propose({ id: full.key, author: full.author, time: Date.now(), title: full.note,
          payload: JSON.parse(JSON.stringify(full)), status: 'pending' });
      }
      return full;
    },
    foreign,
    readOnly: access.readOnly,
    viewMode: viewing,
  };
  const deciding = new Set<string | number>();
  const decide = async (p: Proposal, accept: boolean) => {
    const key = p.key ?? p.id;
    if (deciding.has(key) || p.state !== 'open') return;
    if (access.readOnly()) return setProposalError(viewing() ? 'View mode is on: switch it off to decide proposals.' : 'This document is view only.');
    deciding.add(key); setProposalError('');
    let release: (() => void) | undefined;
    try {
      if (p.key) {
        const s = sync(); const link = s?.link();
        if (!s?.canEdit || !s.live || !link || !user()) throw new Error('Reconnect with editing access to decide this proposal.');
        release = s.live.pauseIncoming();
        const prepared = await prepareDecision(main, p, host, accept, user()!.id);
        if (await stateHash(main.ydoc) !== prepared.base) throw new Error('The document changed; try the decision again.');
        const headers: Record<string, string> = { 'content-type': 'application/octet-stream', 'x-csrf-token': api.csrf,
          'x-proposal-state': accept ? 'accepted' : 'rejected', 'x-proposal-base': prepared.base, 'x-doc-generation': String(link.generation ?? 0) };
        if (link.token) headers['x-link-token'] = link.token;
        const response = await fetch(`/api/docs/${encodeURIComponent(link.id)}/proposals/${encodeURIComponent(p.key)}/decision`,
          { method: 'POST', credentials: 'same-origin', headers, body: prepared.update.slice(), signal: AbortSignal.timeout(15_000) });
        if (!response.ok) {
          const error = await response.json().catch(() => ({})) as { error?: string };
          throw new Error(error.error ?? `Proposal decision failed (${response.status}).`);
        }
        main.undoManager.stopCapturing();
        Y.applyUpdate(main.ydoc, prepared.update, main.localOrigin);
        main.undoManager.stopCapturing();
      } else {
        if (accept && host.readOnly?.()) throw new Error('This document is view only.');
        if (accept) {
          if (p.kind === 'write') { if (p.operation) await applyProposedWrite(host, p.operation); else applies.get(p.id)?.(); }
          else if (p.kind === 'remove') host.doc().removeCell(p.target, `remove ${p.target} (proposed by the AI: ${p.note})`);
          else { if (branching.current?.id !== p.target) branching.open(p.target); branching.accept(); }
        }
        applies.delete(p.id);
        privateProposals = privateProposals.map((x) => x.id === p.id ? { ...x, state: accept ? 'accepted' : 'rejected' } : x);
        refreshProposals();
        if (!foreign() && !host.readOnly?.()) host.doc().log({ kind: 'proposal', author: main.localOrigin.author, authorKind: 'person', text: `${accept ? 'accepted' : 'rejected'}: ${p.note}` });
      }
    } catch (error) { setProposalError(error instanceof Error ? error.message : 'The decision could not be saved.'); }
    finally { release?.(); deciding.delete(key); }
  };
  const onBusy = (busy: boolean) => {
    aiBusy = busy;
    sync()?.updatePresence({ ai: busy ? { active: true } : null });
    const d = current()?.doc;
    if (busy) {
      main.beginAI();
      d?.beginAI();
    } else {
      main.endAI();
      d?.endAI();
    }
  };
  // comments on cells for the AI, kept while the page is open, like the chat
  const [comments, setComments] = createSignal<CellComment[]>([]);
  let nextComment = 1;
  const comment = (cell: string, cellText: string, text: string) => {
    setComments([...comments(), { id: nextComment++, cell, cellText, text, state: 'waiting' }]);
    setAiOpen(true);
  };
  const commentsSent = (ids: number[]) => setComments(comments().map((c) => (ids.includes(c.id) ? { ...c, state: 'sent' } : c)));
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
            <DocView o={props.o} doc={doc} branching={branching} access={access} linked={linked} aiOpen={aiOpen()} onToggleAI={toggleAI} onAsk={ask}
              onSelection={setSelection} api={(a) => (view = a)} sync={sync()} authorLabel={authorLabel} comments={comments} onComment={comment} />
          )}
        </Show>
      </div>
      <Show when={proposalError()}><div class="banner error" role="alert">{proposalError()}</div></Show>
      <Show when={aiOpen()}>
        <AIPanel host={host} main={main} proposals={proposals()} authorLabel={authorLabel} onDecide={decide} onBusy={onBusy} onClose={() => setAiOpen(false)}
          owner={props.o.info.cloud?.owner?.name || props.o.info.cloud?.owner?.email}
          focusRef={(f) => (focusAI = f)} comments={comments} onCommentsSent={commentsSent} onSnapshot={(k) => view?.showSnapshot(k)} />
      </Show>
    </div>
  );
}

const K = isMac ? '⌘' : 'Ctrl+';

function DocView(props: {
  o: OpenDoc;
  doc: Doc;
  branching: Branching;
  access: Access;
  linked: Signal<FileSystemFileHandle | null>;
  aiOpen: boolean;
  onToggleAI: () => void;
  onAsk: () => void;
  onSelection: (s: Selection | null) => void;
  api: (a: ViewApi) => void;
  /** The document's sync with the account, when there is one. */
  sync: DocSync | null;
  authorLabel: (id?: string, kind?: 'person' | 'ai') => string;
  /** Comments on cells for the AI, and making one. */
  comments: () => CellComment[];
  onComment: (cell: string, cellText: string, text: string) => void;
}) {
  const doc = props.doc;
  const main = props.o.doc;
  const onBranch = () => props.branching.current !== null;
  /** The person cannot change the shared document (pinning and accepting a branch write to it). */
  const mainReadOnly = props.access.viewOnly;
  /** View mode, switched on by the person (off again by them): not a view-only document's view. */
  const viewing = props.access.viewing;
  /** View mode that cannot be switched off here: a view-only document itself. */
  const locked = () => !onBranch() && mainReadOnly();
  const readOnly = props.access.readOnly;
  const authorLabel = props.authorLabel;
  const [drafts, setDrafts] = createSignal<Record<string, string>>({});
  const rememberDraft = (id: string, text: string | null) => {
    setDrafts((previous) => {
      if (text === null && !(id in previous) || previous[id] === text) return previous;
      const next = { ...previous };
      if (text === null) delete next[id]; else next[id] = text;
      return next;
    });
    refreshPreview();
  };
  const [cells, setCells] = createSignal(doc.keyedList());
  const previewing = () => cells().some(c => drafts()[c.key] !== undefined && drafts()[c.key] !== c.text);
  const keyOf = (id: string) => cells().find((c) => c.id === id)?.key;
  const idOf = (key: string | null) => cells().find((c) => c.key === key)?.id ?? null;
  const [context, setContext] = createSignal(doc.context);
  const [name, setName] = createSignal(doc.name);
  const [versions, setVersions] = createSignal(doc.versions());
  const [snapshots, setSnapshots] = createSignal(main.snapshotList());
  const [canUndo, setCanUndo] = createSignal(doc.undoManager.canUndo());
  const [canRedo, setCanRedo] = createSignal(doc.undoManager.canRedo());
  const [jobs, setJobs] = createSignal(0);
  const [expanded, setExpanded] = createSignal(new Set<string>());
  const [activeKey, setActiveKey] = createSignal<string | null>(null);
  const active = () => idOf(activeKey());
  const setActive = (id: string | null) => setActiveKey(id ? keyOf(id) ?? null : null);
  const [adding, setAdding] = createSignal<number | null>(null);
  // A stable keyed row keeps every private draft (including the drawing chooser) when moved.
  const composerKey = Symbol('new-cell');
  const insertionIndex = () => Math.min(adding() ?? cells().length, cells().length);
  const rowKeys = () => {
    const keys: (string | typeof composerKey)[] = cells().map((c) => c.key);
    if (adding() !== null) keys.splice(insertionIndex(), 0, composerKey);
    return keys;
  };
  // on a narrow screen the outline is a drawer over the cells, closed at first
  const narrow = () => window.matchMedia?.('(max-width: 760px)').matches ?? false;
  const [outline, setOutline] = createSignal(!narrow());
  const [template, setTemplate] = createSignal<{ text: string; cursor?: number } | undefined>(undefined);
  const [renaming, setRenaming] = createSignal(false);
  const [branchName, setBranchName] = createSignal<string | null>(null);
  const [modal, setModal] = createSignal<null | 'palette' | 'keys' | 'branch' | 'share' | 'files' | 'discard' | { version: number } | { snapshot: number } | { pin: string }>(null);
  // what is shared is what is shown: an edit still waiting for the pause before its save is saved now
  const share = () => {
    void props.sync?.saveNow();
    setModal('share');
  };
  const [linked, setLinked] = props.linked;
  const [message, setMessage] = createSignal('');
  /** Says why an edit (a key or a command) does nothing here. */
  const refuse = () => setMessage(viewing()
    ? `View mode is on: switch it off (${keysOf('view')}) to edit.`
    : 'This document is view only. Export it or make a personal branch to work on a copy.');
  // whether the new-cell composer holds text that closing it would lose
  let composerDirty = false;
  const toggleView = () => {
    if (locked()) return setMessage('This document is view only for you, so view mode stays on. Make a personal branch to work on a copy.');
    if (!viewing() && (previewing() || (adding() !== null && composerDirty)))
      return setMessage('Finish or discard the edit first (Enter or Esc), then switch view mode on.');
    props.access.setViewing(!viewing());
    setMessage(viewing() ? 'View mode: reading only.' : 'View mode off: editing again.');
  };
  const [canvasTarget, setCanvasTarget] = createSignal<{ key: string; word?: string } | null>(null);
  const canvas = () => {
    const target = canvasTarget(), id = target && idOf(target.key);
    return target && id ? { id, word: target.word } : null;
  };
  const setCanvas = (target: { id: string; word?: string } | null) => {
    const key = target && keyOf(target.id);
    setCanvasTarget(target && key ? { key, word: target.word } : null);
  };
  const [hintsVersion, setHintsVersion] = createSignal(0);

  const cursorsFor = (id: string) => {
    cells(); // Relative positions move when committed text changes, without a presence packet.
    const text = doc.cellText(id);
    return (props.sync?.presence() ?? []).flatMap((p) => {
      if (p.cell !== id || !text || !p.cursor || typeof p.cursor !== 'object') return [];
      try {
        const cursor = p.cursor as { anchor: object; head: object };
        const a = Y.createAbsolutePositionFromRelativePosition(Y.createRelativePositionFromJSON(cursor.anchor), doc.ydoc);
        const h = Y.createAbsolutePositionFromRelativePosition(Y.createRelativePositionFromJSON(cursor.head), doc.ydoc);
        return a?.type === text && h?.type === text ? [{ anchor: a.index, head: h.index, name: p.user?.name ?? 'Viewer', colour: p.user?.colour ?? '#537bc4' }] : [];
      } catch { return []; }
    });
  };
  const canvasPresence = () => (props.sync?.presence() ?? []).flatMap((p) => {
    const s = p.selection as Partial<Selection> | undefined;
    return s && typeof s.cellId === 'string' && typeof s.word === 'string' && Array.isArray(s.layers) && s.layers.length <= 512 && s.layers.every(Number.isSafeInteger)
      ? [{ name: p.user?.name ?? 'Viewer', colour: /^#[0-9a-f]{6}$/i.test(p.user?.colour ?? '') ? p.user!.colour : '#537bc4', selection: { cellId: s.cellId, word: s.word, layers: s.layers } }] : [];
  });
  const textSelection = (id: string, selection: { anchor: number; head: number } | null) => {
    if (onBranch()) return;
    const text = doc.cellText(id);
    props.sync?.updatePresence({ cell: id, cursor: text && selection ? {
      anchor: Y.relativePositionToJSON(Y.createRelativePositionFromTypeIndex(text, selection.anchor)),
      head: Y.relativePositionToJSON(Y.createRelativePositionFromTypeIndex(text, selection.head)),
    } : null });
  };

  // cell states, one signal per cell so only changed cells re-render
  const states = new Map<string, Signal<CellState | undefined>>();
  const previewStates = new Map<string, Signal<CellState | undefined>>();
  const slot = (id: string, slots = states) => {
    let s = slots.get(id);
    if (!s) slots.set(id, (s = createSignal<CellState | undefined>(undefined, { equals: false })));
    return s;
  };
  const stateOf = (id: string) => slot(id, previewing() ? previewStates : states)[0]();
  const scalars = () => [...new Set(cells().flatMap((c) => stateOf(c.id)?.parsed?.unknowns ?? []))].sort();
  /** The context with the workspace's scalars, for operations on a single term. */
  const termContext = (preview = true) => {
    const sc = preview ? scalars() : [...new Set(doc.list().flatMap(c => ev?.state(c.id)?.parsed?.unknowns ?? []))].sort();
    return sc.length && !/\bscalars=/.test(context()) ? `${context()} scalars=${sc.join(',')}` : context();
  };
  props.api({
    termContext: () => termContext(false),
    snapshotRun: (id) => ev?.snapshotRun(id),
    showSnapshot: (key) => {
      const i = snapshots().findIndex((s) => s.key === key);
      if (i >= 0) setModal({ snapshot: i });
    },
    settled: async () => {
      // give the evaluator a moment to see the latest step, then wait for every cell
      await new Promise((r) => setTimeout(r, 30));
      const deadline = Date.now() + 60_000;
      for (;;) {
        const list = doc.list();
        const done = list.every((c) => {
          const st = ev?.state(c.id);
          return st && st.text === c.text && st.status !== 'running' && st.status !== 'stale';
        });
        if (done || Date.now() > deadline) return new Map(list.map((c) => [c.id, ev?.state(c.id)!]));
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
  let previewEv: Evaluator | undefined;
  const visibleEvaluator = () => previewing() ? previewEv : ev;
  function refreshPreview() {
    if (!previewEv) return;
    // Local overlays must never change the document read by collaborators, AI, or exports.
    if (previewing()) void previewEv.update(doc.context, cells().map(c => ({ id: c.id, text: drafts()[c.key] ?? c.text })));
    else {
      void previewEv.update(doc.context, []);
      for (const s of previewStates.values()) s[1](undefined);
    }
  }
  const refresh = (_event?: unknown, transaction?: Y.Transaction) => {
    // Never reuse another UUID's row merely because its public ID and text match.
    const old = new Map(cells().map((c) => [c.key, c]));
    setCells(doc.keyedList().map((c) => {
      const previous = old.get(c.key);
      return previous?.id === c.id && previous.text === c.text ? previous : c;
    }));
    setContext(doc.context);
    setName(doc.name);
    setCanUndo(doc.undoManager.canUndo());
    setCanRedo(doc.undoManager.canRedo());
    const remote = transaction && !transaction.local && transaction.origin !== doc.localOrigin && transaction.origin !== doc.aiOrigin;
    void ev?.update(doc.context, doc.list(), remote ? 'remote' : 'local');
    refreshPreview();
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
  doc.cellOrder.observe(refresh);
  doc.meta.observe(refresh);
  doc.history.observe(onHistory);
  main.snapshots.observe(onSnapshots);
  doc.hints.observe(onHints);
  engine.onLoad = setJobs;
  engineId.then((id) => {
    ev = new Evaluator(engine, cache, { engineId: id }, (s) => slot(s.id)[1](s));
    previewEv = new Evaluator(engine, cache, { engineId: id }, (s) => slot(s.id, previewStates)[1](s));
    refresh();
  });
  if (!doc.cells.length && !readOnly()) setAdding(0);
  createEffect(() => { if (readOnly()) setAdding(null); });
  createEffect(() => { props.sync?.status(); props.sync?.updatePresence(onBranch()
    ? { cell: null, branch: 'personal branch', selection: null, cursor: null }
    : { cell: active(), branch: null }); });
  const note = importNotes.get(props.o.info.id);
  if (note) {
    importNotes.delete(props.o.info.id);
    setMessage(note);
  }

  // saving straight to a file on disk (Chrome, Edge)
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  function scheduleFileSave() {
    const h = linked();
    if (!h || onBranch()) return;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        const w = await h.createWritable();
        await w.write((await exportDcw(doc)).text);
        await w.close();
        void markExported(props.o.info.id);
      } catch (e) {
        setMessage(`Cannot save to ${h.name}: ${(e as Error).message}`);
      }
    }, 800);
  }
  // a document linked on an earlier visit: saving goes on at once if the browser still allows it,
  // otherwise after the person allows it again (browsers ask once per visit)
  const [relink, setRelink] = createSignal<FileSystemFileHandle | null>(null);
  if (!linked()) {
    void loadLink(props.o.info.id).then(async (h) => {
      if (!h || linked()) return;
      if ((await filePermission(h, false)) === 'granted') setLinked(h);
      else setRelink(h);
    });
  }
  const allowRelink = async () => {
    const h = relink();
    if (!h) return;
    if ((await filePermission(h, true)) !== 'granted') return setMessage(`Saving to ${h.name} stays paused: the browser was not allowed to write to it.`);
    setRelink(null);
    setLinked(h);
    scheduleFileSave();
    setMessage(`Saving to ${h.name} after every step.`);
  };
  const unlink = () => {
    setLinked(null);
    setRelink(null);
    void saveLink(props.o.info.id, null);
    setMessage('No longer saving to a file.');
  };

  const toggle = (id: string) => {
    const key = keyOf(id);
    if (!key) return;
    if (opened === key) opened = null;
    const s = new Set(expanded());
    if (s.has(key) && activeKey() === key) {
      s.delete(key);
      setActive(null);
    } else {
      s.add(key);
      setActive(id);
    }
    setExpanded(s);
  };
  // cells opened by moving to them close again when the next one opens; cells opened by a click stay open
  let opened: string | null = null;
  const focusCell = (id: string) => {
    const s = new Set(expanded());
    const key = keyOf(id);
    if (!key) return;
    if (opened && opened !== key) s.delete(opened);
    if (!s.has(key)) opened = key;
    s.add(key);
    setExpanded(s);
    setActive(id);
    requestAnimationFrame(() => document.getElementById(`cell-${id}`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }));
  };

  const freshName = () => {
    const used = new Set(cells().map((c) => /^\s*def\s+(\w+)\s*:=/.exec(c.text)?.[1]));
    let n = 1;
    while (used.has(`t${n}`)) n++;
    return `t${n}`;
  };
  /** A bare term becomes a definition with a fresh name. */
  const normalize = (text: string): string => {
    const t = text.trim();
    if (!t || kindOf(t) || t.startsWith('/')) return t;
    return `def ${freshName()} := ${t}`;
  };
  /** Why a cell's text does not parse, as the engine reads it; null when it does. */
  const validate = async (text: string): Promise<string | null> => {
    const t = normalize(text);
    if (!t || t.startsWith('/') || !ev) return null;
    const p = await ev.parse(t);
    return 'error' in p ? p.error : null;
  };
  const commitNew = (index: number, text: string, draw = false, restoreFocus = false) => {
    if (readOnly()) return refuse();
    const t = normalize(text);
    setAdding(null);
    setTemplate(undefined);
    if (!t || t.startsWith('/')) return;
    const id = doc.addCell(t, index);
    focusCell(id);
    if (draw) setCanvas({ id });
    else if (restoreFocus) queueMicrotask(() => {
      if (document.activeElement === document.body) document.getElementById(`cell-${id}`)?.querySelector<HTMLElement>('.head')?.focus();
    });
  };
  const commit = (id: string, text: string) => {
    if (readOnly()) return;
    const t = normalize(text);
    if (t.startsWith('/')) return;
    if (!t) doc.removeCell(id);
    else doc.updateCell(id, t);
  };

  const pin = (id: string) => {
    if (readOnly() || mainReadOnly() || previewing()) return;
    const s = stateOf(id);
    if (s?.value && s.valueKey) setModal({ pin: id });
  };
  const pinAs = (id: string, label: string) => {
    if (mainReadOnly()) return;
    if (readOnly() || previewing()) return;
    setModal(null);
    const s = stateOf(id);
    if (!s?.value || !s.valueKey || s.status !== 'fresh' || !label.trim()) return;
    const run = ev?.snapshotRun(id);
    main.pin({ name: label, cellId: id, cellText: s.text, context: context(), version: versions().length, key: s.valueKey, value: s.value, time: Date.now(), ...(run ? { run } : {}) });
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

  // The focus canvas edits definitions as written; watches are inspection-only.
  // A term using definitions is expanded, and an editable canvas can write that expansion.
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
    if (s?.status !== 'fresh' || !s.parsed?.term) return null;
    if (!usesDefinitions(id)) return s.parsed.term;
    const v = s.value as { status: string; term?: string } | undefined;
    return s.status === 'fresh' && v?.status === 'ok' && v.term ? v.term : null;
  };
  const canvasTitle = (id: string) => {
    const p = stateOf(id)?.parsed;
    return p?.kind === 'def' ? `def ${p.defines}` : `watch (${id})`;
  };
  const writeCanvas = (id: string, term: string, note: string, ids?: (fresh: (letter: string) => string) => Record<string, string[]>) => {
    const p = stateOf(id)?.parsed;
    if (readOnly() || previewing() || p?.kind !== 'def') return;
    const head = `def ${p.defines} := ${term}`;
    doc.updateCell(id, head + Object.entries(p.options).map(([k, v]) => `  ${k}=${v}`).join(''), `${note} in ${p.defines ?? id}`, ids);
  };

  /** A new cell at `index`, starting from the template of a cell kind when one is given. */
  const addAt = (index: number, kind?: string) => {
    if (readOnly()) return refuse();
    const t = TEMPLATES.find(([k]) => k === kind);
    setTemplate(t ? { text: t[1], cursor: t[2] } : undefined);
    setAdding(index);
    queueMicrotask(() => {
      const draft = document.querySelector<HTMLElement>('[data-cell="new"]');
      draft?.scrollIntoView({ block: 'nearest' });
      (draft?.querySelector<HTMLElement>('.cm-content') ?? draft?.querySelector<HTMLElement>('.entry-modes button[aria-pressed="true"]'))?.focus();
    });
  };
  const insertBelow = (id: string | null) => {
    const i = id ? cells().findIndex((c) => c.id === id) : -1;
    addAt(i < 0 ? cells().length : i + 1);
  };
  const stop = (id: string) => visibleEvaluator()?.cancel(id);
  const runLonger = (id: string) => visibleEvaluator()?.rerun(id, 600_000);
  const trace = (id: string) => (word: string, signal: AbortSignal) =>
    visibleEvaluator()?.trace(id, word, signal) ?? Promise.resolve({ status: 'cancelled' as const, ms: 0 });
  const commandLine = (id: string) => {
    const ws = visibleEvaluator()?.workspaceOf(id);
    return ws && `cat > cell.dcw <<'EOF'\n${ws}EOF\ndiagcalc run cell.dcw\n`;
  };
  const rename = (n: string) => {
    if (readOnly()) return refuse();
    setRenaming(false);
    const t = n.trim();
    if (t && t !== name()) doc.rename(t);
  };

  const newBranch = (name?: string) => {
    setBranchName(null);
    if (viewing()) return refuse();
    props.branching.create(name?.trim() || undefined);
  };
  const discardBranch = () => {
    if (viewing()) return refuse();
    if (props.branching.current) setModal('discard');
  };

  const exportFile = async () => {
    const { text, canonical } = await exportDcw(doc);
    download(fileName(name(), 'dcw'), text);
    void markExported(props.o.info.id);
    setMessage(canonical ? 'Exported.' : 'Exported as entered: some cells do not parse.');
    // a copy of the file is kept with the document in the account
    const s = props.sync;
    // in view mode nothing is written, not even a copy kept with the document
    if (s?.canEdit && s.cloudId && !onBranch() && !viewing()) {
      try {
        await api.putFile(s.cloudId, fileName(name(), 'dcw'), text);
        setMessage(`${canonical ? 'Exported' : 'Exported as entered'}; a copy is kept with the document in your account.`);
      } catch {
        // the download itself succeeded
      }
    }
  };
  const importFile = async () => {
    const f = await pickFile(IMPORTABLE);
    if (!f) return;
    try {
      const r = await importText(f.name, f.text);
      if (r.note) importNotes.set(r.id, r.note);
      location.hash = `#/doc/${r.id}`;
    } catch (e) {
      setMessage(`${f.name}: ${(e as Error).message}`);
    }
  };
  const linkFile = async () => {
    const pick = (window as unknown as { showSaveFilePicker?: (o: unknown) => Promise<FileSystemFileHandle> }).showSaveFilePicker;
    if (!pick) return setMessage('This browser cannot save straight to a file; use Export instead.');
    const h = await pick({ suggestedName: fileName(name(), 'dcw'), types: [{ description: 'Workspace', accept: { 'text/plain': ['.dcw'] } }] });
    setLinked(h);
    setRelink(null);
    void saveLink(props.o.info.id, h);
    scheduleFileSave();
    setMessage(`Saving to ${h.name} after every step; the link is kept for the next visit.`);
  };

  // the commands that edit are left out where nothing can be edited
  const commands = (): Command[] => [
    ...(locked() ? [] : [{ name: viewing() ? 'Switch view mode off (edit)' : 'Switch view mode on (read without editing)', keys: keysOf('view'), run: toggleView }]),
    ...(readOnly() ? [] : [
      ...['def', 'watch', 'solve', 'check', 'note'].map((k) => ({ name: `New ${k} cell`, run: () => addAt(cells().length, k) })),
      { name: 'Insert a cell below the active cell', keys: keysOf('insert'), run: () => insertBelow(active()) },
      { name: 'Rename document', run: () => setRenaming(true) },
      { name: 'Undo', keys: `${K}Z`, run: () => doc.undo() },
      { name: 'Redo', keys: isMac ? '⇧⌘Z' : 'Ctrl+Y', run: () => doc.redo() },
    ]),
    { name: 'Export workspace (.dcw)', keys: keysOf('export'), run: exportFile },
    { name: 'Export history (.dch)', run: () => download(fileName(name(), 'dch'), exportDch(doc), 'application/json') },
    { name: 'Import a workspace or history file as a new document', keys: keysOf('import'), run: importFile },
    { name: 'Save to a file on disk after every step', run: linkFile },
    ...(linked() || relink() ? [{ name: 'Stop saving to the file on disk', run: unlink }] : []),
    ...(viewing() ? [] : [{ name: 'Start a scratch branch', keys: keysOf('branch'), run: () => setBranchName(defaultBranchName()) }]),
    ...(props.sync?.cloudId && user() ? [{ name: 'Share this document', run: share }] : []),
    ...(props.sync?.cloudId ? [{ name: 'Files kept with this document', run: () => setModal('files') }] : []),
    ...(onBranch()
      ? [
          ...(viewing() ? [] : [
            { name: 'Compare and accept this branch', run: () => setModal('branch') },
            { name: 'Discard this branch', run: discardBranch },
          ]),
          { name: 'Back to the document', run: () => props.branching.open(null) },
        ]
      : props.branching.list.map((b) => ({ name: `Open branch “${b.name}”`, run: () => props.branching.open(b.id) }))),
    { name: 'Cancel running computations', run: () => { ev?.cancelAll(); previewEv?.cancelAll(); } },
    { name: 'Clear the result cache', run: async () => { await cache.clear(); setMessage('Result cache cleared; results are computed again as needed.'); } },
    { name: 'Show or hide the outline', run: () => setOutline(!outline()) },
    { name: 'Open the active cell in the canvas', keys: keysOf('canvas'), run: () => active() && openCanvas(active()!) },
    { name: 'Show or hide the AI panel', keys: keysOf('ai'), run: props.onToggleAI },
    { name: 'Ask the AI about the selection', keys: keysOf('ask'), run: props.onAsk },
    { name: 'Keyboard shortcuts', keys: keysOf('keys'), run: () => setModal('keys') },
    { name: 'Change the keyboard shortcuts (Settings)', run: () => (location.hash = '#/settings') },
    { name: 'All documents', run: () => (location.hash = '#/') },
  ];

  const onKey = (e: KeyboardEvent) => {
    const inEditor = !!(e.target as HTMLElement).closest?.('.cm-editor, input, textarea');
    const is = (id: ActionId) => isAction(e, id, inEditor);
    // the shortcuts of the settings page (Settings, Keyboard shortcuts)
    if (is('palette')) { e.preventDefault(); setModal(modal() === 'palette' ? null : 'palette'); return; }
    if (is('export')) { e.preventDefault(); void exportFile(); return; }
    if (is('import')) { e.preventDefault(); void importFile(); return; }
    if (is('branch')) { e.preventDefault(); if (viewing()) refuse(); else setBranchName(defaultBranchName()); return; }
    if (is('view')) { e.preventDefault(); toggleView(); return; }
    if (is('ai')) { e.preventDefault(); props.onToggleAI(); return; }
    if (is('ask')) { e.preventDefault(); props.onAsk(); return; }
    if (inEditor) return;
    if (mod(e) && e.key.toLowerCase() === 'z') { e.preventDefault(); if (readOnly()) refuse(); else if (e.shiftKey) doc.redo(); else doc.undo(); return; }
    if (mod(e) && e.key.toLowerCase() === 'y') { e.preventDefault(); if (readOnly()) refuse(); else doc.redo(); return; }
    if (is('keys')) { e.preventDefault(); setModal('keys'); return; }
    if (is('canvas') && active() && !canvas()) { e.preventDefault(); openCanvas(active()!); return; }
    if (is('insert')) { e.preventDefault(); insertBelow(active()); return; }
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
    doc.cellOrder.unobserve(refresh);
    doc.meta.unobserve(refresh);
    doc.history.unobserve(onHistory);
    main.snapshots.unobserve(onSnapshots);
    doc.hints.unobserve(onHints);
    um.off('stack-item-added', onStack);
    um.off('stack-item-popped', onStack);
    ev?.cancelAll();
    previewEv?.cancelAll();
  });

  const reminder = () => {
    const i = props.o.info;
    const since = i.exported ?? i.created;
    return i.edited > since && Date.now() - since > 7 * 86400_000;
  };

  const NewCell = (p: { index: number }) => (
    <CellComposer context={context()} position={p.index} name={freshName()} initial={template()} onDirty={(d) => (composerDirty = d)}
      drawing={(onStart, onChange) => <DiagramStart context={context()} onStart={onStart} onChange={onChange} />}
      onCommit={(t, draw, restoreFocus) => commitNew(p.index, t, draw, restoreFocus)} onCancel={() => {
        setAdding(null);
        queueMicrotask(() => {
          if (document.activeElement === document.body) document.querySelector<HTMLElement>('[data-testid="add-cell"]')?.focus();
        });
      }} validate={validate} />
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
          <Show when={renaming() && !readOnly()} fallback={
            <Show when={!readOnly()} fallback={<span class="name name-text" data-testid="doc-name">{name()}</span>}>
              <button class="ghost name" title="Rename" aria-label={`Rename the document ${name()}`} onClick={() => setRenaming(true)}>{name()}</button>
            </Show>
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
        <ContextChip context={context()} scalars={scalars()} readOnly={readOnly()} onSet={(c) => !readOnly() && doc.setContext(c)} />
        <Show when={props.branching.current} fallback={<>
          <Show when={!viewing()}><span class="branch-start" style={{ position: 'relative' }}>
            <button class="ghost" title={`Try something on a scratch branch, then accept or discard it (${keysOf('branch')})`} aria-expanded={branchName() !== null}
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
          </span></Show>
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
              <Show when={!viewing()}>
                <button class="primary" disabled={mainReadOnly()} onClick={() => setModal('branch')}
                  title={mainReadOnly() ? 'Only people who can edit the document can accept a branch into it' : undefined}>Accept…</button>
                <button onClick={discardBranch}>Discard</button>
              </Show>
              <button class="ghost" title="Back to the document; the branch is kept" onClick={() => props.branching.open(null)}>Leave</button>
            </span>
          )}
        </Show>
        <div class="grow" />
        <span class="hint topbar-msg" role="status" onClick={() => setMessage('')}>{message()}</span>
        <Show when={linked() && !onBranch()}><span class="hint" title="Saved to this file after every step" data-testid="linked">↔ {linked()!.name}</span></Show>
        <Show when={relink() && !onBranch()}>
          <button class="ghost" onClick={() => void allowRelink()} data-testid="relink"
            title="This document saves to a file on disk; the browser needs your permission again for this visit">↔ {relink()!.name}: resume saving</button>
        </Show>
        <details class="presence" data-testid="presence" hidden={!props.sync?.presence().length}>
          <summary aria-label={`People in this document (${props.sync?.presence().length ?? 0})`}>
            <For each={(props.sync?.presence() ?? []).slice(0, 3)}>{p => <span class="presence-avatar" aria-hidden="true"
              style={{ 'border-color': /^#[0-9a-f]{6}$/i.test(p.user?.colour ?? '') ? p.user!.colour : 'var(--border)' }}>{p.user?.name.slice(0, 1) ?? '?'}</span>}</For>
            <span>{props.sync?.presence().length ?? 0} here</span>
          </summary>
          <div class="presence-list" tabIndex={0}>
          <For each={props.sync?.presence() ?? []}>{(p) => <span class="collaborator" data-testid="collaborator"
            style={{ 'border-color': /^#[0-9a-f]{6}$/i.test(p.user?.colour ?? '') ? p.user!.colour : 'var(--border)' }}
            title={typeof p.cell === 'string' ? `At ${p.cell}` : 'Viewing this document'}>
            {p.user?.name ?? 'Viewer'}<Show when={p.ai && typeof p.ai === 'object' && (p.ai as { active?: boolean }).active}> · AI working</Show>
          </span>}</For>
          </div>
        </details>
        <Show when={props.sync}>
          {(s) => (
            <>
              <SyncBadge sync={s()} />
              <Show when={s().cloudId && user()}>
                <button class="ghost" onClick={share} data-testid="share">Share</button>
              </Show>
            </>
          )}
        </Show>
        <Show when={!locked()}>
          <button class="ghost view-toggle" classList={{ on: viewing() }} aria-pressed={viewing()} data-testid="view-mode" onClick={toggleView}
            title={`${viewing() ? 'View mode is on: click to edit again' : 'View mode: read without editing'} (${keysOf('view')})`}>View mode</button>
        </Show>
        <Show when={!readOnly()}>
          <button class="ghost" title={`Undo (${K}Z)`} disabled={!canUndo()} onClick={() => doc.undo()} aria-label="Undo"><TurnArrow /></button>
          <button class="ghost" title="Redo" disabled={!canRedo()} onClick={() => doc.redo()} aria-label="Redo"><TurnArrow redo /></button>
        </Show>
        <button class={`light ${jobs() ? 'busy' : ''}`} disabled={!jobs()} data-testid="light"
          title={jobs() ? `${jobs()} computing; click to cancel` : 'Idle'}
          aria-label={jobs() ? `Cancel ${jobs()} ${jobs() === 1 ? 'computation' : 'computations'}` : 'No computations running'}
          onClick={() => { if (jobs()) { ev?.cancelAll(); previewEv?.cancelAll(); } }} />
        <button class="ghost" classList={{ on: props.aiOpen }} onClick={props.onToggleAI} title={`AI panel (${keysOf('ai')})`} data-testid="ai-toggle">AI</button>
        <button class="ghost" onClick={() => setModal('palette')} title={`Commands (${keysOf('palette')})`} aria-label="Commands">⋯</button>
      </header>
      <div class="layout" classList={{ 'no-outline': !outline() }}>
        <Show when={outline()}>
          <Outline cells={cells()} state={stateOf} active={active()} versions={versions()} snapshots={snapshots()} pinned={pinned()} authorLabel={authorLabel}
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
            <CloudRecovery o={props.o} sync={props.sync} onBranch={(inputs, name) => props.branching.create(name, inputs)} />
            <Show when={readOnly()}>
              <Show when={viewing()} fallback={<div class="banner" data-testid="read-only">View only. Export or make a personal branch to work on a copy.</div>}>
                <div class="banner row" data-testid="view-mode-banner">
                  <span class="grow">View mode: reading only. Nothing here can be changed until you switch it off ({keysOf('view')}).</span>
                  <Show when={!locked()}><button onClick={toggleView}>Edit</button></Show>
                </div>
              </Show>
            </Show>
            <Show when={previewing()}><div class="banner" data-testid="draft-preview" role="status">Results preview your uncommitted edits. Enter or leaving the cell saves them; Esc discards them. Shared documents, exports, and AI reads still use the saved inputs.</div></Show>
            <For each={Object.entries(drafts()).filter(([key]) => !cells().some((c) => c.key === key))}>
              {([id, text]) => <div class="banner" data-testid="deleted-draft">
                Cell {id} was deleted elsewhere. Your uncommitted draft is still here.
                <pre>{text}</pre>
                <button disabled={readOnly()} onClick={() => { doc.addCell(text); rememberDraft(id, null); }}>Restore draft as new cell</button>
                <button onClick={() => rememberDraft(id, null)}>Discard draft</button>
              </div>}
            </For>
            <For each={rowKeys()}>
              {(key, rowIndex) => {
                if (key === composerKey) return <NewCell index={insertionIndex()} />;
                const i = () => rowIndex() - (adding() !== null && rowIndex() > insertionIndex() ? 1 : 0);
                // UUID owns the editor and draft; callbacks resolve the current public ID.
                const row = () => cells().find((c) => c.key === key);
                const id = () => row()?.id ?? '';
                const text = () => row()?.text ?? '';
                return <>

                  <Show when={!readOnly()}><div class="insert"><button tabindex={-1} aria-hidden="true" onClick={() => addAt(i())}>+ cell</button></div></Show>
                  <Show when={props.sync?.presence().some((p) => p.cell === id())}>
                    <div class="cell-presence hint" aria-live="polite">{props.sync?.presence().filter((p) => p.cell === id()).map((p) => p.user?.name ?? 'Viewer').join(', ')} here</div>
                  </Show>
                  <Cell id={id()} text={text()} state={stateOf(id())} expanded={expanded().has(key)} active={activeKey() === key}
                    readOnly={readOnly()} onDraft={(t) => { if (row()) rememberDraft(key, t); }} remoteCursors={cursorsFor(id())} onSelection={(s) => textSelection(id(), s)}
                    onToggle={() => toggle(id())} onCommit={(t) => commit(id(), t)} onCancel={() => setActive(null)}
                    onRemove={() => !readOnly() && doc.removeCell(id())} onMove={(d) => !readOnly() && doc.moveCell(id(), i() + d)}
                    onPin={() => pin(id())} pinDisabled={mainReadOnly() || previewing()} onRerun={() => visibleEvaluator()?.rerun(id())} onCopyTikz={() => copyTikz(id())}
                    onCanvas={(w) => openCanvas(id(), w)} validate={validate} onInsertBelow={() => insertBelow(id())}
                    onStop={() => stop(id())} onRunLonger={() => runLonger(id())} commandLine={() => commandLine(id())} trace={trace(id())}
                    comments={props.comments().filter((x) => x.cell === id())} onComment={readOnly() ? undefined : (t) => props.onComment(id(), text(), t)} />
                </>;
              }}
            </For>

            <Show when={adding() === null && !readOnly()}><div style={{ 'margin-top': '10px' }}>
              <button onClick={() => addAt(cells().length)} data-testid="add-cell">+ New cell</button>
            </div></Show>
          </div>
        </main>
        <Show when={canvas()?.id && `${canvas()!.id}:${previewing()}`} keyed>
          {(_identity) => {
            const id = canvas()!.id;
            // Remount across the preview boundary: a delayed private layout must never become
            // a shared selection merely because the draft was discarded meanwhile.
            const privateCanvas = previewing();
            const key = keyOf(id)!;
            return <Canvas doc={doc} readOnly={readOnly() || previewing() || stateOf(id)?.parsed?.kind !== 'def'} remoteSelections={canvasPresence()} engine={engine} cellId={id} title={canvasTitle(id)} context={termContext()}
              term={canvasTerm(id)} expanded={usesDefinitions(id)} generators={described()?.generators ?? []}
              colours={described()?.colours ?? []} hintsVersion={hintsVersion()} initialWord={canvas()?.word} value={stateOf(id)?.value}
              onWrite={(t, note, ids) => { const current = idOf(key); if (!readOnly() && current) writeCanvas(current, t, note, ids); }} onSelection={(s) => {
                const selection = privateCanvas || previewing() ? null : s;
                props.onSelection(selection);
                if (!onBranch()) props.sync?.updatePresence({ selection });
              }} onAsk={props.onAsk}
              onClose={() => { setCanvas(null); const current = idOf(key); if (current) focusCell(current); }} />;
          }}
        </Show>
        </div>
      </div>
      <Show when={modal() === 'palette'}><Palette commands={commands()} onClose={() => setModal(null)} /></Show>
      <Show when={modal() === 'keys'}><Keys onClose={() => setModal(null)} /></Show>
      <Show when={modal() === 'share' && props.sync?.cloudId}>
        {(id) => <ShareDialog cloudId={id()} name={name()} role={props.sync!.role ?? 'viewer'} onClose={() => setModal(null)}
          onCleanCopy={async () => { setModal(null); location.hash = `#/doc/${await createCleanCopy(main, `${name()} (clean copy)`)}`; }} />}
      </Show>
      <Show when={modal() === 'files' && props.sync?.cloudId}>
        {(id) => <FilesDialog cloudId={id()} name={name()} canEdit={!!props.sync?.canEdit && !viewing()} token={props.o.info.cloud?.token} onClose={() => setModal(null)}
          onOpen={async (file, text) => {
            const r = await importText(file, text);
            if (r.note) importNotes.set(r.id, r.note);
            setModal(null);
            location.hash = `#/doc/${r.id}`;
          }} />}
      </Show>
      <Show when={modal() === 'branch' && props.branching.preview()} keyed>
        {(pv) => (
          <BranchView name={props.branching.current?.name ?? ''} canAccept={!mainReadOnly()} cmp={pv.comparison} before={pv.before} after={pv.after} onClose={() => setModal(null)}
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
        {(n) => <VersionView n={n} canRestore={!readOnly()} canBranch={!viewing()} at={doc.at(n)} dcw={(s) => doc.toDcw(s)} current={doc.toDcw()} onClose={() => setModal(null)}
          onRestore={(s) => { if (readOnly()) return; doc.replace(s.context, s.cells, `restore version ${n}`); setModal(null); }}
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
                <tr><td>computed by</td><td>{s.author ? `${authorLabel(s.author)} (browser)` : 'Unknown (older snapshot)'}</td></tr>
                <tr><td>version</td><td>{s.version}, {new Date(s.time).toLocaleString()}</td></tr>
                <tr><td>inputs</td><td class="hint">{s.key}</td></tr>
              </tbody></table>
              <Show when={stateOf(s.cellId)?.valueKey !== s.key}><p class="hint" data-testid="snapshot-outdated">The current inputs or engine differ from this snapshot, or its cell was removed. The saved result is unchanged.</p></Show>
              <PinnedRecompute snapshot={s} />
              <Show when={!s.run}><p class="hint">This older snapshot does not retain its dependency inputs.</p></Show>
              <Show when={s.run && !viewing()}>
                <button onClick={() => void parseDcw(s.run!.workspace).then((inputs) => { setModal(null); props.branching.create(`Recompute ${s.name}`, { ...inputs, hints: {} }); }).catch((e) => setMessage(`Cannot reopen snapshot inputs: ${e.message}`))}>Open inputs in personal branch</button>
              </Show>
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

function BranchView(props: { name: string; canAccept: boolean; cmp: Comparison; before: Inputs; after: Inputs; onClose: () => void; onAccept: () => void; onDiscard: () => void }) {
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
        <ValueChanges before={props.before} after={props.after} />
        <div class="row" style={{ 'margin-top': '10px' }}>
          <button class="primary" disabled={!props.canAccept} onClick={props.onAccept}>Accept</button>
          <button onClick={props.onClose}>Keep working on the branch</button>
          <div class="grow" />
          <button onClick={props.onDiscard}>Discard the branch</button>
        </div>
    </Dialog>
  );
}

/** Cell states of some inputs, computed by an evaluator of their own (mostly from the result cache) while shown. */
function useStates(inputs: Inputs): () => Map<string, CellState> {
  const [states, setStates] = createSignal(new Map<string, CellState>(), { equals: false });
  let ev: Evaluator | undefined;
  let alive = true;
  void engineId.then((id) => {
    if (!alive) return;
    ev = new Evaluator(engine, cache, { engineId: id }, (s) => {
      if (!alive) return;
      const m = states();
      m.set(s.id, s);
      setStates(m);
    });
    void ev.update(inputs.context, inputs.cells);
  });
  onCleanup(() => {
    alive = false;
    ev?.cancelAll();
  });
  return states;
}

/** A value as text, to tell whether two values differ. */
function valueText(s?: CellState): string | null {
  if (!s || s.status === 'running' || s.status === 'stale') return null;
  if (s.parseError) return `parse error ${s.parseError}`;
  const v = s.value;
  if (!v) return '';
  if (v.status !== 'ok') return `${v.status} ${v.message}`;
  switch (v.kind) {
    case 'watch': return `watch ${v.normal_form}`;
    case 'def': return `def ${v.term}`;
    case 'check': return `check ${v.holds}`;
    default: return JSON.stringify({ ...v, provenance: undefined });
  }
}

/**
 * How accepting a branch changes the values of the document's cells: for each cell whose value
 * would change, the value now and the value after, typeset.
 */
function ValueChanges(props: { before: Inputs; after: Inputs }) {
  const before = useStates(props.before);
  const after = useStates(props.after);
  const computes = (s?: CellState) => !!s && s.status !== 'static' && !!s.parsed && !['note', 'comment', 'blank'].includes(s.parsed.kind);
  const rows = () => {
    const [b, a] = [before(), after()];
    const out: { id: string; kind: string; old?: CellState; now?: CellState; what: 'changed' | 'added' | 'removed' }[] = [];
    const ids = new Set(props.before.cells.map((c) => c.id));
    for (const c of props.after.cells) {
      const now = a.get(c.id);
      const old = ids.has(c.id) ? b.get(c.id) : undefined;
      if (!computes(now) && !computes(old)) continue;
      if (ids.has(c.id)) {
        if (old?.key && now?.key && old.key === now.key) continue;
        const [x, y] = [valueText(old), valueText(now)];
        if (x !== null && y !== null && x === y) continue;
      }
      out.push({ id: c.id, kind: (now ?? old)?.parsed?.kind ?? '', old, now, what: ids.has(c.id) ? 'changed' : 'added' });
    }
    const kept = new Set(props.after.cells.map((c) => c.id));
    for (const c of props.before.cells) {
      const old = b.get(c.id);
      if (!kept.has(c.id) && computes(old)) out.push({ id: c.id, kind: old!.parsed!.kind, old, what: 'removed' });
    }
    return out;
  };
  const pending = () => [...before().values(), ...after().values()].some((s) => s.status === 'running' || s.status === 'stale');
  return (
    <div class="value-changes" data-testid="value-changes">
      <h4>Values</h4>
      <Show when={rows().length} fallback={<p class="hint">{pending() ? 'Computing the values…' : 'No value of a cell changes.'}</p>}>
        <table class="kv">
          <tbody>
            <For each={rows()}>
              {(r) => (
                <tr data-cell={r.id}>
                  <td><span class="kind">{r.kind}</span> {r.id}</td>
                  <td>
                    <span class="row" style={{ 'flex-wrap': 'wrap', gap: '6px' }}>
                      <Show when={r.what !== 'added'} fallback={<span class="hint">new cell</span>}><Brief s={r.old} /></Show>
                      <span class="hint" aria-label="becomes">→</span>
                      <Show when={r.what !== 'removed'} fallback={<span class="hint">removed</span>}><Brief s={r.now} /></Show>
                    </span>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <Show when={pending()}><p class="hint">Computing the other values…</p></Show>
      </Show>
    </div>
  );
}

/**
 * A line diff from `a` to `b` (longest common subsequence): each line kept (' '), removed ('−'),
 * or added ('+').
 */
export function lineDiff(a: string[], b: string[]): [' ' | '−' | '+', string][] {
  const n = a.length;
  const m = b.length;
  const l: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) l[i][j] = a[i] === b[j] ? l[i + 1][j + 1] + 1 : Math.max(l[i + 1][j], l[i][j + 1]);
  const out: [' ' | '−' | '+', string][] = [];
  let [i, j] = [0, 0];
  while (i < n || j < m) {
    if (i < n && j < m && a[i] === b[j]) {
      out.push([' ', a[i++]]);
      j++;
    } else if (j < m && (i === n || l[i][j + 1] >= l[i + 1][j])) out.push(['+', b[j++]]);
    else out.push(['−', a[i++]]);
  }
  return out;
}

function VersionView(props: {
  canRestore: boolean;
  /** Whether it can be opened as a branch (not in view mode). */
  canBranch: boolean;
  n: number;
  at: { context: string; cells: CellData[] } | null;
  dcw: (s: { context: string; cells: CellData[] }) => string;
  /** The document now, as .dcw text, to compare the version with. */
  current: string;
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
              <Show when={props.dcw(s()) !== props.current} fallback={<p class="hint">The document is the same as this version now.</p>}>
                <p class="hint" style={{ margin: '0 0 6px' }}>What changed since this version: lines marked − are in the version only, lines marked + are in the document now.</p>
              </Show>
              <pre class="textform diff" data-testid="version-diff">
                <For each={lineDiff(props.dcw(s()).split('\n'), props.current.split('\n'))}>
                  {([k, line]) => <div class={k === '+' ? 'add' : k === '−' ? 'del' : undefined}>{k} {line}</div>}
                </For>
              </pre>
              <div class="row" style={{ 'margin-top': '10px' }}>
                <Show when={props.canRestore}><button class="primary" onClick={() => props.onRestore(s())}>Restore this version</button></Show>
                <Show when={props.canBranch}><button onClick={() => props.onBranch(s())}>Open as a branch</button></Show>
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
    ['Insert a cell below', keysOf('insert'), 'hover between cells, + cell'],
    ['At the start of a new cell: choose its kind', '/', ''],
    ['Show a result’s text, selected, to copy', '', 'click the result'],
    ['Undo, redo (outside a cell editor)', `${K}Z, ${isMac ? '⇧⌘Z' : 'Ctrl+Shift+Z or Ctrl+Y'}`, ''],
    ['Command palette', keysOf('palette'), '⋯'],
    ['Import a workspace, export this one', `${keysOf('import')}, ${keysOf('export')}`, ''],
    ['Start a scratch branch: try something, then accept or discard it', keysOf('branch'), '⑂ Branch'],
    ['Switch view mode on or off: read without editing (kept in this browser, not shared)', keysOf('view'), 'View mode'],
    ['Show or hide the AI panel; ask the AI about the selection', `${keysOf('ai')}, ${keysOf('ask')}`, 'AI'],
    ['Open the active definition or watch in the focus canvas (? there lists its gestures)', keysOf('canvas'), 'Canvas'],
    ['This sheet', keysOf('keys'), ''],
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
        <p class="hint" style={{ margin: '8px 0 0' }}>The main shortcuts can be changed in <a href="#/settings" onClick={props.onClose}>Settings</a>.</p>
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

/** Whether the page may write to a file (asking the person when `ask`, which needs a click). */
async function filePermission(h: FileSystemFileHandle, ask: boolean): Promise<PermissionState> {
  const f = h as FileSystemFileHandle & {
    queryPermission?: (d: { mode: 'readwrite' }) => Promise<PermissionState>;
    requestPermission?: (d: { mode: 'readwrite' }) => Promise<PermissionState>;
  };
  try {
    const p = (await f.queryPermission?.({ mode: 'readwrite' })) ?? 'granted';
    return p === 'granted' || !ask ? p : ((await f.requestPermission?.({ mode: 'readwrite' })) ?? 'denied');
  } catch {
    return 'denied';
  }
}
