// Recovery never applies a different generation to the active CRDT. Personal data stays
// in the sidecar; the account branch endpoint is user-scoped, not a room broadcast.
import * as Y from 'yjs';
import { Doc, validBranch, type BranchRecord, type Inputs } from '../doc/model';
import { createDoc, openDoc, updateInfo, type OpenDoc } from '../doc/store';
import { ApiError, type Api } from './api';

const REMOTE_BRANCH = Symbol('personal branch download');
const freshId = () => crypto.randomUUID();
export function encodeBranch(b: BranchRecord): Uint8Array {
  if (!validBranch(b)) throw new Error('Invalid personal branch');
  return new TextEncoder().encode(JSON.stringify({ ...b, state: Array.from(b.state) }));
}
export function decodeBranch(bytes: Uint8Array): BranchRecord {
  const b = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  if (!b || !Array.isArray(b.state) || !b.state.every((n: unknown) => Number.isInteger(n) && Number(n) >= 0 && Number(n) <= 255)) throw new Error('Invalid branch bytes');
  b.state = new Uint8Array(b.state);
  if (!validBranch(b)) throw new Error('Invalid personal branch');
  return b;
}
const fingerprint = (b: BranchRecord) => new TextDecoder().decode(encodeBranch(b));

/** Immutable remote keys avoid a fetch/PUT race (the branch API has no compare-and-swap).
 * Only this browser's superseded uploads are pruned after verified replacement. Seen markers
 * persist privately, so locally discarded branches do not reappear on every mount. */
export class PersonalBranches {
  private stopped = false;
  private running: Promise<void> | null = null;
  private again = false;
  private readonly seen;
  private readonly sent;
  private readonly uploaded;
  constructor(private readonly doc: Doc, private readonly api: Api, private readonly cloudId: string, account: string,
    private readonly onError: (error: unknown) => void = () => {}) {
    this.seen = doc.privateDoc.getMap<string>(`branch-seen:${account}:${cloudId}`);
    this.sent = doc.privateDoc.getMap<string>(`branch-sent:${account}:${cloudId}`);
    this.uploaded = doc.privateDoc.getMap<string>(`branch-uploaded:${account}:${cloudId}`);
  }
  start() {
    this.doc.branches.observe(this.changed);
    globalThis.addEventListener?.('online', this.wake);
    globalThis.addEventListener?.('focus', this.wake);
    this.wake();
    return () => this.stop();
  }
  stop() {
    this.stopped = true;
    this.doc.branches.unobserve(this.changed);
    globalThis.removeEventListener?.('online', this.wake);
    globalThis.removeEventListener?.('focus', this.wake);
  }
  private wake = () => { void this.sync().catch(this.onError); };
  private async remove(key: string) {
    await this.api.deleteBranch(this.cloudId, key);
    try { await this.api.branch(this.cloudId, key); }
    catch (e) { if (e instanceof ApiError && e.status === 404) return; throw e; }
    throw new Error('Personal branch deletion verification failed');
  }
  private changed = (_event: Y.YMapEvent<BranchRecord>, tr: Y.Transaction) => {
    if (tr.origin !== REMOTE_BRANCH) this.wake();
  };
  sync(): Promise<void> {
    if (this.stopped) return Promise.resolve();
    if (this.running) { this.again = true; return this.running; }
    this.running = this.run().finally(() => {
      this.running = null;
      if (this.again && !this.stopped) { this.again = false; this.wake(); }
    });
    return this.running;
  }
  private async run() {
    const { branches } = await this.api.branches(this.cloudId);
    for (const item of branches) {
      if (this.stopped) return;
      if (this.seen.has(item.id)) continue;
      const incoming = decodeBranch(await this.api.branch(this.cloudId, item.id));
      if (this.stopped) return;
      const local = this.doc.branch(incoming.id);
      const same = local && fingerprint(local) === fingerprint(incoming);
      const b = local && !same ? { ...incoming, id: freshId(), name: `${incoming.name} (recovered fork)` } : incoming;
      this.doc.privateDoc.transact(() => {
        if (!same) this.doc.branches.set(b.id, b);
        this.seen.set(item.id, b.id);
        this.sent.set(b.id, fingerprint(b));
      }, REMOTE_BRANCH);
    }
    for (const b of this.doc.branchList()) {
      if (this.stopped) return;
      const bytes = encodeBranch(b), mark = fingerprint(b);
      if (this.sent.get(b.id) === mark) continue;
      const key = freshId();
      await this.api.putBranch(this.cloudId, key, bytes, b.name);
      // Read back the exact immutable target before recording it as uploaded.
      const stored = decodeBranch(await this.api.branch(this.cloudId, key));
      if (fingerprint(stored) !== mark) throw new Error('Personal branch verification failed');
      if (this.stopped) return;
      this.seen.set(key, b.id); this.sent.set(b.id, mark);
      const previous = this.uploaded.get(b.id);
      this.uploaded.set(b.id, key);
      // The replacement is verified before pruning, and keys are never reused. A failure
      // only leaves an extra recovery revision; it cannot delete another browser's write.
      if (previous) await this.remove(previous);
    }
    // A deliberate local discard removes only immutable records we have already seen.
    // Unseen remote experiments are fetched first and are never mistaken for deletions.
    for (const item of branches) {
      if (this.stopped) return;
      const localId = this.seen.get(item.id);
      if (localId && !this.doc.branches.has(localId)) await this.remove(item.id);
    }
  }
}

export interface RecoveryResult { id: string }
/** Stop DocSync and branch sync before calling. On any failure the source remains intact;
 * a prepared, detached entry is also left in the document list if publication is uncertain. */
export async function recoverCloud(o: OpenDoc, api: Api, mode: 'migrate' | 'reopen', account: string | null): Promise<RecoveryResult> {
  const link = o.info.cloud;
  if (!link) throw new Error('This document is not linked to an account');
  if (link.account !== null && link.account !== account) throw new Error('Sign in to the original account first');
  if (mode === 'migrate' && (!account || link.role !== 'owner')) throw new Error('Only the owner can upgrade this document');
  o.doc.checkpoint('Before cloud recovery'); await o.flush();
  const latest = await api.snapshot(link.id, link.token);
  if (mode === 'reopen' && (latest.generation ?? 0) === 0) throw new Error('The owner must upgrade the account document before it can be reopened');
  const y = new Y.Doc({ gc: false });
  if (latest.data.length) Y.applyUpdate(y, latest.data);
  const privateDoc = new Y.Doc(); Y.applyUpdate(privateDoc, Y.encodeStateAsUpdate(o.doc.privateDoc));
  privateDoc.getMap('cloud-recovery').set('sourceUpdate', Y.encodeStateAsUpdate(o.doc.ydoc));
  privateDoc.getMap('cloud-recovery').set('sourceId', o.info.id);
  let replacement = new Doc(y, privateDoc);
  if (mode === 'migrate') {
    if (latest.role !== 'owner' || (latest.generation ?? 0) !== 0) throw new Error('Ownership or generation changed; reopen the account document instead');
    Y.applyUpdate(y, Y.encodeStateAsUpdate(o.doc.ydoc));
    replacement = replacement.migrateLegacy();
  }
  // Persist the replacement and ALL private recovery before touching the server or source link.
  const id = await createDoc(`${o.doc.name} (recovered)`, d => {
    Y.applyUpdate(d.ydoc, Y.encodeStateAsUpdate(replacement.ydoc));
    Y.applyUpdate(d.privateDoc, Y.encodeStateAsUpdate(replacement.privateDoc));
  }, { localOnly: true });
  const prepared = await openDoc(id);
  try {
    await prepared.flush();
    let current = latest;
    if (mode === 'migrate') {
      const personal = new PersonalBranches(prepared.doc, api, link.id, account!);
      try { await personal.sync(); await prepared.flush(); } finally { personal.stop(); }
      const published = await api.migrate(link.id, Y.encodeStateAsUpdate(replacement.ydoc), 0, link.token);
      current = await api.snapshot(link.id, link.token);
      if (current.generation !== published.generation) throw new Error('Generation changed during upgrade; recovery copies are preserved');
      // Server may already include subsequent commits: only merge into the fresh generation.
      Y.applyUpdate(prepared.doc.ydoc, current.data);
      await prepared.flush();
    }
    const cloud = { ...link, account, role: current.role, version: current.version, generation: current.generation ?? 0, sv: Y.encodeStateVector(prepared.doc.ydoc), synced: Date.now(), deletions: undefined };
    await updateInfo(id, { cloud, localOnly: false });
    await updateInfo(o.info.id, { cloud: undefined, localOnly: true });
    o.info.cloud = undefined;
    o.info.localOnly = true;
    return { id };
  } finally { prepared.close(); }
}

/** A checkpoint preview is detached and read-only; the caller can create a personal branch. */
export async function cloudCheckpoint(api: Api, cloudId: string, version: string, token?: string): Promise<Inputs> {
  const y = new Y.Doc({ gc: false });
  try { Y.applyUpdate(y, await api.version(cloudId, version, token)); return new Doc(y).inputs(); }
  finally { y.destroy(); }
}
