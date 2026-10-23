// Browser transport fixture, NOT a security or Durable Object persistence proof.
// It uses the real Yjs wire protocol; production authorization/storage is tested in web/live.
import type { WebSocketRoute } from '@playwright/test';
import * as Y from 'yjs';
import * as sync from 'y-protocols/sync';
import * as awareness from 'y-protocols/awareness';
import * as encoding from 'lib0/encoding';
import * as decoding from 'lib0/decoding';
import type { User } from './mockapi';

type Peer = { socket: WebSocketRoute; user: User | null; role: string; ids: Set<number> };
const packet = (type: number, write: (e: encoding.Encoder) => void) => {
  const e = encoding.createEncoder();
  encoding.writeVarUint(e, type);
  write(e);
  return Buffer.from(encoding.toUint8Array(e));
};

export class MockRoom {
  readonly doc = new Y.Doc();
  readonly presence = new awareness.Awareness(this.doc);
  readonly peers = new Set<Peer>();
  acceptedUpdates = 0;
  constructor(data: Uint8Array, private readonly persist: (data: Buffer) => void) {
    if (data.length) Y.applyUpdate(this.doc, data);
    this.presence.setLocalState(null);
    // The fixture does not need awareness's background expiry timer.
    clearInterval(this.presence._checkInterval);
    this.doc.on('update', (update: Uint8Array, origin: unknown) => {
      this.persist(Buffer.from(Y.encodeStateAsUpdate(this.doc)));
      const message = packet(0, (e) => sync.writeUpdate(e, update));
      for (const peer of this.peers) if (peer !== origin) peer.socket.send(message);
    });
    this.presence.on('update', ({ added, updated, removed }: { added: number[]; updated: number[]; removed: number[] }) => {
      const message = packet(1, (e) => encoding.writeVarUint8Array(e, awareness.encodeAwarenessUpdate(this.presence, [...added, ...updated, ...removed])));
      for (const peer of this.peers) peer.socket.send(message);
    });
  }
  connect(socket: WebSocketRoute, user: User | null, role: string) {
    const peer: Peer = { socket, user, role, ids: new Set() };
    this.peers.add(peer);
    socket.onClose(() => this.remove(peer));
    socket.onMessage((raw) => {
      try {
        const d = decoding.createDecoder(typeof raw === 'string' ? Buffer.from(raw) : raw);
        const type = decoding.readVarUint(d);
        if (type === 0) {
          const subtype = decoding.peekVarUint(d);
          if (role === 'viewer' && subtype !== sync.messageYjsSyncStep1) return this.close(peer, 4003);
          const e = encoding.createEncoder();
          encoding.writeVarUint(e, 0);
          sync.readSyncMessage(d, e, this.doc, peer);
          if (encoding.length(e) > 1) socket.send(Buffer.from(encoding.toUint8Array(e)));
          if (subtype !== sync.messageYjsSyncStep1) {
            this.acceptedUpdates++;
            // One ack per incoming update/step2, never an extra ack for a read-only step1.
            this.persist(Buffer.from(Y.encodeStateAsUpdate(this.doc)));
            socket.send(packet(2, (ack) => encoding.writeVarString(ack, JSON.stringify({ generation: 1, saved: true }))));
          }
        } else if (type === 1) {
          const input = decoding.createDecoder(decoding.readVarUint8Array(d));
          const output = encoding.createEncoder();
          const count = decoding.readVarUint(input);
          encoding.writeVarUint(output, count);
          for (let i = 0; i < count; i++) {
            const id = decoding.readVarUint(input);
            const clock = decoding.readVarUint(input);
            const state = JSON.parse(decoding.readVarString(input));
            peer.ids.add(id);
            if (state) state.user = { id: user?.id ?? 'link-viewer', name: user?.name ?? 'Viewer', colour: '#3366aa' };
            encoding.writeVarUint(output, id);
            encoding.writeVarUint(output, clock);
            encoding.writeVarString(output, JSON.stringify(state));
          }
          awareness.applyAwarenessUpdate(this.presence, encoding.toUint8Array(output), peer);
        }
      } catch {
        this.close(peer, 4008);
      }
    });
    socket.send(packet(0, (e) => sync.writeSyncStep1(e, this.doc)));
    if (this.presence.getStates().size) socket.send(packet(1, (e) => encoding.writeVarUint8Array(e, awareness.encodeAwarenessUpdate(this.presence, [...this.presence.getStates().keys()]))));
  }
  private remove(peer: Peer) {
    this.peers.delete(peer);
    awareness.removeAwarenessStates(this.presence, [...peer.ids], null);
  }
  private close(peer: Peer, code: number) {
    this.remove(peer);
    peer.socket.close({ code });
  }
  disconnect(user: User, code = 1012) {
    for (const peer of [...this.peers]) if (peer.user?.id === user.id) this.close(peer, code);
  }
  dispose() {
    for (const peer of [...this.peers]) this.close(peer, 1000);
    this.presence.destroy();
    this.doc.destroy();
  }
}
