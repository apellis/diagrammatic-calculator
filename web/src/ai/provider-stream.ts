// HTTP/SSE transport only. No SDK, durable storage, retries, or provider credentials here.
import type { TurnRequest } from './providers';

export const isEventStream = (res: Response) => res.headers.get('content-type')?.split(';')[0].trim().toLowerCase() === 'text/event-stream';

/** SSE framing is independent of UTF-8/network chunks (including split CRLFs). */
export async function* eventData(res: Response, signal: AbortSignal): AsyncGenerator<string> {
  if (!res.body) throw new Error('Provider stream has no body.');
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let line = '', data: string[] = [], skipLF = false;
  const abort = () => { void reader.cancel().catch(() => {}); };
  signal.addEventListener('abort', abort, { once: true });
  try {
    signal.throwIfAborted();
    while (true) {
      const chunk = await reader.read();
      signal.throwIfAborted();
      const text = decoder.decode(chunk.value, { stream: !chunk.done });
      for (const char of text) {
        signal.throwIfAborted();
        if (skipLF && char === '\n') { skipLF = false; continue; }
        skipLF = false;
        if (char !== '\r' && char !== '\n') { line += char; continue; }
        skipLF = char === '\r';
        if (!line) {
          if (data.length) { const value = data.join('\n'); data = []; yield value; }
        } else if (line === 'data' || line.startsWith('data:')) {
          data.push(line === 'data' ? '' : line.slice(5).replace(/^ /, ''));
        }
        line = '';
      }
      // A partial event at EOF is not a complete event, per SSE framing rules.
      if (chunk.done) break;
    }
  } finally {
    signal.removeEventListener('abort', abort);
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

export interface ResponseOutput {
  type: string;
  [key: string]: unknown;
  content?: { type: string; text?: string; refusal?: string }[];
  call_id?: string;
  name?: string;
  arguments?: string;
}
export interface ResponsesBody {
  status?: string;
  error?: { message?: string } | null;
  incomplete_details?: { reason?: string } | null;
  output?: ResponseOutput[];
  usage?: { input_tokens?: number; output_tokens?: number; input_tokens_details?: { cached_tokens?: number } };
}

/** Terminal Responses events carry the authoritative ordered output (including opaque reasoning).
 * Do not reconstruct that output from display deltas or stringify parsed tool arguments.
 * https://developers.openai.com/api/docs/guides/streaming-responses
 */
export async function readResponses(res: Response, r: TurnRequest): Promise<{ body: ResponsesBody; streamed: boolean }> {
  if (!isEventStream(res)) {
    const body = await res.json() as ResponsesBody;
    r.signal.throwIfAborted();
    checkResponse(body);
    return { body, streamed: false };
  }
  for await (const data of eventData(res, r.signal)) {
    const event = JSON.parse(data) as { type: string; delta?: string; message?: string; error?: { message?: string }; response?: ResponsesBody };
    if (event.type === 'error' || event.error) throw new Error(event.message ?? event.error?.message ?? 'Provider stream failed.');
    if (event.type === 'response.output_text.delta' || event.type === 'response.refusal.delta') {
      if (event.delta) r.onText(event.delta);
    } else if (event.type === 'response.reasoning_summary_text.delta') {
      if (event.delta) r.onThinking(event.delta);
    } else if (['response.completed', 'response.incomplete', 'response.failed', 'response.cancelled'].includes(event.type)) {
      if (!event.response) throw new Error('Provider terminal event has no response.');
      if (event.type === 'response.failed' || event.type === 'response.cancelled') throw new Error(event.response.error?.message ?? `Provider ${event.type}.`);
      checkResponse(event.response);
      return { body: event.response, streamed: true };
    }
  }
  throw new Error('Provider stream ended without a terminal response.');
}
function checkResponse(body: ResponsesBody) {
  if (body.error || body.status === 'failed' || body.status === 'cancelled') throw new Error(body.error?.message ?? `Provider response ${body.status}.`);
  if (!Array.isArray(body.output)) throw new Error('Provider response has no output.');
}

export interface ChatCall { id: string; type?: string; function: { name: string; arguments: string } }
export interface ChatBody {
  choices: { index?: number; finish_reason: string | null; message: { content?: string | null; tool_calls?: ChatCall[]; refusal?: string | null } }[];
  usage?: { prompt_tokens?: number; completion_tokens?: number; prompt_tokens_details?: { cached_tokens?: number } };
}
export async function readChat(res: Response, r: TurnRequest): Promise<{ body: ChatBody; streamed: boolean }> {
  if (!isEventStream(res)) {
    const body = await res.json() as ChatBody & { error?: { message?: string } };
    r.signal.throwIfAborted();
    if (body.error) throw new Error(body.error.message ?? 'Provider failed.');
    if (!body.choices?.[0]?.message) throw new Error('Provider response has no assistant message.');
    return { body, streamed: false };
  }
  let content = '', refusal = '', finish: string | null = null, usage: ChatBody['usage'];
  const calls = new Map<number, ChatCall>();
  for await (const data of eventData(res, r.signal)) {
    if (data === '[DONE]') {
      if (!finish) throw new Error('Provider stream ended without a finish reason.');
      return { body: { choices: [{ finish_reason: finish, message: { content, refusal, tool_calls: [...calls].sort(([a], [b]) => a - b).map(([, c]) => c) } }], usage }, streamed: true };
    }
    const chunk = JSON.parse(data) as {
      error?: { message?: string }; usage?: ChatBody['usage'];
      choices?: { index: number; finish_reason?: string | null; delta?: { content?: string | null; refusal?: string | null; tool_calls?: { index: number; id?: string; type?: string; function?: { name?: string; arguments?: string } }[] } }[];
    };
    if (chunk.error) throw new Error(chunk.error.message ?? 'Provider stream failed.');
    if (chunk.usage) usage = chunk.usage;
    const choice = chunk.choices?.find((c) => c.index === 0);
    if (!choice) continue;
    if (choice.finish_reason) finish = choice.finish_reason;
    const delta = choice.delta;
    if (delta?.content) { content += delta.content; r.onText(delta.content); }
    if (delta?.refusal) { refusal += delta.refusal; r.onText(delta.refusal); }
    for (const part of delta?.tool_calls ?? []) {
      if (!Number.isInteger(part.index) || part.index < 0) throw new Error('Provider tool delta has no valid index.');
      const call = calls.get(part.index) ?? { id: '', function: { name: '', arguments: '' } };
      if (part.id) call.id = part.id;
      if (part.type) call.type = part.type;
      call.function.name += part.function?.name ?? '';
      call.function.arguments += part.function?.arguments ?? '';
      calls.set(part.index, call);
    }
  }
  throw new Error('Provider stream ended without [DONE].');
}
