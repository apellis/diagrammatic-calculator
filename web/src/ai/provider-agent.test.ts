// Exercise the actual agent's conversation ownership, not just adapter translation.
import type Anthropic from '@anthropic-ai/sdk';
import { afterEach, expect, it, vi } from 'vitest';
import { run } from './agent';
import { DEFAULT_SETTINGS, openaiResponses } from './providers';
import { runTool, type ToolHost } from './tools';
vi.mock('./tools', () => ({ TOOLS: [], runTool: vi.fn(async () => ({ content: '{"ok":true}', summary: 'ok' })) }));
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); });
const reasoning = (id: string) => ({ type: 'reasoning', id, summary: [], encrypted_content: `opaque-${id}` });
const text = (value: string) => ({ type: 'message', id: `msg_${value}`, role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: value, annotations: [] }] });
const call = { type: 'function_call', id: 'fc_1', call_id: 'call_1', name: 'inspect', arguments: '{ "x": 1 }' };
function response(output: unknown[], incomplete = false) {
  const data = `data: ${JSON.stringify({ type: incomplete ? 'response.incomplete' : 'response.completed', response: { status: incomplete ? 'incomplete' : 'completed', output, ...(incomplete ? { incomplete_details: { reason: 'max_output_tokens' } } : {}) } })}\n\n`;
  return new Response(new ReadableStream({ start(c) { c.enqueue(new TextEncoder().encode(data)); c.close(); } }), { headers: { 'Content-Type': 'text/event-stream' } });
}
const provider = () => openaiResponses('test-key', { ...DEFAULT_SETTINGS, provider: 'openai', model: 'test-model' });
it('retains reasoning through real tool execution and a later user request with a new provider instance', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(response([reasoning('r1'), call])).mockResolvedValueOnce(response([reasoning('r2'), text('done')])).mockResolvedValueOnce(response([text('next')]));
  vi.stubGlobal('fetch', fetcher);
  const messages: Anthropic.MessageParam[] = [{ role: 'user', content: 'inspect' }];
  const args = { host: {} as ToolHost, messages, signal: new AbortController().signal, emit: vi.fn() };
  await run({ ...args, provider: provider() });
  expect(runTool).toHaveBeenCalledExactlyOnceWith(args.host, 'inspect', { x: 1 });
  const toolResult = { type: 'function_call_output', call_id: 'call_1', output: '{"ref":"op1","ok":true}' };
  expect(JSON.parse(fetcher.mock.calls[1][1].body).input).toEqual([{ role: 'user', content: 'inspect' }, reasoning('r1'), call, toolResult]);
  messages.push({ role: 'user', content: 'follow up' });
  await run({ ...args, provider: provider() });
  expect(JSON.parse(fetcher.mock.calls[2][1].body).input).toEqual([{ role: 'user', content: 'inspect' }, reasoning('r1'), call, toolResult, reasoning('r2'), text('done'), { role: 'user', content: 'follow up' }]);
  expect(args.emit).not.toHaveBeenCalledWith(expect.objectContaining({ type: 'stopped' }));
});
it('never executes a length-truncated streamed tool call', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response([reasoning('r1'), { ...call, arguments: '{' }], true)));
  const messages: Anthropic.MessageParam[] = [{ role: 'user', content: 'inspect' }];
  const emit = vi.fn();
  await run({ provider: provider(), host: {} as ToolHost, messages, signal: new AbortController().signal, emit });
  expect(runTool).not.toHaveBeenCalled();
  expect(messages).toHaveLength(1);
  expect(emit).toHaveBeenCalledWith({ type: 'stopped', reason: expect.stringContaining('nothing was run') });
});
