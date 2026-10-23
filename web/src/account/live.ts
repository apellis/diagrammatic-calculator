// Ticket-authenticated Yjs transport. IndexedDB remains the working copy: disconnecting
// never replaces it. Only the room's durable ack, not WebSocket.send, marks a write saved.
import * as Y from 'yjs';
import * as encoding from 'lib0/encoding';
import * as decoding from 'lib0/decoding';
import * as syncProtocol from 'y-protocols/sync';
import { Awareness, applyAwarenessUpdate, encodeAwarenessUpdate, removeAwarenessStates } from 'y-protocols/awareness';
import { ApiError, type Api, type Role, type User } from './api';
import { CLOUD } from './sync';

export type LiveStatus = 'connecting' | 'live' | 'saving' | 'reconnecting' | 'offline' | 'view-only' | 'auth-required' | 'forbidden' | 'rate-limited' | 'too-large' | 'gone' | 'generation-mismatch' | 'migration-required' | 'unavailable' | 'error';
export interface Presence {
  clientId: number;
  user?: { id: string; name: string; colour: string };
  [field: string]: unknown;
}
export interface LiveSocket {
  readyState: number;
  binaryType: string;
  onopen: (() => void) | null;
  onmessage: ((event: { data: ArrayBuffer }) => void) | null;
  onclose: ((event: { code: number }) => void) | null;
  onerror: (() => void) | null;
  send(data: Uint8Array): void;
  close(): void;
}
export interface LiveOptions {
  api: Api;
  ydoc: Y.Doc;
  docId: string;
  token?: string;
  user?: User | null;
  generation?: number;
  role?: Role;
  onStatus?: (status: LiveStatus) => void;
  onPresence?: (presence: Presence[]) => void;
  onGenerationMismatch?: (generation: number) => void;
  socketFactory?: (url: string) => LiveSocket;
  random?: () => number;
}
export class LiveProvider {
  status: LiveStatus = 'offline';
  readonly awareness: Awareness;
  role: Role = 'viewer';
  private socket?: LiveSocket;
  private timer?: ReturnType<typeof setTimeout>;
  private presenceTimer?: ReturnType<typeof setTimeout>;
  private connectingTimer?: ReturnType<typeof setTimeout>;
  private active = false;
  private epoch = 0;
  private attempts = 0;
  private pending = 0;
  private synced = false;
  private handshake = false;
  private hasReadAsViewer = false;
  private pauses = new Set<symbol>();
  private incoming: { bytes: ArrayBuffer; epoch: number }[] = [];
  private incomingBytes = 0;

  /** Hold canonical broadcasts while an atomic REST decision is applied with local undo origin. */
  pauseIncoming(): () => void {
    const token = Symbol('incoming-pause');
    this.pauses.add(token);
    return () => {
      if (!this.pauses.delete(token) || this.pauses.size) return;
      const queued = this.incoming;
      this.incoming = []; this.incomingBytes = 0;
      for (const frame of queued) this.deliver(frame.bytes, frame.epoch);
    };
  }
  private deliver(bytes: ArrayBuffer, epoch: number) {
    if (!this.active || epoch !== this.epoch) return;
    if (!this.pauses.size) { this.receive(bytes); return; }
    if (this.incomingBytes + bytes.byteLength > 4 * 1024 * 1024) {
      this.socket?.close(); this.retry(); return;
    }
    this.incoming.push({ bytes: bytes.slice(0), epoch });
    this.incomingBytes += bytes.byteLength;
  }
  private discardIncoming() { this.incoming = []; this.incomingBytes = 0; }
  private waiters: { resolve: () => void; reject: (e: Error) => void }[] = [];

  constructor(private readonly options: LiveOptions) {
    this.awareness = new Awareness(options.ydoc);
    this.awareness.setLocalState(null);
    this.awareness.on('update', this.onAwareness);
  }
  get presence(): Presence[] {
    return [...this.awareness.getStates()].filter(([id]) => id !== this.options.ydoc.clientID).map(([clientId, state]) => ({ ...state, clientId }));
  }
  start() {
    if (this.active) return;
    this.active = true;
    this.options.ydoc.on('update', this.onUpdate);
    void this.connect();
  }
  stop() {
    this.active = false; this.epoch++;
    this.discardIncoming(); this.pauses.clear();
    clearTimeout(this.timer); clearTimeout(this.connectingTimer); clearTimeout(this.presenceTimer);
    this.options.ydoc.off('update', this.onUpdate);
    this.socket?.close(); this.socket = undefined;
    this.awareness.off('update', this.onAwareness); this.awareness.destroy();
    this.rejectWaiters('Live sync stopped; unsaved changes remain in this browser');
  }
  setPresence(fields: Record<string, unknown>) {
    // Identity is exclusively supplied by the room, never by a local profile or caller.
    const { user: _user, clientId: _clientId, ...safe } = fields;
    this.awareness.setLocalState({ ...this.awareness.getLocalState(), ...safe });
  }
  whenSaved(): Promise<void> {
    if (this.status === 'live' || this.status === 'view-only') return Promise.resolve();
    if (!['connecting', 'saving', 'reconnecting'].includes(this.status)) return Promise.reject(new Error(`Live sync: ${this.status}`));
    return new Promise((resolve, reject) => this.waiters.push({ resolve, reject }));
  }
  private rejectWaiters(message: string) { this.waiters.splice(0).forEach(w => w.reject(new Error(message))); }
  private state(status: LiveStatus) {
    this.status = status; this.options.onStatus?.(status);
    if (status === 'live' || status === 'view-only') this.waiters.splice(0).forEach(w => w.resolve());
  }
  private healthy() {
    if (!this.synced || (!this.handshake && this.role !== 'viewer')) return;
    if (!this.pending) { this.attempts = 0; clearTimeout(this.connectingTimer); }
    this.state(this.role === 'viewer' ? 'view-only' : this.pending ? 'saving' : 'live');
  }
  private terminal(status: LiveStatus) {
    this.active = false; this.epoch++;
    this.discardIncoming(); this.pauses.clear();
    clearTimeout(this.timer); clearTimeout(this.connectingTimer); clearTimeout(this.presenceTimer);
    this.socket?.close(); this.socket = undefined;
    removeAwarenessStates(this.awareness, [...this.awareness.getStates().keys()].filter(id => id !== this.options.ydoc.clientID), this);
    this.state(status); this.rejectWaiters(`Live sync: ${status}`);
  }
  private mismatch(generation: number) {
    this.terminal('generation-mismatch'); this.options.onGenerationMismatch?.(generation);
  }
  private async connect() {
    if (!this.active) return;
    const epoch = ++this.epoch;
    this.state(this.attempts ? 'reconnecting' : 'connecting');
    if (!this.options.socketFactory && typeof WebSocket === 'undefined') { this.terminal('unavailable'); return; }
    try {
      const ticket = await this.options.api.liveTicket(this.options.docId, this.options.token);
      if (!this.active || epoch !== this.epoch) return;
      if (ticket.generation !== (this.options.generation ?? 0)) { this.mismatch(ticket.generation); return; }
      if (ticket.generation === 0) { this.terminal('migration-required'); return; }
      // A cached viewer copy may contain private edits; never upload it after promotion.
      if ((this.options.role === 'viewer' || this.hasReadAsViewer) && ticket.role !== 'viewer') { this.terminal('generation-mismatch'); return; }
      this.role = ticket.role;
      this.hasReadAsViewer ||= ticket.role === 'viewer';
      this.pending = 0; this.synced = false; this.handshake = false;
      const factory = this.options.socketFactory ?? ((url: string) => new WebSocket(url) as unknown as LiveSocket);
      const socket = this.socket = factory(this.options.api.liveUrl(this.options.docId, ticket.ticket));
      socket.binaryType = 'arraybuffer';
      const current = () => this.active && epoch === this.epoch;
      socket.onopen = () => {
        if (!current()) return;
        this.sendSync(e => syncProtocol.writeSyncStep1(e, this.options.ydoc));
        this.sendAwareness([this.options.ydoc.clientID]);
      };
      socket.onmessage = event => { if (current()) this.deliver(event.data, epoch); };
      socket.onclose = event => { if (current()) this.closed(event.code); };
      socket.onerror = () => { if (current()) { socket.close(); this.retry(); } };
      this.connectingTimer = setTimeout(() => { if (current() && (!this.synced || !this.handshake || this.pending > 0)) { socket.close(); this.retry(); } }, 10000);
    } catch (e) {
      if (!this.active || epoch !== this.epoch) return;
      if (e instanceof ApiError) {
        if (e.status === 401) { this.terminal('auth-required'); return; }
        if (e.status === 403) { this.terminal('forbidden'); return; }
        if (e.status === 409) { this.terminal('migration-required'); return; }
        if (e.status === 410) { this.terminal('gone'); return; }
        if ([404, 501, 503].includes(e.status)) { this.terminal('unavailable'); return; }
      }
      this.retry();
    }
  }
  private closed(code: number) {
    const terminal: Record<number, LiveStatus> = { 4001: 'auth-required', 4003: 'forbidden', 4008: 'rate-limited', 4009: 'too-large', 4010: 'gone', 4011: 'generation-mismatch' };
    if (terminal[code]) this.terminal(terminal[code]); else this.retry();
  }
  private retry() {
    if (!this.active) return;
    this.epoch++; this.discardIncoming(); clearTimeout(this.connectingTimer); clearTimeout(this.timer);
    this.socket = undefined;
    removeAwarenessStates(this.awareness, [...this.awareness.getStates().keys()].filter(id => id !== this.options.ydoc.clientID), this);
    this.attempts++;
    // After bounded failed handshakes REST is the unavailable-transport fallback.
    if (this.attempts >= 4) { this.terminal('unavailable'); return; }
    this.state(typeof navigator !== 'undefined' && navigator.onLine === false ? 'offline' : 'reconnecting');
    const delay = Math.min(30000, 500 * 2 ** (this.attempts - 1)) + (this.options.random ?? Math.random)() * 250;
    this.timer = setTimeout(() => void this.connect(), delay);
  }
  private send(e: encoding.Encoder) { if (this.socket?.readyState === 1) this.socket.send(encoding.toUint8Array(e)); }
  private sendSync(write: (e: encoding.Encoder) => void, durable = false) {
    if (this.socket?.readyState !== 1) return;
    const e = encoding.createEncoder(); encoding.writeVarUint(e, 0); write(e);
    if (durable) { this.pending++; this.state('saving'); }
    this.send(e);
  }
  private onUpdate = (update: Uint8Array, origin: unknown) => {
    if (origin === CLOUD || origin === this || !this.active || this.role === 'viewer') return;
    this.sendSync(e => syncProtocol.writeUpdate(e, update), true);
  };
  private sendAwareness(ids: number[]) {
    if (!this.active || this.socket?.readyState !== 1) return;
    const e = encoding.createEncoder(); encoding.writeVarUint(e, 1);
    encoding.writeVarUint8Array(e, encodeAwarenessUpdate(this.awareness, ids)); this.send(e);
  }
  private onAwareness = ({ added, updated, removed }: { added: number[]; updated: number[]; removed: number[] }, origin: unknown) => {
    if (origin !== this && this.active && [...added, ...updated, ...removed].includes(this.options.ydoc.clientID) && this.presenceTimer === undefined) {
      this.presenceTimer = setTimeout(() => {
        this.presenceTimer = undefined; this.sendAwareness([this.options.ydoc.clientID]);
      }, 110);
    }
    this.options.onPresence?.(this.presence);
  };
  private receive(bytes: ArrayBuffer) {
    try {
      const d = decoding.createDecoder(new Uint8Array(bytes));
      const type = decoding.readVarUint(d);
      if (type === 0) {
        const sub = decoding.readVarUint(d);
        if (sub === syncProtocol.messageYjsSyncStep1) {
          const sv = decoding.readVarUint8Array(d);
          if (this.role !== 'viewer') { this.handshake = true; this.sendSync(e => syncProtocol.writeSyncStep2(e, this.options.ydoc, sv), true); }
        } else if (sub === syncProtocol.messageYjsSyncStep2 || sub === syncProtocol.messageYjsUpdate) {
          Y.applyUpdate(this.options.ydoc, decoding.readVarUint8Array(d), CLOUD);
          if (sub === syncProtocol.messageYjsSyncStep2) { this.synced = true; }
          this.healthy();
        } else throw new Error('Unknown sync message');
      } else if (type === 1) applyAwarenessUpdate(this.awareness, decoding.readVarUint8Array(d), this);
      else if (type === 2) {
        const ack = JSON.parse(decoding.readVarString(d)) as { generation: number; saved?: boolean; code?: number };
        // Notify and close locally without waiting for the peer's close handshake. A terminal
        // notice is never a durable acknowledgement or an unspecified generation change.
        if (ack.saved === false && Number.isInteger(ack.code)) { this.closed(ack.code!); return; }
        if (!Number.isSafeInteger(ack.generation)) throw new Error('Invalid generation');
        if (ack.generation !== this.options.generation) { this.mismatch(ack.generation); return; }
        if (ack.saved === true && this.pending > 0) this.pending--;
        this.healthy();
      } else throw new Error('Unknown live message');
    } catch { this.terminal('error'); }
  }
}
