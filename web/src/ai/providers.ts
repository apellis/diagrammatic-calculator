// Model providers, called straight from the browser with the person's own key.
//
// Anthropic goes through the official SDK, which opts in to browser calls. OpenAI goes through
// its Responses API, and a local OpenAI-compatible server through the chat-completions protocol;
// those two are offered only after `checkProvider` shows the browser can reach them, since not
// every provider accepts browser calls. The conversation is kept in the Anthropic message format
// and translated for the others.
import Anthropic from '@anthropic-ai/sdk';
import type { ToolDef } from './tools';
import { readChat, readResponses, type ResponseOutput } from './provider-stream';

export type ProviderId = 'anthropic' | 'openai' | 'compatible';
export type Effort = 'low' | 'medium' | 'high' | 'xhigh' | 'max';

export interface AISettings {
  provider: ProviderId;
  model: string;
  effort: Effort;
  /** For an OpenAI-compatible server, its base URL (ending in /v1). */
  baseUrl: string;
  /** The most tokens (in, cached, and out) one request may use before it stops; 0 for no limit. */
  budget: number;
}

export const BUDGETS = [250_000, 1_000_000, 4_000_000, 0];

export const ANTHROPIC_MODELS: { id: string; name: string }[] = [
  { id: 'claude-opus-5-5', name: 'Claude Opus 5.5' },
  { id: 'claude-sonnet-5-5', name: 'Claude Sonnet 5.5' },
  { id: 'claude-haiku-5-5', name: 'Claude Haiku 5.5' },
  { id: 'claude-fable-5-1', name: 'Claude Fable 5.1' },
];

export const DEFAULT_SETTINGS: AISettings = { provider: 'anthropic', model: 'claude-opus-5-5', effort: 'high', baseUrl: '', budget: 1_000_000 };

export interface TurnRequest {
  system: string;
  tools: ToolDef[];
  messages: Anthropic.MessageParam[];
  signal: AbortSignal;
  onText: (delta: string) => void;
  onThinking: (delta: string) => void;
}

/** One model turn: the assistant's content and why it stopped. */
export interface Turn {
  content: Anthropic.ContentBlock[];
  stop: Anthropic.StopReason | null;
  usage?: { input: number; output: number; cacheRead: number; cacheWrite: number };
}

export interface Provider {
  label: string;
  turn(r: TurnRequest): Promise<Turn>;
}

/** Why a call failed, in words for the person. */
export function explain(e: unknown, provider: string): string {
  if (e instanceof Anthropic.APIUserAbortError) return 'Stopped.';
  if (e instanceof Anthropic.AuthenticationError) return `${provider} refused the key. Check it in the AI settings.`;
  if (e instanceof Anthropic.PermissionDeniedError) return `${provider} says this key may not use this model or feature.`;
  if (e instanceof Anthropic.RateLimitError) return `${provider} is rate-limiting this key; wait a moment and send again.`;
  if (e instanceof Anthropic.APIConnectionError) return `Could not reach ${provider}: ${e.message}`;
  if (e instanceof Anthropic.APIError) return `${provider} answered with an error (${e.status ?? 'no status'}): ${e.message}`;
  if (e instanceof BrowserBlocked) return e.message;
  if (e instanceof DOMException && e.name === 'AbortError') return 'Stopped.';
  return `Something went wrong: ${(e as Error)?.message ?? String(e)}`;
}

export function anthropic(key: string, s: AISettings): Provider {
  const client = new Anthropic({ apiKey: key, dangerouslyAllowBrowser: true, maxRetries: 2 });
  return {
    label: 'Anthropic',
    async turn(r) {
      // the system prompt and the tool list are the stable prefix: cache them
      const tools = r.tools.map((t, i) => ({
        ...t,
        input_schema: t.input_schema as Anthropic.Tool.InputSchema,
        eager_input_streaming: true,
        ...(i === r.tools.length - 1 ? { cache_control: { type: 'ephemeral' as const } } : {}),
      }));
      const stream = client.messages.stream(
        {
          model: s.model,
          max_tokens: 32_000,
          system: [{ type: 'text', text: r.system, cache_control: { type: 'ephemeral' } }],
          tools,
          thinking: { type: 'adaptive', display: 'summarized' },
          ...(s.model.includes('haiku') ? {} : { output_config: { effort: s.effort } }),
          cache_control: { type: 'ephemeral' },
          messages: r.messages,
        },
        { signal: r.signal },
      );
      stream.on('text', (d) => r.onText(d));
      stream.on('thinking', (d) => r.onThinking(d));
      const m = await stream.finalMessage();
      const u = m.usage;
      return {
        content: m.content,
        stop: m.stop_reason,
        usage: { input: u.input_tokens, output: u.output_tokens, cacheRead: u.cache_read_input_tokens ?? 0, cacheWrite: u.cache_creation_input_tokens ?? 0 },
      };
    },
  };
}

/** A provider that refuses calls made from a browser page. */
export class BrowserBlocked extends Error {}

const base = (s: AISettings) => (s.provider === 'openai' ? 'https://api.openai.com/v1' : s.baseUrl.trim().replace(/\/+$/, ''));
const providerName = (s: AISettings) => (s.provider === 'anthropic' ? 'Anthropic' : s.provider === 'openai' ? 'OpenAI' : `the server at ${base(s)}`);

/** How long the connection check waits for an answer. */
const CHECK_TIMEOUT_MS = 30_000;

/**
 * The browser check: can this page call the provider with this key? It makes the same kind of
 * call the chat makes (for OpenAI a minimal Responses request with the chosen model, a few tokens),
 * so a check that passes means the chat can reach the provider, and one that fails says why.
 */
export async function checkProvider(key: string, s: AISettings): Promise<{ ok: boolean; message: string }> {
  const name = providerName(s);
  if (s.provider === 'anthropic') {
    try {
      const client = new Anthropic({ apiKey: key, dangerouslyAllowBrowser: true, maxRetries: 0, timeout: CHECK_TIMEOUT_MS });
      await client.models.list({ limit: 1 });
      return { ok: true, message: `${name} accepts calls from this page.` };
    } catch (e) {
      return { ok: false, message: explain(e, name) };
    }
  }
  if (s.provider === 'compatible' && !hasServerUrl(s)) return { ok: false, message: 'Enter the server URL first (for example https://my-server.example/v1).' };
  if (s.provider === 'openai' && !s.model) return { ok: false, message: 'Choose a model first: the check sends it a one-word request.' };
  let res: Response;
  try {
    res =
      s.provider === 'openai'
        ? await fetch(`${base(s)}/responses`, {
            method: 'POST',
            signal: AbortSignal.timeout(CHECK_TIMEOUT_MS),
            headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${key}` },
            body: JSON.stringify({ model: s.model, input: 'Reply with OK.', max_output_tokens: 16, store: false }),
          })
        : await fetch(`${base(s)}/models`, { signal: AbortSignal.timeout(CHECK_TIMEOUT_MS), headers: key ? { Authorization: `Bearer ${key}` } : {} });
  } catch (e) {
    if ((e as Error).name === 'TimeoutError') return { ok: false, message: `${name} did not answer within ${CHECK_TIMEOUT_MS / 1000} seconds.` };
    return { ok: false, message: blockedMessage(name, (e as Error).message) };
  }
  if (res.status === 401) return { ok: false, message: `${name} refused the key.` };
  if (!res.ok) return { ok: false, message: `${name} answered ${res.status}: ${await errorText(res)}` };
  return { ok: true, message: s.provider === 'openai' ? `${name} accepts calls from this page with ${s.model}.` : `${name} accepts calls from this page.` };
}

/** The error message in a provider's error answer, else the start of its text. */
async function errorText(res: Response): Promise<string> {
  const text = await res.text();
  try {
    const m = (JSON.parse(text) as { error?: { message?: string } }).error?.message;
    if (m) return m;
  } catch {
    /* not JSON */
  }
  return text.slice(0, 300) || res.statusText;
}

export interface ModelOption {
  id: string;
  name: string;
}

/** Model ids that are not chat models, in OpenAI's list (embeddings, speech, images, moderation, and the like). */
const NOT_CHAT = /embed|tts|whisper|dall-e|image|audio|realtime|moderation|transcribe|search|babbage|davinci|computer-use|sora/i;

/**
 * The models this key may use, newest first, from the provider's own list. Anthropic's comes with
 * display names; OpenAI's is filtered to chat models; an OpenAI-compatible server's is taken as it is.
 */
export async function listModels(key: string, s: AISettings): Promise<ModelOption[]> {
  const name = providerName(s);
  if (s.provider === 'anthropic') {
    const client = new Anthropic({ apiKey: key, dangerouslyAllowBrowser: true, maxRetries: 1 });
    const out: ModelOption[] = [];
    try {
      for await (const m of client.models.list({ limit: 100 })) out.push({ id: m.id, name: m.display_name || m.id });
    } catch (e) {
      throw new Error(explain(e, name));
    }
    return out;
  }
  let res: Response;
  try {
    res = await fetch(`${base(s)}/models`, { headers: key ? { Authorization: `Bearer ${key}` } : {} });
  } catch (e) {
    throw new Error(blockedMessage(name, (e as Error).message));
  }
  if (res.status === 401) throw new Error(`${name} refused the key.`);
  if (!res.ok) throw new Error(`${name} answered ${res.status} ${res.statusText}.`);
  const body = (await res.json()) as { data?: { id: string; created?: number }[] };
  return (body.data ?? [])
    .filter((m) => s.provider !== 'openai' || !NOT_CHAT.test(m.id))
    .sort((a, b) => (b.created ?? 0) - (a.created ?? 0) || a.id.localeCompare(b.id))
    .map((m) => ({ id: m.id, name: m.id }));
}

/** Where a provider's model list is kept between visits: per provider, and per server for a compatible one. */
const modelsName = (s: AISettings) => `diagcalc.ai.models.${s.provider}${s.provider === 'compatible' ? `.${base(s)}` : ''}`;

export function loadModels(s: AISettings): { models: ModelOption[]; fetched: number } | null {
  try {
    const v = JSON.parse(localStorage.getItem(modelsName(s)) ?? 'null');
    return v && Array.isArray(v.models) ? v : null;
  } catch {
    return null;
  }
}

export function saveModels(s: AISettings, models: ModelOption[]) {
  try {
    localStorage.setItem(modelsName(s), JSON.stringify({ models, fetched: Date.now() }));
  } catch {
    /* storage unavailable: the list lives in memory for this page only */
  }
}

/** A call the browser could not make at all; the browser does not say why, so the likely causes are listed. */
const blockedMessage = (name: string, detail?: string) =>
  `The browser could not reach ${name}${detail ? ` (${detail})` : ''}. The network may be down, a browser extension may block the call, or ${name} refuses calls made from a browser (or is not running). In the last case choose another provider in Settings.`;

type ChatMessage =
  | { role: 'system' | 'user'; content: string }
  | { role: 'assistant'; content: string | null; refusal?: string; tool_calls?: { id: string; type: 'function'; function: { name: string; arguments: string } }[] }
  | { role: 'tool'; tool_call_id: string; content: string };

// As with Responses below, preserve wire arguments in memory rather than re-encoding JSON.
const chatHistory = new WeakMap<object, ChatMessage>();

/** The conversation in the chat-completions format. */
export function toChat(system: string, messages: Anthropic.MessageParam[]): ChatMessage[] {
  const out: ChatMessage[] = [{ role: 'system', content: system }];
  for (const m of messages) {
    const original = m.role === 'assistant' && typeof m.content !== 'string' ? chatHistory.get(m.content) : undefined;
    if (original) { out.push(original); continue; }
    const blocks = typeof m.content === 'string' ? [{ type: 'text' as const, text: m.content }] : m.content;
    if (m.role === 'user') {
      for (const b of blocks) {
        if (b.type === 'tool_result') {
          const c = typeof b.content === 'string' ? b.content : (b.content ?? []).map((x) => ('text' in x ? x.text : '')).join('');
          out.push({ role: 'tool', tool_call_id: b.tool_use_id, content: c });
        }
      }
      const text = blocks.filter((b) => b.type === 'text').map((b) => (b as { text: string }).text).join('\n');
      if (text) out.push({ role: 'user', content: text });
    } else {
      const text = blocks.filter((b) => b.type === 'text').map((b) => (b as { text: string }).text).join('\n');
      const calls = blocks.filter((b) => b.type === 'tool_use') as Anthropic.ToolUseBlockParam[];
      out.push({
        role: 'assistant',
        content: text || null,
        ...(calls.length ? { tool_calls: calls.map((c) => ({ id: c.id, type: 'function' as const, function: { name: c.name, arguments: JSON.stringify(c.input) } })) } : {}),
      });
    }
  }
  return out;
}

export function chatCompletions(key: string, s: AISettings): Provider {
  const name = providerName(s);
  return {
    label: name,
    async turn(r) {
      let res: Response;
      try {
        res = await fetch(`${base(s)}/chat/completions`, {
          method: 'POST',
          signal: r.signal,
          headers: { 'Content-Type': 'application/json', ...(key ? { Authorization: `Bearer ${key}` } : {}) },
          body: JSON.stringify({
            model: s.model,
            messages: toChat(r.system, r.messages),
            stream: true,
            stream_options: { include_usage: true },
            tools: r.tools.map((t) => ({ type: 'function', function: { name: t.name, description: t.description, parameters: t.input_schema } })),
          }),
        });
      } catch (e) {
        if ((e as Error).name === 'AbortError') throw e;
        throw new BrowserBlocked(blockedMessage(name, (e as Error).message));
      }
      if (!res.ok) throw new Error(`${name} answered ${res.status}: ${(await res.text()).slice(0, 300)}`);
      const { body: j, streamed } = await readChat(res, r);
      const choice = j.choices[0];
      const content: Anthropic.ContentBlock[] = [];
      if (choice.message.content) {
        if (!streamed) r.onText(choice.message.content);
        content.push({ type: 'text', text: choice.message.content, citations: null } as Anthropic.TextBlock);
      }
      if (choice.message.refusal) {
        if (!streamed) r.onText(choice.message.refusal);
        content.push({ type: 'text', text: choice.message.refusal, citations: null });
      }
      for (const c of choice.message.tool_calls ?? []) {
        if (!c.id || !c.function.name || (c.type && c.type !== 'function')) throw new Error('Provider returned an invalid tool call.');
        let input: unknown;
        try {
          input = JSON.parse(c.function.arguments || '{}');
        } catch {
          input = { INVALID_JSON: c.function.arguments };
        }
        content.push({ type: 'tool_use', id: c.id, name: c.function.name, input } as Anthropic.ToolUseBlock);
      }
      const stop: Anthropic.StopReason =
        choice.message.refusal || choice.finish_reason === 'content_filter' ? 'refusal'
        : choice.finish_reason === 'tool_calls' ? 'tool_use' : choice.finish_reason === 'length' ? 'max_tokens' : 'end_turn';
      const cached = j.usage?.prompt_tokens_details?.cached_tokens ?? 0;
      chatHistory.set(content, {
        role: 'assistant', content: choice.message.content ?? null,
        ...(choice.message.refusal ? { refusal: choice.message.refusal } : {}),
        ...(choice.message.tool_calls?.length ? { tool_calls: choice.message.tool_calls.map((c) => ({ id: c.id, type: 'function' as const, function: { ...c.function } })) } : {}),
      });
      return { content, stop, usage: { input: Math.max(0, (j.usage?.prompt_tokens ?? 0) - cached), output: j.usage?.completion_tokens ?? 0, cacheRead: cached, cacheWrite: 0 } };
    },
  };
}

type ResponseItem =
  | ResponseOutput
  | { role: 'user' | 'assistant'; content: string }
  | { type: 'function_call'; call_id: string; name: string; arguments: string }
  | { type: 'function_call_output'; call_id: string; output: string };

// The agent and chat panel retain each content array by reference for the lifetime of the
// conversation. Keep the exact provider output alongside it, not in Anthropic blocks, storage,
// or UI text. Recreating a Provider between requests must not lose the continuation. Clearing
// the conversation releases this state; serialization intentionally does not persist it.
// https://developers.openai.com/api/docs/guides/reasoning#preserve-reasoning-without-stored-responses
const responseHistory = new WeakMap<object, ResponseOutput[]>();

/** The conversation as input items of OpenAI's Responses API. */
export function toResponses(messages: Anthropic.MessageParam[]): ResponseItem[] {
  const out: ResponseItem[] = [];
  for (const m of messages) {
    const original = m.role === 'assistant' && typeof m.content !== 'string' ? responseHistory.get(m.content) : undefined;
    if (original) { out.push(...original); continue; }
    const blocks = typeof m.content === 'string' ? [{ type: 'text' as const, text: m.content }] : m.content;
    for (const b of blocks) {
      if (b.type === 'text' && b.text) out.push({ role: m.role === 'assistant' ? 'assistant' : 'user', content: b.text });
      else if (b.type === 'tool_use') out.push({ type: 'function_call', call_id: b.id, name: b.name, arguments: JSON.stringify(b.input) });
      else if (b.type === 'tool_result') {
        const c = typeof b.content === 'string' ? b.content : (b.content ?? []).map((x) => ('text' in x ? x.text : '')).join('');
        out.push({ type: 'function_call_output', call_id: b.tool_use_id, output: c });
      }
    }
  }
  return out;
}

/**
 * OpenAI through its Responses API, which (unlike chat completions) lets reasoning models call
 * tools. Nothing is stored at OpenAI (`store: false`); the model's own reasoning default applies.
 */
export function openaiResponses(key: string, s: AISettings): Provider {
  const name = providerName(s);
  return {
    label: name,
    async turn(r) {
      let res: Response;
      try {
        res = await fetch(`${base(s)}/responses`, {
          method: 'POST',
          signal: r.signal,
          headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${key}` },
          body: JSON.stringify({
            model: s.model,
            instructions: r.system,
            input: toResponses(r.messages),
            tools: r.tools.map((t) => ({ type: 'function', name: t.name, description: t.description, parameters: t.input_schema, strict: false })),
            store: false,
            stream: true,
            include: ['reasoning.encrypted_content'],
          }),
        });
      } catch (e) {
        if ((e as Error).name === 'AbortError') throw e;
        throw new BrowserBlocked(blockedMessage(name, (e as Error).message));
      }
      if (!res.ok) throw new Error(`${name} answered ${res.status}: ${(await res.text()).slice(0, 300)}`);
      const { body: j, streamed } = await readResponses(res, r);
      const content: Anthropic.ContentBlock[] = [];
      let refused = false;
      for (const item of j.output ?? []) {
        if (item.type === 'message' && 'content' in item) {
          for (const c of item.content ?? []) {
            if (c.type === 'refusal') refused = true;
            const text = c.type === 'output_text' ? c.text : c.type === 'refusal' ? c.refusal : undefined;
            if (!text) continue;
            if (!streamed) r.onText(text);
            content.push({ type: 'text', text, citations: null } as Anthropic.TextBlock);
          }
        } else if (item.type === 'function_call' && 'call_id' in item) {
          if (!item.call_id || !item.name || typeof item.arguments !== 'string') throw new Error('Provider returned an invalid tool call.');
          let input: unknown;
          try {
            input = JSON.parse(item.arguments || '{}');
          } catch {
            input = { INVALID_JSON: item.arguments };
          }
          content.push({ type: 'tool_use', id: item.call_id, name: item.name, input } as Anthropic.ToolUseBlock);
        }
      }
      const reason = j.status === 'incomplete' ? j.incomplete_details?.reason : undefined;
      const stop: Anthropic.StopReason =
        refused || reason === 'content_filter' ? 'refusal'
        : reason === 'max_output_tokens' ? 'max_tokens'
        : content.some((b) => b.type === 'tool_use') ? 'tool_use'
        : 'end_turn';
      const u = j.usage;
      // Like OpenAI's toResponseInputItems: created_by is output-only provenance.
      // No SDK-only parsed fields exist here because these are raw wire items.
      responseHistory.set(content, (j.output ?? []).map(({ created_by: _createdBy, ...item }) => item));
      const cached = u?.input_tokens_details?.cached_tokens ?? 0;
      return { content, stop, usage: { input: Math.max(0, (u?.input_tokens ?? 0) - cached), output: u?.output_tokens ?? 0, cacheRead: cached, cacheWrite: 0 } };
    },
  };
}

export function provider(key: string, s: AISettings): Provider {
  if (s.provider === 'anthropic') return anthropic(key, s);
  return s.provider === 'openai' ? openaiResponses(key, s) : chatCompletions(key, s);
}

// keys: for the session by default, in this browser's storage only when asked

const keyName = (p: ProviderId) => `diagcalc.ai.key.${p}`;

export function loadKey(p: ProviderId): { key: string; remembered: boolean } {
  try {
    const remembered = localStorage.getItem(keyName(p));
    if (remembered) return { key: remembered, remembered: true };
    return { key: sessionStorage.getItem(keyName(p)) ?? '', remembered: false };
  } catch {
    return { key: '', remembered: false };
  }
}

export function saveKey(p: ProviderId, key: string, remember: boolean) {
  try {
    localStorage.removeItem(keyName(p));
    sessionStorage.removeItem(keyName(p));
    if (key) (remember ? localStorage : sessionStorage).setItem(keyName(p), key);
  } catch {
    /* storage unavailable: the key lives in memory for this page only */
  }
}

/** The server URL once offered by default; it was never chosen, so it is dropped when read. */
const OLD_DEFAULT_URL = 'http://localhost:11434/v1';

export function loadSettings(): AISettings {
  try {
    const s: AISettings = { ...DEFAULT_SETTINGS, ...JSON.parse(localStorage.getItem('diagcalc.ai.settings') ?? '{}') };
    if (!BUDGETS.includes(s.budget)) s.budget = DEFAULT_SETTINGS.budget;
    return s.baseUrl === OLD_DEFAULT_URL ? { ...s, baseUrl: '' } : s;
  } catch {
    return DEFAULT_SETTINGS;
  }
}

/** Whether a server URL has been entered that can be called: http or https with a host. */
export const hasServerUrl = (s: AISettings) => /^https?:\/\/[^\s/]+/.test(s.baseUrl.trim());

/** Whether the AI can be used with these settings and key: a key, or for a compatible server its URL. */
export const isSetUp = (s: AISettings, key: string) => (s.provider === 'compatible' ? hasServerUrl(s) : !!key);

export function saveSettings(s: AISettings) {
  try {
    localStorage.setItem('diagcalc.ai.settings', JSON.stringify(s));
  } catch {
    /* storage unavailable */
  }
}
