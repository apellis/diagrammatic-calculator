import { describe, expect, it } from 'vitest';
import { Api } from './api';

describe('phase 7 API contract', () => {
  it('keeps link secrets in headers and generation fences on snapshot/migration writes', async () => {
    const calls: { path: string; init: RequestInit; headers: Headers }[] = [];
    const api = new Api('https://app.example/api', async (input, init = {}) => {
      const path = new URL(String(input)).pathname; calls.push({ path, init, headers: new Headers(init.headers) });
      if (path.endsWith('/live-ticket')) return Response.json({ ticket: 'single-use', role: 'editor', generation: 1 });
      if (init.method === 'GET') return new Response(new Uint8Array([0, 0]), { headers: { 'x-doc-generation': '2', 'x-doc-role': 'editor', 'x-doc-version': '9' } });
      return Response.json({ generation: 2, version: 9 });
    });
    api.csrf = 'csrf'; const ticket = await api.liveTicket('doc', 'secret');
    expect(ticket.generation).toBe(1); expect(calls[0].headers.get('x-link-token')).toBe('secret');
    expect(calls[0].headers.get('x-csrf-token')).toBe('csrf');
    expect(api.liveUrl('doc', ticket.ticket)).toBe('wss://app.example/api/docs/doc/live?ticket=single-use');
    const snapshot = await api.snapshot('doc', 'secret'); expect(snapshot.generation).toBe(2);
    await api.save('doc', snapshot.data, 9, 'Name', 'secret', 2);
    expect(calls[2].headers.get('x-doc-generation')).toBe('2');
    expect(calls[2].headers.get('if-match')).toBe('"9"');
    await api.migrate('doc', snapshot.data, 0, 'secret');
    expect(calls[3].headers.get('x-doc-generation')).toBe('0');
    expect(calls[3].init.method).toBe('POST');
  });
  it('addresses checkpoint versions and personal branches without leaking tokens into paths', async () => {
    const calls: { path: string; method: string; headers: Headers }[] = [];
    const api = new Api('/api', async (input, init = {}) => {
      const path = String(input); calls.push({ path, method: init.method!, headers: new Headers(init.headers) });
      if (path.endsWith('/versions')) return Response.json({ versions: [] });
      if (path.endsWith('/branches')) return Response.json({ branches: [] });
      return new Response(new Uint8Array([0, 0]));
    });
    expect(await api.versions('doc', 'secret')).toEqual({ versions: [] });
    expect(await api.version('doc', 'checkpoint/1', 'secret')).toEqual(new Uint8Array([0, 0]));
    expect(await api.branches('doc')).toEqual({ branches: [] });
    await api.branch('doc', 'branch/1'); await api.putBranch('doc', 'branch/1', new Uint8Array([0, 0]), 'What if'); await api.deleteBranch('doc', 'branch/1');
    expect(calls.map(c => [c.method, c.path])).toEqual([
      ['GET', '/api/docs/doc/versions'], ['GET', '/api/docs/doc/versions/checkpoint%2F1'], ['GET', '/api/docs/doc/branches'],
      ['GET', '/api/docs/doc/branches/branch%2F1'], ['PUT', '/api/docs/doc/branches/branch%2F1'], ['DELETE', '/api/docs/doc/branches/branch%2F1'],
    ]);
    expect(calls[4].headers.get('x-doc-name')).toBe('What%20if');
  });
});
