// The AI panel: the chat (with each claim linked to the operation it comes from, and the
// person's comments on cells), proposals to accept or reject, and the activity timeline of AI
// steps with the key diagrams of its milestones.
import { createEffect, createSignal, For, Match, onCleanup, onMount, Show, Switch, untrack } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type Anthropic from '@anthropic-ai/sdk';
import type { Doc, Snapshot, TimelineEntry } from '../doc/model';
import type { CellState } from '../eval/evaluator';
import { commentMessage, run, userMessage, type AgentEvent } from '../ai/agent';
import { ANTHROPIC_MODELS, isSetUp, loadKey, loadSettings, provider } from '../ai/providers';
import type { Proposal, SelectionRef, ToolHost } from '../ai/tools';
import { markdown } from './markdown';
import { ResultView } from './Results';
import { keysOf } from './shortcuts';

type Part =
  | { type: 'text'; text: string }
  | { type: 'thinking'; text: string }
  | { type: 'tool'; id: string; ref: string; name: string; input: string; summary?: string; result?: string; isError?: boolean }
  | { type: 'stopped'; reason: string };
type Entry = { role: 'user'; text: string; selection?: string; cell?: string } | { role: 'assistant'; parts: Part[] };
type ToolPart = Extract<Part, { type: 'tool' }>;

/** A comment the person made on a cell for the AI; it waits until the AI can take it. */
export interface CellComment {
  id: number;
  cell: string;
  /** The cell's text when the comment was made. */
  cellText: string;
  text: string;
  state: 'waiting' | 'sent';
}

/** A tool's input or result as text to read: JSON indented, long text clipped. */
function readable(s: string, max = 6000): string {
  let t = s;
  try {
    t = JSON.stringify(JSON.parse(s), (k, v) => (k === 'svg' ? undefined : v), 2);
  } catch {
    // not JSON: as it is
  }
  return t.length > max ? `${t.slice(0, max)}… [${t.length - max} more characters]` : t;
}

const NOTICE = 'diagcalc.ai.noticeSeen';

export interface AIPanelProps {
  host: ToolHost;
  /** The document itself: its timeline. */
  main: Doc;
  proposals: Proposal[];
  onDecide: (p: Proposal, accept: boolean) => void;
  /** Steps during a request are the AI's and undo as one. */
  onBusy: (busy: boolean) => void;
  onClose: () => void;
  /** Set by the panel: focuses the message box (for Cmd-L). */
  focusRef?: (f: () => void) => void;
  /** Who owns the document, when it is someone else's. */
  owner?: string;
  /** The person's comments on cells; waiting ones go to the AI as soon as it can take them. */
  comments?: () => CellComment[];
  onCommentsSent?: (ids: number[]) => void;
  /** Opens a snapshot (a milestone's key diagram) by its key. */
  onSnapshot?: (key: string) => void;
}

export function AIPanel(props: AIPanelProps) {
  // the provider, model and key are set on the settings page; read afresh when used
  const settings = () => loadSettings();
  const key = () => loadKey(settings().provider).key;
  const ready = () => isSetUp(settings(), key());
  const modelName = () => ANTHROPIC_MODELS.find((m) => m.id === settings().model)?.name ?? settings().model;
  const [noticeSeen, setNoticeSeen] = createSignal(seen());
  const [entries, setEntries] = createStore<Entry[]>([]);
  const [draft, setDraft] = createSignal('');
  const [attach, setAttach] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [usage, setUsage] = createSignal<{ input: number; output: number; cacheRead: number } | null>(null);
  const [timeline, setTimeline] = createSignal<TimelineEntry[]>(props.main.timelineList());
  const [snapshots, setSnapshots] = createSignal<Snapshot[]>(props.main.snapshotList());
  const [tab, setTab] = createSignal<'chat' | 'activity'>('chat');
  const messages: Anthropic.MessageParam[] = [];
  let abort: AbortController | null = null;
  let box!: HTMLTextAreaElement;
  let log!: HTMLDivElement;

  const onTimeline = () => setTimeline(props.main.timelineList());
  const onSnapshots = () => setSnapshots(props.main.snapshotList());
  props.main.timeline.observe(onTimeline);
  props.main.snapshots.observe(onSnapshots);
  onCleanup(() => {
    props.main.timeline.unobserve(onTimeline);
    props.main.snapshots.unobserve(onSnapshots);
    abort?.abort();
  });
  onMount(() => props.focusRef?.(() => box?.focus()));

  const selection = (): SelectionRef | null => props.host.selection();
  const selectionLabel = (s: SelectionRef) =>
    `${s.cell}, summand ${s.summand}${s.layers.length ? `, ${s.layers.length} piece${s.layers.length > 1 ? 's' : ''}` : ''}`;

  const scroll = () => requestAnimationFrame(() => log && (log.scrollTop = log.scrollHeight));

  /** The assistant entry the running request writes into. */
  let at = -1;
  const waiting = () => (props.comments?.() ?? []).filter((c) => c.state === 'waiting');
  /** Takes the waiting comments: shown in the chat, logged, and returned as the person's messages. */
  const takeComments = (): string[] => {
    const cs = waiting();
    if (!cs.length) return [];
    props.onCommentsSent?.(cs.map((c) => c.id));
    for (const c of cs) {
      setEntries(entries.length, { role: 'user', text: c.text, cell: c.cell });
      // the comment's text stays in this chat, like a request's
      if (!props.host.foreign()) props.main.log({ kind: 'request', author: 'you', text: `Commented on ${c.cell} for the AI.` });
    }
    return cs.map((c) => commentMessage({ id: c.cell, text: c.cellText }, c.text));
  };

  /** Runs one request whose user message is already in `messages`. */
  const request = async () => {
    setEntries(entries.length, { role: 'assistant', parts: [] });
    at = entries.length - 1;
    setBusy(true);
    props.onBusy(true);
    abort = new AbortController();
    const parts = (f: (ps: Part[]) => void) => setEntries(at, produce((e) => { if (e.role === 'assistant') f(e.parts); }));
    const add = (part: Part) => parts((ps) => ps.push(part));
    const append = (type: 'text' | 'thinking', delta: string) =>
      parts((ps) => {
        const last = ps[ps.length - 1];
        if (last && last.type === type) last.text += delta;
        else ps.push({ type, text: delta });
      });
    const emit = (e: AgentEvent) => {
      switch (e.type) {
        case 'text':
        case 'thinking':
          append(e.type, e.delta);
          break;
        case 'tool':
          add({ type: 'tool', id: e.id, ref: e.ref, name: e.name, input: JSON.stringify(e.input) });
          break;
        case 'result':
          parts((ps) => {
            const p = ps.find((x) => x.type === 'tool' && x.id === e.id) as ToolPart | undefined;
            if (p) Object.assign(p, { summary: e.summary, result: e.content, isError: e.isError });
          });
          break;
        case 'comments':
          // the comments were shown when taken; the AI's answer continues below them
          setEntries(entries.length, { role: 'assistant', parts: [] });
          at = entries.length - 1;
          break;
        case 'usage':
          setUsage((u) => ({ input: (u?.input ?? 0) + e.input, output: (u?.output ?? 0) + e.output, cacheRead: (u?.cacheRead ?? 0) + e.cacheRead }));
          break;
        case 'stopped':
          add({ type: 'stopped', reason: e.reason });
          // the reason can name the provider or a private server address: it stays in this chat
          if (e.reason !== 'Stopped.' && !props.host.foreign()) props.main.log({ kind: 'error', text: 'The request stopped with an error (shown in the chat of whoever made it).' });
          break;
      }
      scroll();
    };
    setUsage(null);
    try {
      await run({ provider: provider(key(), settings()), host: props.host, messages, signal: abort.signal, emit, budget: settings().budget, interject: takeComments });
    } finally {
      abort = null;
      setBusy(false);
      props.onBusy(false);
      scroll();
    }
  };

  const send = async () => {
    const text = draft().trim();
    if (!text || busy() || !ready()) return;
    if (!noticeSeen()) return;
    const sel = attach() ? selection() : null;
    const doc = props.host.doc();
    messages.push({ role: 'user', content: userMessage(text, { context: doc.context, branch: props.host.branch()?.name ?? null, version: doc.versions().length }, sel) });
    setEntries(entries.length, { role: 'user', text, selection: sel ? selectionLabel(sel) : undefined });
    setDraft('');
    // the request's text stays in this chat: the document (shared with others) only records that one was made
    if (!props.host.foreign()) props.main.log({ kind: 'request', author: 'you', text: 'Asked the AI.' });
    await request();
  };

  // comments made while the AI is idle start a request of their own
  createEffect(() => {
    if (busy() || !ready() || !noticeSeen() || !waiting().length) return;
    untrack(() => {
      const texts = takeComments();
      messages.push({ role: 'user', content: texts.map((text) => ({ type: 'text' as const, text })) });
      void request();
    });
  });

  /** A tool call of this conversation by its reference. */
  const toolByRef = (ref: string): ToolPart | undefined => {
    for (const e of entries) if (e.role === 'assistant') for (const p of e.parts) if (p.type === 'tool' && p.ref === ref) return p;
    return undefined;
  };

  const stop = () => abort?.abort();
  const newConversation = () => {
    messages.length = 0;
    setEntries([]);
    setUsage(null);
  };


  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void send();
    }
    if (e.key === 'Escape') box.blur();
  };

  const open = () => props.proposals.filter((p) => p.state === 'open');
  const time = (t: number) => new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

  return (
    <aside class="ai" aria-label="AI panel" data-keeps-focus>
      <div class="ai-head row">
        <strong class="grow">AI</strong>
        <div class="tabs">
          <button classList={{ on: tab() === 'chat' }} onClick={() => setTab('chat')}>Chat</button>
          <button classList={{ on: tab() === 'activity' }} onClick={() => setTab('activity')} data-testid="activity-tab">Activity</button>
        </div>
        <a class="ghost model-link" href="#/settings" title="Provider, model and key: in Settings" data-testid="ai-model">{ready() ? modelName() || 'choose a model' : 'not set up'}</a>
        <button class="ghost" title={`Hide (${keysOf('ai')})`} onClick={props.onClose} aria-label="Hide the AI panel">×</button>
      </div>

      <Show when={!noticeSeen()}>
        <div class="banner ai-notice" data-testid="ai-notice">
          Every message sends your text, the selection and the terms involved to the provider you choose, with your key, straight from this browser.
          Nothing goes anywhere else.
          <div><button onClick={() => { markSeen(); setNoticeSeen(true); }}>I understand</button></div>
        </div>
      </Show>

      <Show when={props.host.foreign()}>
        <div class="banner" data-testid="ai-foreign">
          This document belongs to {props.owner || 'someone else'}. Its text goes to the AI with your messages and could be written to steer it,
          so every change the AI makes here waits for you to accept it.
        </div>
      </Show>

      <Show when={!ready()}>
        <div class="banner" data-testid="ai-setup">
          No AI provider is set up yet. <a href="#/settings" data-testid="ai-setup-link">Set one up in Settings</a>: choose a provider and model and enter your key.
        </div>
      </Show>

      <Switch>
        <Match when={tab() === 'activity'}>
          <div class="ai-log" data-testid="timeline">
            <Show when={timeline().length} fallback={<div class="hint">No AI activity yet.</div>}>
              <For each={timeline()}>
                {(t) => (
                  <div class={`tl tl-${t.kind}`}>
                    <span class="hint">{time(t.time)}</span>
                    <span class="tl-author">{t.author}</span>
                    <span class="grow">
                      {t.text}
                      <Show when={t.branch}><span class="branch-name small">⑂ {t.branch}</span></Show>
                      <Show when={t.detail && t.kind !== 'note' && t.kind !== 'milestone'}><div class="hint tl-detail">{t.detail}</div></Show>
                      <Show when={t.kind === 'milestone'}>
                        <Show when={t.detail}><div class="ai-text tl-summary" innerHTML={markdown(t.detail!)} /></Show>
                        <For each={t.snapshots ?? []}>
                          {(k) => (
                            <Show when={snapshots().find((x) => x.key === k)} keyed fallback={<div class="hint">A key diagram of this milestone is no longer in the document.</div>}>
                              {(snap) => (
                                <div class="tl-diagram" data-testid="milestone-diagram">
                                  <div class="row">
                                    <span class="hint grow">{snap.name}</span>
                                    <button class="ghost small" onClick={() => props.onSnapshot?.(k)}>Open</button>
                                  </div>
                                  <ResultView s={{ id: `milestone-${k}`, text: snap.cellText, status: 'fresh', value: snap.value as CellState['value'] }} />
                                </div>
                              )}
                            </Show>
                          )}
                        </For>
                      </Show>
                    </span>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </Match>
        <Match when={tab() === 'chat'}>
          <div class="ai-log" ref={log} data-testid="chat">
            <Show when={!entries.length}>
              <div class="hint">
                Ask about the workspace, or select pieces in the canvas and press {keysOf('ask')}. The AI calls the calculator's operations; it never does algebra itself.
                What-if questions run on a scratch branch you can accept or discard.
              </div>
            </Show>
            <For each={entries}>
              {(e) => (
                <Switch>
                  <Match when={e.role === 'user' && e}>
                    {(u) => (
                      <div class="msg user" classList={{ comment: !!u().cell }}>
                        <Show when={u().cell}><div class="hint" data-testid="comment-on">comment on {u().cell}</div></Show>
                        {u().text}
                        <Show when={u().selection}><div class="hint">with the selection: {u().selection}</div></Show>
                      </div>
                    )}
                  </Match>
                  <Match when={e.role === 'assistant' && e}>
                    {(a) => (
                      <div class="msg assistant">
                        <For each={a().parts}>
                          {(p) => (
                            <Switch>
                              <Match when={p.type === 'text' && p}>
                                {(t) => {
                                  // a claim's reference shows the call it comes from and its result
                                  const [shown, setShown] = createSignal<string | null>(null);
                                  const onClick = (e: MouseEvent) => {
                                    const b = (e.target as HTMLElement).closest<HTMLElement>('[data-ref]');
                                    if (b) setShown(shown() === b.dataset.ref ? null : b.dataset.ref!);
                                  };
                                  return (
                                    <>
                                      <div class="ai-text" innerHTML={markdown(t().text, { refs: true })} onClick={onClick} />
                                      <Show when={shown()} keyed>
                                        {(ref) => (
                                          <div class="claim" data-testid="claim" role="region" aria-label={`Operation ${ref}`}>
                                            <Show when={toolByRef(ref)} keyed fallback={<div class="hint">This reply cites {ref}, but no operation of this conversation has that reference.</div>}>
                                              {(c) => (
                                                <>
                                                  <div class="row">
                                                    <strong class="grow"><span class="mono">{ref}</span> <span class="toolname">{c.name}</span> <span class="hint">{c.summary ?? 'running…'}</span></strong>
                                                    <button class="ghost" aria-label="Close" onClick={() => setShown(null)}>×</button>
                                                  </div>
                                                  <div class="hint">Called with</div>
                                                  <pre class="textform">{readable(c.input)}</pre>
                                                  <div class="hint">Result</div>
                                                  <pre class="textform" classList={{ bad: !!c.isError }} data-testid="claim-result">{c.result === undefined ? 'not back yet' : readable(c.result)}</pre>
                                                </>
                                              )}
                                            </Show>
                                          </div>
                                        )}
                                      </Show>
                                    </>
                                  );
                                }}
                              </Match>
                              <Match when={p.type === 'thinking' && p}>
                                {(t) => <details class="ai-thinking"><summary>Thinking</summary><div class="ai-text" innerHTML={markdown(t().text)} /></details>}
                              </Match>
                              <Match when={p.type === 'tool' && p}>
                                {(t) => (
                                  <details class="ai-tool" classList={{ bad: !!t().isError, pending: !t().summary }}>
                                    <summary><span class="hint mono">{t().ref}</span> <span class="toolname">{t().name}</span> <span class="hint">{t().summary ?? 'running…'}</span></summary>
                                    <div class="textform">{t().input}</div>
                                    <Show when={t().result !== undefined}><pre class="textform">{readable(t().result!)}</pre></Show>
                                  </details>
                                )}
                              </Match>
                              <Match when={p.type === 'stopped' && p}>{(t) => <div class="error">{t().reason}</div>}</Match>
                            </Switch>
                          )}
                        </For>
                      </div>
                    )}
                  </Match>
                </Switch>
              )}
            </For>
            <For each={open()}>
              {(p) => (
                <div class="proposal" data-testid="proposal">
                  <div>
                    <strong>{p.kind === 'remove' ? `Remove ${p.target}?` : p.kind === 'write' ? `Let the AI ${p.target}?` : 'Accept the branch into the document?'}</strong>
                    <div class="hint">{p.note}</div>
                  </div>
                  <div class="row">
                    <button class="primary" onClick={() => props.onDecide(p, true)}>{p.kind === 'remove' ? 'Remove' : p.kind === 'write' ? 'Allow' : 'Accept'}</button>
                    <button onClick={() => props.onDecide(p, false)}>Reject</button>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Match>
      </Switch>

      <Show when={props.host.branch()}>
        {(b) => (
          <div class="ai-branch row">
            <span class="branch-name">⑂ {b().name}</span>
            <span class="hint grow">the workspace shows this branch</span>
          </div>
        )}
      </Show>

      <div class="ai-compose">
        <Show when={waiting().length}>
          <div class="hint" data-testid="comments-waiting">
            {waiting().length === 1 ? 'A comment on a cell waits' : `${waiting().length} comments on cells wait`} {!ready() ? 'until an AI provider is set up' : !noticeSeen() ? 'until you have read the note above' : 'for the AI to finish its step'}.
          </div>
        </Show>
        <Show when={selection()}>
          {(s) => (
            <label class="row hint" data-testid="ai-selection">
              <input type="checkbox" checked={attach()} onChange={(e) => setAttach(e.currentTarget.checked)} /> Selection: {selectionLabel(s())}
            </label>
          )}
        </Show>
        <textarea ref={box} rows={3} value={draft()} onInput={(e) => setDraft(e.currentTarget.value)} onKeyDown={onKey} aria-label="Message to the AI"
          disabled={!noticeSeen()}
          placeholder={!ready() ? 'Set up a provider and key in Settings first.' : 'Ask about the workspace; Enter sends, Shift+Enter starts a new line.'} />
        <div class="row">
          <Show when={busy()} fallback={<button class="primary" onClick={() => void send()} disabled={!draft().trim() || !noticeSeen()} data-testid="ai-send">Send</button>}>
            <button onClick={stop} data-testid="ai-stop">Stop</button>
          </Show>
          <button class="ghost" onClick={newConversation} disabled={busy()} title="Start a new conversation; the workspace and the activity stay">New conversation</button>
          <div class="grow" />
          <Show when={usage()}>{(u) => <span class="hint" title="Tokens in this request (input read from the cache)">{u().input + u().cacheRead} in ({u().cacheRead} cached), {u().output} out</span>}</Show>
        </div>
      </div>
    </aside>
  );
}

function seen(): boolean {
  try {
    return localStorage.getItem(NOTICE) === '1';
  } catch {
    return false;
  }
}
function markSeen() {
  try {
    localStorage.setItem(NOTICE, '1');
  } catch {
    /* storage unavailable: the notice shows again next time */
  }
}
