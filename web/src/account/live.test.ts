import { afterEach, describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';
import * as encoding from 'lib0/encoding';
import * as decoding from 'lib0/decoding';
import * as sync from 'y-protocols/sync';
import { LiveProvider, type LiveSocket } from './live';
import type { Api, Role } from './api';

class Socket implements LiveSocket {
  readyState = 0;
  binaryType = '';
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: ArrayBuffer }) => void) | null = null;
  onclose: ((event: { code: number }) => void) | null = null;
  onerror: (() => void) | null = null;
  sent: Uint8Array[] = [];
  send(data: Uint8Array) { this.sent.push(data); }
  close() { this.readyState = 3; }
  open() { this.readyState = 1; this.onopen?.(); }
  receive(e: encoding.Encoder) { this.onmessage?.({ data: encoding.toUint8Array(e).slice().buffer }); }
  drop(code = 1012) { this.readyState = 3; this.onclose?.({ code }); }
}
const frame = (write: (e: encoding.Encoder) => void) => { const e = encoding.createEncoder(); encoding.writeVarUint(e, 0); write(e); return e; };
const ack = (generation = 1) => { const e = encoding.createEncoder(); encoding.writeVarUint(e, 2); encoding.writeVarString(e, JSON.stringify({ generation, saved: true })); return e; };
const providers: LiveProvider[] = [];
afterEach(() => { providers.splice(0).forEach(p => p.stop()); vi.useRealTimers(); });
async function setup(role: Role = 'editor', generation = 1) {
  const sockets: Socket[] = [];
  const ticket = vi.fn(async () => ({ ticket: 'single-use', role, generation }));
  const ydoc = new Y.Doc();
  const p = new LiveProvider({ api: { liveTicket: ticket, liveUrl: () => 'wss://app.example/live' } as unknown as Api, ydoc, docId: 'doc', generation: 1, role, socketFactory: () => { const s = new Socket(); sockets.push(s); return s; }, random: () => 0 });
  providers.push(p); p.start(); await Promise.resolve(); await Promise.resolve();
  return { p, ydoc, sockets, ticket };
}
describe('live provider wire contract', () => {
  it('uses real sync frames and waits for durable acknowledgement, not socket send', async () => {
    const { p, ydoc, sockets: [s] } = await setup();
    s.open();
    const remote = new Y.Doc(); remote.getMap('meta').set('name', 'remote');
    s.receive(frame(e => sync.writeSyncStep2(e, remote)));
    expect(ydoc.getMap('meta').get('name')).toBe('remote');
    s.receive(frame(e => sync.writeSyncStep1(e, remote)));
    expect(p.status).toBe('saving'); s.receive(ack()); expect(p.status).toBe('live');
    ydoc.getMap('meta').set('local', 'kept');
    expect(p.status).toBe('saving');
    const d = decoding.createDecoder(s.sent.at(-1)!); expect(decoding.readVarUint(d)).toBe(0);
    sync.readSyncMessage(d, encoding.createEncoder(), remote, null);
    expect(remote.getMap('meta').get('local')).toBe('kept');
    s.receive(ack()); expect(p.status).toBe('live');
  });
  it('viewer never sends step2 or update, including dirty cached content', async () => {
    const { ydoc, sockets: [s] } = await setup('viewer');
    ydoc.getMap('meta').set('private', true); s.open();
    s.receive(frame(e => sync.writeSyncStep1(e, new Y.Doc())));
    ydoc.getMap('meta').set('private', false);
    const types = s.sent.map(bytes => { const d = decoding.createDecoder(bytes); return decoding.readVarUint(d) === 0 ? decoding.readVarUint(d) : -1; });
    expect(types.filter(t => t >= 0)).toEqual([0]);
  });
  it('reconnect obtains another ticket and resends offline state by sync step2', async () => {
    vi.useFakeTimers();
    const { ydoc, sockets, ticket } = await setup(); sockets[0].open(); sockets[0].drop();
    ydoc.getMap('meta').set('offline', true);
    await vi.advanceTimersByTimeAsync(1000); expect(ticket).toHaveBeenCalledTimes(2);
    sockets[1].open(); const remote = new Y.Doc(); sockets[1].receive(frame(e => sync.writeSyncStep1(e, remote)));
    const bytes = sockets[1].sent.find(b => { const d = decoding.createDecoder(b); return decoding.readVarUint(d) === 0 && decoding.readVarUint(d) === 1; })!;
    const d = decoding.createDecoder(bytes); decoding.readVarUint(d); sync.readSyncMessage(d, encoding.createEncoder(), remote, null);
    expect(remote.getMap('meta').get('offline')).toBe(true);
  });
  it.each([4001, 4003, 4008, 4009, 4010, 4011])('does not endlessly retry terminal close %s', async code => {
    vi.useFakeTimers(); const { sockets: [s], ticket } = await setup(); s.open(); s.drop(code);
    await vi.advanceTimersByTimeAsync(120000); expect(ticket).toHaveBeenCalledTimes(1);
  });
  it('does not let one durable ack cover later unacknowledged edits', async () => {
    const { p, ydoc, sockets: [s] } = await setup(); s.open();
    const remote = new Y.Doc(); s.receive(frame(e => sync.writeSyncStep2(e, remote)));
    s.receive(frame(e => sync.writeSyncStep1(e, remote))); s.receive(ack());
    ydoc.getMap('meta').set('first', 1); ydoc.getMap('meta').set('second', 2);
    s.receive(ack()); expect(p.status).toBe('saving'); s.receive(ack()); expect(p.status).toBe('live');
  });
  it.each([[4001, 'auth-required'], [4003, 'forbidden'], [4008, 'rate-limited'], [4009, 'too-large'], [4010, 'gone'], [4011, 'generation-mismatch']])('handles terminal notice %s before a close frame arrives', async (code, status) => {
    const { p, ydoc, sockets: [s] } = await setup(); s.open();
    ydoc.getMap('meta').set('unsaved', true);
    const e = encoding.createEncoder(); encoding.writeVarUint(e, 2);
    encoding.writeVarString(e, JSON.stringify({ saved: false, code })); s.receive(e);
    expect(p.status).toBe(status); expect(s.readyState).toBe(3);
    expect(ydoc.getMap('meta').get('unsaved')).toBe(true);
  });
  it('preserves local edits when a generation-changing ack fences the connection', async () => {
    const { p, ydoc, sockets: [s] } = await setup(); s.open(); ydoc.getMap('meta').set('private', 1);
    s.receive(ack(2)); expect(p.status).toBe('generation-mismatch'); expect(s.readyState).toBe(3);
    expect(ydoc.getMap('meta').get('private')).toBe(1);
  });
  it('uses refreshed role on reconnect and never resends an offline edit after downgrade', async () => {
    vi.useFakeTimers(); const { p, ydoc, sockets, ticket } = await setup(); sockets[0].open(); sockets[0].drop();
    ydoc.getMap('meta').set('offline', true);
    ticket.mockResolvedValue({ ticket: 'new', role: 'viewer', generation: 1 });
    await vi.advanceTimersByTimeAsync(1000); sockets[1].open();
    sockets[1].receive(frame(e => sync.writeSyncStep1(e, new Y.Doc())));
    expect(p.role).toBe('viewer');
    expect(sockets[1].sent.filter(b => { const d = decoding.createDecoder(b); return decoding.readVarUint(d) === 0; })).toHaveLength(1);
  });
  it('bounds unavailable-transport retries and preserves offline state', async () => {
    vi.useFakeTimers(); const { p, sockets, ticket, ydoc } = await setup();
    ydoc.getMap('meta').set('local', 1);
    for (let i = 0; i < 4; i++) { sockets[i].drop(1006); await vi.advanceTimersByTimeAsync(4000); }
    expect(ticket).toHaveBeenCalledTimes(4); expect(p.status).toBe('unavailable');
    expect(ydoc.getMap('meta').get('local')).toBe(1);
  });
  it('converges three real Yjs clients including offline text edits and a late join', async () => {
    vi.useFakeTimers();
    const server = new Y.Doc(); server.getText('text').insert(0, 'base');
    const clients = [await setup(), await setup()];
    const consumed = new Map<Socket, number>();
    const greeted = new Set<Socket>();
    const flush = () => {
      for (let round = 0; round < 10; round++) {
        for (const c of clients) {
          const s = c.sockets.at(-1)!; if (s.readyState !== 1) continue;
          for (let i = consumed.get(s) ?? 0; i < s.sent.length; i++) {
            const bytes = s.sent[i]; consumed.set(s, i + 1);
            const inspect = decoding.createDecoder(bytes); if (decoding.readVarUint(inspect) !== 0) continue;
            const subtype = decoding.readVarUint(inspect);
            const d = decoding.createDecoder(bytes); decoding.readVarUint(d);
            const reply = encoding.createEncoder(); encoding.writeVarUint(reply, 0);
            sync.readSyncMessage(d, reply, server, null);
            if (encoding.length(reply) > 1) s.receive(reply);
            if (!greeted.has(s)) { greeted.add(s); s.receive(frame(e => sync.writeSyncStep1(e, server))); }
            if (subtype !== 0) {
              s.receive(ack());
              for (const peer of clients) if (peer.sockets.at(-1)!.readyState === 1) peer.sockets.at(-1)!.receive(frame(e => sync.writeUpdate(e, Y.encodeStateAsUpdate(server))));
            }
          }
        }
      }
    };
    clients.forEach(c => c.sockets[0].open()); flush();
    clients[0].ydoc.getText('text').insert(0, 'A'); clients[1].ydoc.getText('text').insert(0, 'B'); flush();
    clients[0].sockets[0].drop(); clients[0].ydoc.getText('text').insert(0, 'offline');
    clients[1].ydoc.getText('text').insert(0, 'online'); flush();
    clients.push(await setup()); clients[2].sockets[0].open(); flush();
    await vi.advanceTimersByTimeAsync(1000); clients[0].sockets[1].open(); flush();
    const want = server.getText('text').toString();
    expect(want).toContain('offline'); expect(want).toContain('online');
    for (const c of clients) { expect(c.ydoc.getText('text').toString()).toBe(want); expect(c.p.status).toBe('live'); }
  });

  it('pauses canonical broadcasts so a successful decision retains local undo', async () => {
    const { p, ydoc, sockets: [s] } = await setup(); s.open();
    const canonical = new Y.Doc(); canonical.getText('text').insert(0, 'base');
    s.receive(frame(e => sync.writeSyncStep2(e, canonical)));
    const localOrigin = {}; const undo = new Y.UndoManager(ydoc.getText('text'), { trackedOrigins: new Set([localOrigin]) });
    const candidate = new Y.Doc(); Y.applyUpdate(candidate, Y.encodeStateAsUpdate(ydoc)); candidate.getText('text').insert(4, ' accepted');
    const update = Y.encodeStateAsUpdate(candidate);
    const release = p.pauseIncoming();
    s.receive(frame(e => sync.writeUpdate(e, update)));
    expect(ydoc.getText('text').toString()).toBe('base');
    Y.applyUpdate(ydoc, update, localOrigin); release(); release();
    expect(ydoc.getText('text').toString()).toBe('base accepted');
    undo.undo(); expect(ydoc.getText('text').toString()).toBe('base'); undo.destroy();
  });
  it('discards paused old-connection frames after disconnect or stop', async () => {
    const { p, ydoc, sockets: [s] } = await setup(); s.open();
    const release = p.pauseIncoming(); const remote = new Y.Doc(); remote.getMap('meta').set('stale', true);
    s.receive(frame(e => sync.writeSyncStep2(e, remote))); s.drop(); release();
    expect(ydoc.getMap('meta').has('stale')).toBe(false);
    p.stop(); release(); expect(ydoc.getMap('meta').has('stale')).toBe(false);
  });
  it('bounds paused input at 4 MiB and reconnects without applying excess data', async () => {
    const { p, sockets: [s] } = await setup(); s.open(); const release = p.pauseIncoming();
    s.onmessage!({ data: new ArrayBuffer(4 * 1024 * 1024 + 1) });
    expect(s.readyState).toBe(3); expect(p.status).toBe('reconnecting'); release();
  });

  it('fences generation before opening or applying incompatible state', async () => {
    const { p, sockets, ydoc } = await setup('editor', 2);
    expect(sockets).toHaveLength(0); expect(p.status).toBe('generation-mismatch');
    expect(ydoc.getMap('meta').size).toBe(0);
  });
});
