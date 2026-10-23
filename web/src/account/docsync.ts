// Keeps an open document in step with the account while it is shown: a sync when it opens, a
// save shortly after each change and when the page is hidden, and a look for others' changes
// every minute while the page is visible.
import { createSignal } from 'solid-js';
import * as Y from 'yjs';
import { ApiError, type User } from './api';
import { api, uploadOnce } from './account';
import { CLOUD, syncDoc } from './sync';
import { updateInfo, type CloudLink, type OpenDoc } from '../doc/store';

export type SyncStatus =
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
  /** Changes made since the running sync began, which it may not include. */
  private dirty = false;

  constructor(
    private readonly o: OpenDoc,
    /** Null for someone not signed in, who can only read a document opened by its link. */
    private readonly u: User | null,
  ) {
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
    return !!this.u && !!this.role && this.role !== 'viewer';
  }

  start() {
    const c = this.o.info.cloud;
    if (!c && !this.u) return;
    if (c && c.account !== null && c.account !== this.u?.id) {
      this.setStatus('other-account');
      return;
    }
    this.ydoc.on('update', this.onUpdate);
    document.addEventListener('visibilitychange', this.onVisibility);
    this.poll = setInterval(() => document.visibilityState === 'visible' && void this.sync(), POLL);
    void this.sync();
  }

  /** Stops watching, saving what is not saved yet. */
  stop(): Promise<void> {
    this.stopped = true;
    clearTimeout(this.timer);
    clearInterval(this.poll);
    this.ydoc.off('update', this.onUpdate);
    document.removeEventListener('visibilitychange', this.onVisibility);
    return this.sync(true);
  }

  private onUpdate = (_u: Uint8Array, origin: unknown) => {
    if (origin === CLOUD || this.role === 'viewer' || !this.u) return;
    clearTimeout(this.timer);
    this.dirty = true;
    this.setStatus('saving');
    this.timer = setTimeout(() => void this.sync(), SAVE_DELAY);
  };

  private onVisibility = () => {
    if (document.visibilityState === 'hidden') void this.sync();
  };

  /** Syncs now, or right after the sync already running. */
  sync(final = false): Promise<void> {
    if (this.stopped && !final) return Promise.resolve();
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
      info.cloud = link;
      this.setLink(link);
      await updateInfo(info.id, { cloud: link });
      // a change made meanwhile has its own save coming
      this.setStatus(link.role === 'viewer' || !this.u ? 'view-only' : this.dirty ? 'saving' : 'saved');
    } catch (e) {
      if (e instanceof ApiError) this.setStatus(e.status === 413 ? 'too-large' : e.status === 404 ? 'gone' : e.status === 401 ? 'offline' : 'error');
      else this.setStatus(navigator.onLine === false || e instanceof TypeError ? 'offline' : 'error');
    }
  }
}

/** What the top bar says for each state. */
export const statusText: Record<SyncStatus, string> = {
  saving: 'Saving…',
  saved: 'Saved to your account',
  'view-only': 'View only: changes stay in this browser',
  offline: 'Offline: saved in this browser',
  'too-large': 'Too large to save to your account; saved in this browser',
  'other-account': 'In another account: saved in this browser only',
  gone: 'No longer in your account; saved in this browser',
  error: 'Not saved to your account; saved in this browser',
};
