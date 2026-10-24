import { describe, expect, it } from 'vitest';
import { eventData } from './provider-stream';

async function collect(text: string) {
  const bytes = new TextEncoder().encode(text);
  const response = new Response(new ReadableStream<Uint8Array>({ start(c) {
    for (const byte of bytes) c.enqueue(new Uint8Array([byte]));
    c.close();
  } }));
  const out: string[] = [];
  for await (const data of eventData(response, new AbortController().signal)) out.push(data);
  return out;
}
describe('provider SSE framing', () => {
  it.each(['\n', '\r', '\r\n'])('handles %j delimiters, multiline data, comments, BOM, and fragmented UTF-8', async (nl) => {
    expect(await collect(`\uFEFF: comment${nl}event: ignored${nl}id: ignored${nl}data: hé🙂${nl}data: two${nl}${nl}data: next${nl}${nl}`)).toEqual(['hé🙂\ntwo', 'next']);
  });
  it('does not dispatch a partial event at EOF', async () => {
    expect(await collect('data: incomplete\n')).toEqual([]);
  });
  it('propagates read failures rather than silently completing', async () => {
    const response = new Response(new ReadableStream({ start(c) { c.error(new Error('connection reset')); } }));
    await expect(eventData(response, new AbortController().signal).next()).rejects.toThrow('connection reset');
  });
});
