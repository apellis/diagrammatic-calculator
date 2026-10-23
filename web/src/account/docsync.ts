// Keeps an open document in step with the account while it is shown: a sync when it opens, a
// save shortly after each change and when the page is hidden, and a look for others' changes
// every minute while the page is visible.
import { createSignal } from 'solid-js';
import * as Y from 'yjs';
import { ApiError, type User } from './api';
import { api, uploadOnce } from './account';
import { CLOUD, syncDoc, type CloudLink } from './sync';
import { LiveProvider, type LiveStatus, type Presence } from './live';
import { updateInfo, type OpenDoc } from '../doc/store';

export type SyncStatus = LiveStatus
  | 'local-only'
  | 'saving'
  | 'saved'
  | 'view-only'
  | 'offline'
  | 'too-large'
  | 'other-account'
  | 'gone'
  | 'error';

const SAVE_DELAY = 1500;
const POLL = 60_000;

export class DocSync {
  readonly status;
  private readonly setStatus;
  /** The document's link to the account, once it has one (reactive). */
  readonly link;
  private readonly setLink;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private poll: ReturnType<typeof setInterval> | undefined;
  private running: Promise<void> | null = null;
  private again = false;
  private stopped = false;
  private starting: Promise<void> | null = null;
  private stopping: Promise<void> | null = null;
  private metadata: Promise<unknown> | null = null;
  /** Changes made since the running sync began, which it may not include. */
  private dirty = false;
  live: LiveProvider | null = null;
  readonly presence;
  private readonly setPresence;
  private fallback = false;
  private legacy = false;

  constructor(
    private readonly o: OpenDoc,
    /** Null for someone not signed in, who can only read a document opened by its link. */
    private readonly u: User | null,
  ) {
    [this.presence, this.setPresence] = createSignal<Presence[]>([]);
    [this.status, this.setStatus] = createSignal<SyncStatus>(o.info.cloud?.role === 'viewer' || !u ? 'view-only' : 'saving');
    [this.link, this.setLink] = createSignal<CloudLink | undefined>(o.info.cloud);
  }

  /** Who this sync saves as (null: reading by link without an account). */
  get user(): User | null {
    return this.u;
  }

  get ydoc(): Y.Doc {
    return this.o.doc.ydoc;
  }

  /** The document's ID in the account, once it has one. */
  get cloudId(): string | undefined {
    return this.link()?.id;
  }

  get role() {
    return this.link()?.role;
  }

  /** Whether the signed-in person can change the document in the account. */
  get canEdit(): boolean {
    if (!this.link()) return true; // A pending first upload never locks local editing.
    return (this.link()?.generation ?? 1) !== 0 && !!this.u && !!this.role && this.role !== 'viewer' && !this.legacy && !['auth-required', 'forbidden', 'gone', 'generation-mismatch', 'migration-required'].includes(this.status());
  }

  start() {
    if (this.stopped || this.starting) return;
    if (this.o.info.localOnly) { this.setStatus('local-only'); return; }
    const c = this.o.info.cloud;
    if (!c && !this.u) return;
    if (c && c.account !== null && c.account !== this.u?.id) {
      this.setStatus('other-account');
      return;
    }
    this.ydoc.on('update', this.onUpdate);
    document.addEventListener('visibilitychange', this.onVisibility);
    this.starting = this.startLive();
  }

  updatePresence(fields: Record<string, unknown>) { this.live?.setPresence(fields); }

  private async startLive() {
    try {
      let link = this.link();
      if (!link) {
        link = await uploadOnce(this.o.info.id, this.ydoc, this.o.doc.name, this.u!);
        this.o.info.cloud = link;
        this.setLink(link);
      }
      if (this.stopped) return;
      this.live = new LiveProvider({
        api, ydoc: this.ydoc, docId: link.id, token: link.token, user: this.u,
        generation: link.generation ?? 0, role: link.role,
        onPresence: p => this.setPresence(p),
        onStatus: status => {
          if (this.stopped) return;
          this.setStatus(status);
          // Only transport absence/failure uses REST. Never turn an auth or generation
          // rejection into a second write path.
          if (status === 'unavailable' || status === 'migration-required') {
            this.legacy = status === 'migration-required';
            queueMicrotask(() => this.startFallback());
          } else if (status === 'view-only' && this.live?.role === 'viewer') {
            const current = this.link();
            if (current) {
              const viewer = { ...current, role: 'viewer' as const };
              this.o.info.cloud = viewer; this.setLink(viewer);
              this.metadata = updateInfo(this.o.info.id, { cloud: viewer });
              // Surface failure now and retain the rejected promise for stop's caller.
              void this.metadata.catch(() => this.setStatus('error'));
            }
          }
        },
      });
      this.live.start();
    } catch (e) {
      this.setStatus(e instanceof ApiError && e.status === 403 ? 'forbidden' : e instanceof TypeError ? 'offline' : 'error');
    }
  }

  private startFallback() {
    if (this.stopped || this.fallback) return;
    this.live?.stop(); this.live = null;
    this.fallback = true;
    this.setPresence([]);
    this.poll = setInterval(() => document.visibilityState === 'visible' && void this.sync(), POLL);
    void this.sync();
  }

  /** Stops transport; unacknowledged live edits remain in the local working copy. */
  stop(): Promise<void> {
    if (this.stopping) return this.stopping;
    this.stopped = true;
    this.live?.stop();
    this.live = null;
    clearTimeout(this.timer);
    clearInterval(this.poll);
    this.ydoc.off('update', this.onUpdate);
    document.removeEventListener('visibilitychange', this.onVisibility);
    this.stopping = (async () => {
      await this.starting;
      await this.metadata;
      // Drain the in-flight write before recovery can detach or close its source.
      // Changes made during that write need one final save, never a second stop.
      if (this.running) await this.running;
      if (this.fallback && this.dirty) await this.run();
    })();
    return this.stopping;
  }

  private onUpdate = (_u: Uint8Array, origin: unknown) => {
    if (!this.fallback || origin === CLOUD || !this.canEdit) return;
    clearTimeout(this.timer);
    this.dirty = true;
    this.setStatus('saving');
    this.timer = setTimeout(() => {
      this.timer = undefined;
      void this.sync();
    }, SAVE_DELAY);
  };

  /**
   * Saves the changes waiting for the pause after an edit now, so that what is shared next (a link
   * opened at once, a person added) includes them. Resolves when they are saved.
   */
  saveNow(): Promise<void> {
    if (this.live) return this.live.whenSaved();
    if (this.timer === undefined) return this.running ?? Promise.resolve();
    clearTimeout(this.timer);
    this.timer = undefined;
    return this.sync();
  }

  private onVisibility = () => {
    if (this.fallback && document.visibilityState === 'hidden') void this.sync();
  };

  /** Syncs now, or right after the sync already running. */
  sync(): Promise<void> {
    if (this.stopped || this.o.info.localOnly) return Promise.resolve();
    if (!this.fallback) return this.live?.whenSaved() ?? Promise.resolve();
    if (this.running) {
      this.again = true;
      return this.running;
    }
    this.running = this.run().finally(() => {
      this.running = null;
      if (this.again && !this.stopped) {
        this.again = false;
        void this.sync();
      }
    });
    return this.running;
  }

  private async run() {
    const info = this.o.info;
    if (info.localOnly) return;
    this.dirty = false;
    try {
      if (info.cloud?.role !== 'viewer') this.setStatus('saving');
      let link;
      if (!info.cloud) {
        // saved to the account first (once), then brought in step with this copy
        const first = await uploadOnce(info.id, this.ydoc, this.o.doc.name, this.u!);
        link = await syncDoc(api, this.ydoc, first, this.o.doc.name);
      }
      else if (!this.u) {
        // without an account, a link only reads
        const meta = await api.get(info.cloud.id, info.cloud.token);
        link = { ...(await syncDoc(api, this.ydoc, info.cloud, this.o.doc.name, { version: meta.version, role: 'viewer' })), role: info.cloud.role };
      } else link = await syncDoc(api, this.ydoc, info.cloud, this.o.doc.name);
      this.legacy = link.generation === 0;
      info.cloud = link;
      this.setLink(link);
      await updateInfo(info.id, { cloud: link });
      // a change made meanwhile has its own save coming
      this.setStatus(this.legacy ? (link.role === 'owner' ? 'migration-required' : 'view-only') : link.role === 'viewer' || !this.u ? 'view-only' : this.dirty ? 'saving' : 'saved');
    } catch (e) {
      if (e instanceof ApiError) this.setStatus(e.status === 409 && /generation|reopen/.test(e.code) ? 'generation-mismatch' : e.status === 403 ? 'forbidden' : e.status === 413 ? 'too-large' : e.status === 404 ? 'gone' : e.status === 401 ? 'offline' : 'error');
      else this.setStatus(navigator.onLine === false || e instanceof TypeError ? 'offline' : 'error');
    }
  }
}

/** What the top bar says for each state. */
export const statusText: Record<SyncStatus, string> = {
  'local-only': 'Recovery copy: saved in this browser only',
  connecting: 'Connecting…',
  live: 'Live: saved to your account',
  reconnecting: 'Reconnecting…',
  'auth-required': 'Sign in again; changes saved in this browser',
  forbidden: 'Access changed; changes saved in this browser',
  'rate-limited': 'Live limit reached; changes saved in this browser',
  'generation-mismatch': 'Reopen the account document; this browser copy is preserved',
  'migration-required': 'The owner must upgrade this document for live editing',
  unavailable: 'Live unavailable; using account sync',
  saving: 'Saving…',
  saved: 'Saved to your account',
  'view-only': 'View only',
  offline: 'Offline: saved in this browser',
  'too-large': 'Too large to save to your account; saved in this browser',
  'other-account': 'In another account: saved in this browser only',
  gone: 'No longer in your account; saved in this browser',
  error: 'Not saved to your account; saved in this browser',
};
