import type Anthropic from '@anthropic-ai/sdk';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DEFAULT_SETTINGS, openaiResponses, chatCompletions, toResponses, toChat, type TurnRequest } from './providers';

const settings = { ...DEFAULT_SETTINGS, provider: 'openai' as const, model: 'test-model' };
const request = (overrides: Partial<TurnRequest> = {}): TurnRequest => ({
  system: 'system', tools: [], messages: [{ role: 'user', content: 'hello' }],
  signal: new AbortController().signal, onText: vi.fn(), onThinking: vi.fn(), ...overrides,
});
const frame = (data: unknown) => `: keepalive\r\nevent: update\r\ndata: ${typeof data === 'string' ? data : JSON.stringify(data)}\r\n\r\n`;
// Real Response/ReadableStream, with UTF-8 and SSE lines split at every byte.
function sse(events: unknown[]) {
  const bytes = new TextEncoder().encode(events.map(frame).join(''));
  return new Response(new ReadableStream<Uint8Array>({ start(c) {
    for (const b of bytes) c.enqueue(new Uint8Array([b]));
    c.close();
  } }), { headers: { 'Content-Type': 'text/event-stream; charset=utf-8' } });
}
const message = (text: string) => ({ type: 'message', id: 'msg_1', role: 'assistant', status: 'completed', phase: 'final_answer', content: [{ type: 'output_text', text, annotations: [] }] });
const reasoning = { type: 'reasoning', id: 'rs_1', summary: [{ type: 'summary_text', text: 'Checking.' }], encrypted_content: 'opaque-provider-ciphertext' };
const call = { type: 'function_call', id: 'fc_1', call_id: 'call_1', name: 'inspect', arguments: '{ "x": 1 }', status: 'completed' };
afterEach(() => vi.unstubAllGlobals());

describe.each(['responses', 'chat'] as const)('%s lifecycle', (protocol) => {
  const provider = () => protocol === 'responses' ? openaiResponses('', settings) : chatCompletions('', { ...settings, provider: 'compatible', baseUrl: 'https://example.invalid/v1' });
  it('rejects a truncated stream instead of returning partial tool calls', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(sse(protocol === 'responses' ? [{ type: 'response.output_text.delta', delta: 'partial' }] : [{ choices: [{ index: 0, delta: { content: 'partial' }, finish_reason: null }] }])));
    await expect(provider().turn(request())).rejects.toThrow(/ended|incomplete|terminal/i);
  });
  it('surfaces streamed errors', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(sse([{ type: 'error', error: { message: 'provider broke' }, message: 'provider broke' }])));
    await expect(provider().turn(request())).rejects.toThrow('provider broke');
  });
  it('cancels a blocked reader and preserves AbortError', async () => {
    const cancel = vi.fn();
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(new ReadableStream({ cancel }), { headers: { 'Content-Type': 'text/event-stream' } })));
    const ac = new AbortController();
    const pending = provider().turn(request({ signal: ac.signal }));
    const rejection = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
    await Promise.resolve();
    ac.abort();
    await rejection;
    expect(cancel).toHaveBeenCalled();
  });
});

describe('compatible chat streaming', () => {
  const provider = () => chatCompletions('', { ...settings, provider: 'compatible', baseUrl: 'https://example.invalid/v1' });
  it('assembles interleaved tool arguments by index and reads usage after finish', async () => {
    const fetcher = vi.fn().mockResolvedValue(sse([
      { choices: [{ index: 0, delta: { content: 'hé🙂' }, finish_reason: null }] },
      { choices: [{ index: 0, delta: { tool_calls: [{ index: 1, id: 'b', type: 'function', function: { name: 'two', arguments: '{' } }, { index: 0, id: 'a', type: 'function', function: { name: 'one', arguments: '{ "x":' } }] } }] },
      { choices: [{ index: 0, delta: { tool_calls: [{ index: 0, function: { arguments: ' 1 }' } }, { index: 1, function: { arguments: '}' } }] } }] },
      { choices: [{ index: 0, delta: {}, finish_reason: 'tool_calls' }] },
      { choices: [], usage: { prompt_tokens: 20, completion_tokens: 4, prompt_tokens_details: { cached_tokens: 5 } } },
      '[DONE]',
    ]));
    vi.stubGlobal('fetch', fetcher);
    const r = request();
    const t = await provider().turn(r);
    expect(r.onText).toHaveBeenCalledExactlyOnceWith('hé🙂');
    expect(t.content).toMatchObject([{ type: 'text', text: 'hé🙂' }, { type: 'tool_use', id: 'a', name: 'one', input: { x: 1 } }, { type: 'tool_use', id: 'b', name: 'two', input: {} }]);
    expect(t.stop).toBe('tool_use');
    expect(toChat('system', [{ role: 'assistant', content: t.content }])[1]).toEqual({ role: 'assistant', content: 'hé🙂', tool_calls: [
      { id: 'a', type: 'function', function: { name: 'one', arguments: '{ "x": 1 }' } },
      { id: 'b', type: 'function', function: { name: 'two', arguments: '{}' } },
    ] });
    expect(t.usage).toEqual({ input: 15, output: 4, cacheRead: 5, cacheWrite: 0 });
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ stream: true, stream_options: { include_usage: true } });
  });
  it('emits text while the server is still generating', async () => {
    let c!: ReadableStreamDefaultController<Uint8Array>;
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(new ReadableStream<Uint8Array>({ start(controller) { c = controller; } }), { headers: { 'Content-Type': 'text/event-stream' } })));
    const r = request();
    const pending = provider().turn(r);
    c.enqueue(new TextEncoder().encode(frame({ choices: [{ index: 0, delta: { content: 'first' } }] })));
    await vi.waitFor(() => expect(r.onText).toHaveBeenCalledWith('first'));
    c.enqueue(new TextEncoder().encode(frame({ choices: [{ index: 0, delta: { content: ' second' }, finish_reason: 'stop' }] }) + frame('[DONE]')));
    expect((await pending).content).toMatchObject([{ type: 'text', text: 'first second' }]);
  });
  it('preserves refusals and maps their stop reason', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(sse([
      { choices: [{ index: 0, delta: { refusal: 'No.' }, finish_reason: 'stop' }] }, '[DONE]',
    ])));
    const r = request();
    const turn = await provider().turn(r);
    expect(turn.stop).toBe('refusal');
    expect(turn.content).toMatchObject([{ type: 'text', text: 'No.' }]);
    expect(r.onText).toHaveBeenCalledExactlyOnceWith('No.');
  });
  it('accepts a JSON response from servers that ignore stream', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ choices: [{ finish_reason: 'stop', message: { content: 'fallback' } }] })));
    const r = request();
    expect((await provider().turn(r)).stop).toBe('end_turn');
    expect(r.onText).toHaveBeenCalledExactlyOnceWith('fallback');
  });
});

describe('OpenAI Responses transport', () => {
  it('removes output-only provenance on replay but preserves opaque reasoning and phase', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ status: 'completed', output: [{ ...reasoning, created_by: 'server-agent' }, { ...message('Hi'), created_by: 'server-agent' }, { ...call, created_by: 'server-agent' }] })));
    const t = await openaiResponses('', settings).turn(request());
    expect(toResponses([{ role: 'assistant', content: t.content }])).toEqual([reasoning, message('Hi'), call]);
  });
  it.each(['max_output_tokens', 'content_filter'])('maps incomplete %s safely', async (reason) => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(sse([{ type: 'response.incomplete', response: { status: 'incomplete', incomplete_details: { reason }, output: [call] } }])));
    expect((await openaiResponses('', settings).turn(request())).stop).toBe(reason === 'max_output_tokens' ? 'max_tokens' : 'refusal');
  });
  it('surfaces response.failed and never returns a tool call', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(sse([{ type: 'response.failed', response: { status: 'failed', output: [call], error: { message: 'failed response' } } }])));
    await expect(openaiResponses('', settings).turn(request())).rejects.toThrow('failed response');
  });
  it('does not invent or display encrypted reasoning when no summary is returned', async () => {
    const opaque = { ...reasoning, summary: [] };
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(sse([{ type: 'response.completed', response: { status: 'completed', output: [opaque] } }])));
    const r = request();
    const t = await openaiResponses('', settings).turn(r);
    expect(t.content).toEqual([]);
    expect(r.onThinking).not.toHaveBeenCalled();
    expect(r.onText).not.toHaveBeenCalled();
    expect(toResponses([{ role: 'assistant', content: t.content }])).toEqual([opaque]);
  });
  it('emits deltas before completion, then replays exact ordered reasoning/tool items across turns without storage', async () => {
    let controller!: ReadableStreamDefaultController<Uint8Array>;
    const fetcher = vi.fn().mockResolvedValueOnce(new Response(new ReadableStream<Uint8Array>({ start(c) { controller = c; } }), { headers: { 'Content-Type': 'text/event-stream' } }));
    vi.stubGlobal('fetch', fetcher);
    const r = request();
    const pending = openaiResponses('test-key', settings).turn(r);
    const push = (event: unknown) => controller.enqueue(new TextEncoder().encode(frame(event)));
    push({ type: 'response.output_text.delta', delta: 'Checking' });
    await vi.waitFor(() => expect(r.onText).toHaveBeenCalledWith('Checking'));
    push({ type: 'response.reasoning_summary_text.delta', delta: 'Checking.' });
    const output = [reasoning, message('Checking'), call];
    push({ type: 'response.completed', response: { status: 'completed', output, usage: { input_tokens: 20, output_tokens: 8, input_tokens_details: { cached_tokens: 5 } } } });
    const turn = await pending;
    expect(r.onText).toHaveBeenCalledTimes(1);
    expect(r.onThinking).toHaveBeenCalledWith('Checking.');
    expect(turn.stop).toBe('tool_use');
    expect(turn.usage).toEqual({ input: 15, output: 8, cacheRead: 5, cacheWrite: 0 });
    const messages: Anthropic.MessageParam[] = [...r.messages, { role: 'assistant', content: turn.content }, { role: 'user', content: [{ type: 'tool_result', tool_use_id: 'call_1', content: 'result' }] }];
    const output2 = [{ ...reasoning, id: 'rs_2', encrypted_content: 'opaque-2' }, message('Done')];
    fetcher.mockResolvedValueOnce(sse([{ type: 'response.output_text.delta', delta: 'Done' }, { type: 'response.completed', response: { status: 'completed', output: output2 } }]));
    const second = await openaiResponses('test-key', settings).turn(request({ messages }));
    messages.push({ role: 'assistant', content: second.content }, { role: 'user', content: 'next question' });
    expect(toResponses(messages)).toEqual([{ role: 'user', content: 'hello' }, ...output, { type: 'function_call_output', call_id: 'call_1', output: 'result' }, ...output2, { role: 'user', content: 'next question' }]);
    const body = JSON.parse(fetcher.mock.calls[1][1].body);
    expect(body.input).toEqual(toResponses(messages).slice(0, 5));
    expect(body).toMatchObject({ stream: true, store: false, include: ['reasoning.encrypted_content'] });
    expect(body).not.toHaveProperty('previous_response_id');
    expect(body).not.toHaveProperty('conversation');
    expect(JSON.stringify(messages)).not.toContain('opaque-provider-ciphertext');
    expect(JSON.stringify(toChat('system', messages))).not.toContain('opaque-provider-ciphertext');
  });

  it('decodes fragmented UTF-8, does not duplicate final text, preserves function arguments verbatim', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(sse([
      { type: 'response.output_text.delta', delta: 'hé🙂' },
      { type: 'response.function_call_arguments.delta', output_index: 1, delta: '{ "x":' },
      { type: 'response.function_call_arguments.delta', output_index: 1, delta: ' 1 }' },
      { type: 'response.output_item.done', output_index: 1, item: call },
      { type: 'response.completed', response: { status: 'completed', output: [message('hé🙂'), call] } },
    ])));
    const r = request();
    const turn = await openaiResponses('', settings).turn(r);
    expect(r.onText).toHaveBeenCalledExactlyOnceWith('hé🙂');
    expect(turn.content[1]).toMatchObject({ type: 'tool_use', id: 'call_1', input: { x: 1 } });
    expect(toResponses([{ role: 'assistant', content: turn.content }])).toEqual([message('hé🙂'), call]);
  });
});
