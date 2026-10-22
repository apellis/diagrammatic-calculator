// The AI panel: settings with the person's own key, the chat, proposals to accept or reject,
// and the activity timeline of AI steps.
import { createSignal, For, Match, onCleanup, onMount, Show, Switch } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type Anthropic from '@anthropic-ai/sdk';
import type { Doc, TimelineEntry } from '../doc/model';
import { run, userMessage, type AgentEvent } from '../ai/agent';
import { ANTHROPIC_MODELS, loadKey, loadSettings, provider } from '../ai/providers';
import type { Proposal, SelectionRef, ToolHost } from '../ai/tools';
import { isMac } from '../app';
import { markdown } from './markdown';

type Part =
  | { type: 'text'; text: string }
  | { type: 'thinking'; text: string }
  | { type: 'tool'; id: string; name: string; input: string; summary?: string; isError?: boolean }
  | { type: 'stopped'; reason: string };
type Entry = { role: 'user'; text: string; selection?: string } | { role: 'assistant'; parts: Part[] };

const K = isMac ? '⌘' : 'Ctrl+';
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
}

export function AIPanel(props: AIPanelProps) {
  // the provider, model and key are set on the settings page; read afresh when used
  const settings = () => loadSettings();
  const key = () => loadKey(settings().provider).key;
  const ready = () => !!key() || settings().provider === 'compatible';
  const modelName = () => ANTHROPIC_MODELS.find((m) => m.id === settings().model)?.name ?? settings().model;
  const [noticeSeen, setNoticeSeen] = createSignal(seen());
  const [entries, setEntries] = createStore<Entry[]>([]);
  const [draft, setDraft] = createSignal('');
  const [attach, setAttach] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [usage, setUsage] = createSignal<{ input: number; output: number; cacheRead: number } | null>(null);
  const [timeline, setTimeline] = createSignal<TimelineEntry[]>(props.main.timeline.toArray());
  const [tab, setTab] = createSignal<'chat' | 'activity'>('chat');
  const messages: Anthropic.MessageParam[] = [];
  let abort: AbortController | null = null;
  let box!: HTMLTextAreaElement;
  let log!: HTMLDivElement;

  const onTimeline = () => setTimeline(props.main.timeline.toArray());
  props.main.timeline.observe(onTimeline);
  onCleanup(() => {
    props.main.timeline.unobserve(onTimeline);
    abort?.abort();
  });
  onMount(() => props.focusRef?.(() => box?.focus()));

  const selection = (): SelectionRef | null => props.host.selection();
  const selectionLabel = (s: SelectionRef) =>
    `${s.cell}, summand ${s.summand}${s.layers.length ? `, ${s.layers.length} piece${s.layers.length > 1 ? 's' : ''}` : ''}`;

  const scroll = () => requestAnimationFrame(() => log && (log.scrollTop = log.scrollHeight));

  const send = async () => {
    const text = draft().trim();
    if (!text || busy() || !key() && settings().provider !== 'compatible') return;
    if (!noticeSeen()) return;
    const sel = attach() ? selection() : null;
    const doc = props.host.doc();
    messages.push({ role: 'user', content: userMessage(text, { context: doc.context, branch: props.host.branch()?.name ?? null, version: doc.versions().length }, sel) });
    setEntries(entries.length, { role: 'user', text, selection: sel ? selectionLabel(sel) : undefined });
    setEntries(entries.length, { role: 'assistant', parts: [] });
    const at = entries.length - 1;
    setDraft('');
    setBusy(true);
    props.onBusy(true);
    props.main.log({ kind: 'request', author: 'you', text: text.split('\n')[0].slice(0, 120) });
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
          add({ type: 'tool', id: e.id, name: e.name, input: JSON.stringify(e.input) });
          break;
        case 'result':
          parts((ps) => {
            const p = ps.find((x) => x.type === 'tool' && x.id === e.id) as Extract<Part, { type: 'tool' }> | undefined;
            if (p) Object.assign(p, { summary: e.summary, isError: e.isError });
          });
          break;
        case 'usage':
          setUsage((u) => ({ input: (u?.input ?? 0) + e.input, output: (u?.output ?? 0) + e.output, cacheRead: (u?.cacheRead ?? 0) + e.cacheRead }));
          break;
        case 'stopped':
          add({ type: 'stopped', reason: e.reason });
          if (e.reason !== 'Stopped.') props.main.log({ kind: 'error', text: e.reason });
          break;
      }
      scroll();
    };
    setUsage(null);
    try {
      await run({ provider: provider(key(), settings()), host: props.host, messages, signal: abort.signal, emit });
    } finally {
      abort = null;
      setBusy(false);
      props.onBusy(false);
      scroll();
    }
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
        <button class="ghost" title={`Hide (${K}J)`} onClick={props.onClose} aria-label="Hide the AI panel">×</button>
      </div>

      <Show when={!noticeSeen()}>
        <div class="banner ai-notice" data-testid="ai-notice">
          Every message sends your text, the selection and the terms involved to the provider you choose, with your key, straight from this browser.
          Nothing goes anywhere else.
          <div><button onClick={() => { markSeen(); setNoticeSeen(true); }}>I understand</button></div>
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
                      <Show when={t.detail && t.kind !== 'note'}><div class="hint tl-detail">{t.detail}</div></Show>
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
                Ask about the workspace, or select pieces in the canvas and press {K}L. The AI calls the calculator's operations; it never does algebra itself.
                What-if questions run on a scratch branch you can accept or discard.
              </div>
            </Show>
            <For each={entries}>
              {(e) => (
                <Switch>
                  <Match when={e.role === 'user' && e}>
                    {(u) => (
                      <div class="msg user">
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
                              <Match when={p.type === 'text' && p}>{(t) => <div class="ai-text" innerHTML={markdown(t().text)} />}</Match>
                              <Match when={p.type === 'thinking' && p}>
                                {(t) => <details class="ai-thinking"><summary>Thinking</summary><div class="ai-text" innerHTML={markdown(t().text)} /></details>}
                              </Match>
                              <Match when={p.type === 'tool' && p}>
                                {(t) => (
                                  <details class="ai-tool" classList={{ bad: !!t().isError, pending: !t().summary }}>
                                    <summary><span class="toolname">{t().name}</span> <span class="hint">{t().summary ?? 'running…'}</span></summary>
                                    <div class="textform">{t().input}</div>
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
                    <strong>{p.kind === 'remove' ? `Remove ${p.target}?` : 'Accept the branch into the document?'}</strong>
                    <div class="hint">{p.note}</div>
                  </div>
                  <div class="row">
                    <button class="primary" onClick={() => props.onDecide(p, true)}>{p.kind === 'remove' ? 'Remove' : 'Accept'}</button>
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
