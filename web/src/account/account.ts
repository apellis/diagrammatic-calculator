// The account this browser is signed in to, if the site has accounts at all, and syncing the
// document list with it. Without an accounts API (a static host, the development server, no
// network) the state is 'unavailable' and the app is the anonymous app, unchanged.
import { createSignal } from 'solid-js';
import * as Y from 'yjs';
import { IndexeddbPersistence } from 'y-indexeddb';
import { Api, type CloudDoc, type User } from './api';
import { CLOUD, syncDoc, upload } from './sync';
import { createFromCloud, getInfo, listDocs, updateInfo, type CloudLink, type DocInfo } from '../doc/store';
import { Doc } from '../doc/model';

export type AccountState =
  | { status: 'loading' }
  | { status: 'unavailable' }
  | { status: 'anonymous' }
  | { status: 'signed-in'; user: User };

export const api = new Api();
const [account, setAccount] = createSignal<AccountState>({ status: 'loading' });
export { account };

/** The signed-in user, or null. */
export const user = (): User | null => {
  const a = account();
  return a.status === 'signed-in' ? a.user : null;
};

let loading: Promise<AccountState> | null = null;

/** Asks the server who is signed in (once per page load unless `again`). */
export function loadAccount(again = false): Promise<AccountState> {
  if (!loading || again) {
    loading = api.me().then((me) => {
      const s: AccountState = !me || !me.accounts ? { status: 'unavailable' } : me.user ? { status: 'signed-in', user: me.user } : { status: 'anonymous' };
      setAccount(s);
      return s;
    });
  }
  return loading;
}

const LINK_KEY = 'diagcalc-signin-link';

/**
 * Leaves for Google's sign-in, coming back to this page. A document link's token
 * (`#/s/<id>/<token>`) does not travel through the sign-in URL, where it would end up in request
 * logs: it waits in this tab's session storage and the return address is `#/s/<id>`.
 */
export function signIn(returnTo = location.pathname + location.hash) {
  const m = /^(.*#\/s\/[A-Za-z0-9_-]+)\/([A-Za-z0-9_-]+)$/.exec(returnTo);
  if (m) {
    try {
      sessionStorage.setItem(LINK_KEY, JSON.stringify({ path: m[1], token: m[2] }));
      returnTo = m[1];
    } catch {
      // no session storage: keep the token in the address rather than lose it
    }
  }
  location.assign(`/api/auth/login?return=${encodeURIComponent(returnTo)}`);
}

/** The link token put aside by signIn for the document `id`, taken (once). */
export function takeLinkToken(id: string): string | undefined {
  try {
    const raw = sessionStorage.getItem(LINK_KEY);
    if (!raw) return undefined;
    const v = JSON.parse(raw) as { path?: unknown; token?: unknown };
    if (typeof v.path !== 'string' || typeof v.token !== 'string' || !v.path.endsWith(`#/s/${id}`)) return undefined;
    sessionStorage.removeItem(LINK_KEY);
    return v.token;
  } catch {
    return undefined;
  }
}

/** Signs out. Documents stay in this browser. */
export async function signOut() {
  await api.logout();
  setAccount({ status: 'anonymous' });
}

/** Signs out in every browser where this account is signed in. */
export async function signOutEverywhere() {
  await api.logoutEverywhere();
  setAccount({ status: 'anonymous' });
}

/** What the sign-in redirect reported, read once from the address and removed from it. */
export function takeSignInOutcome(): string | null {
  const p = new URLSearchParams(location.search);
  const outcome = p.get('signin');
  if (!outcome) return null;
  p.delete('signin');
  const q = p.toString();
  history.replaceState(null, '', location.pathname + (q ? `?${q}` : '') + location.hash);
  return outcome;
}

/** Whether a local document belongs to this account's sync (or to nobody's yet). */
export const ours = (d: DocInfo, u: User) => !d.cloud || d.cloud.account === u.id;

const uploads = new Map<string, Promise<CloudLink>>();

/**
 * Saves a document that is only in this browser to the account, once: the document list and the
 * open document may both try at the same moment, and must not make two copies.
 */
export function uploadOnce(localId: string, ydoc: Y.Doc, name: string, u: User): Promise<CloudLink> {
  let p = uploads.get(localId);
  if (!p) {
    p = (async () => {
      const cur = await getInfo(localId);
      if (cur?.cloud) return cur.cloud;
      const link = await upload(api, ydoc, name, u);
      await updateInfo(localId, { cloud: link });
      return link;
    })().finally(() => uploads.delete(localId));
    uploads.set(localId, p);
  }
  return p;
}

const dbName = (id: string) => `diagcalc-doc-${id}`;

/** Runs `fn` on a stored document's Yjs document, saving what it changes. */
async function withYdoc<T>(id: string, fn: (ydoc: Y.Doc) => Promise<T>): Promise<T> {
  const ydoc = new Y.Doc({ gc: false });
  const p = new IndexeddbPersistence(dbName(id), ydoc);
  await p.whenSynced;
  try {
    return await fn(ydoc);
  } finally {
    await new Promise((r) => setTimeout(r, 0));
    await p.destroy();
  }
}

export interface SyncReport {
  uploaded: number;
  downloaded: number;
  updated: number;
  failed: number;
}

/**
 * Syncs the document list with the account: documents only in this browser are saved to it (on
 * the first sign-in, that is every document made anonymously), documents in the account but not
 * here are copied here, and copies that changed on either side are brought in step.
 */
export async function syncAll(u: User, onChange?: () => void): Promise<SyncReport> {
  const report: SyncReport = { uploaded: 0, downloaded: 0, updated: 0, failed: 0 };
  const remote = await api.list();
  const byId = new Map<string, CloudDoc>([...remote.owned, ...remote.shared].map((d) => [d.id, d]));
  const local = await listDocs();
  for (const d of local) {
    if (!ours(d, u)) continue;
    try {
      if (!d.cloud) {
        await withYdoc(d.id, (y) => uploadOnce(d.id, y, d.name, u));
        report.uploaded++;
      } else {
        const meta = byId.get(d.cloud.id);
        // gone from the account, or no longer shared: leave the local copy alone
        if (!meta) continue;
        const changedHere = d.edited > (d.cloud.synced ?? 0);
        if (meta.version === d.cloud.version && !changedHere) continue;
        const { link, name, edited } = await withYdoc(d.id, async (y) => {
          const link = await syncDoc(api, y, d.cloud!, d.name, meta);
          const doc = new Doc(y);
          return { link, name: doc.meta.has('name') ? doc.name : d.name, edited: doc.edited ?? d.edited };
        });
        await updateInfo(d.id, { cloud: link, name, edited: Math.max(d.edited, edited) });
        report.updated++;
      }
    } catch {
      report.failed++;
    }
    onChange?.();
  }
  const have = new Set(local.map((d) => d.cloud?.id).filter(Boolean));
  for (const meta of remote.owned) {
    if (have.has(meta.id)) continue;
    try {
      await openFromAccount(u, meta);
      report.downloaded++;
    } catch {
      report.failed++;
    }
    onChange?.();
  }
  return report;
}

/** Makes a local copy of a document from the account (or from a link), returning its local ID. */
export async function openFromAccount(u: User | null, meta: Pick<CloudDoc, 'id' | 'name' | 'owner'>, token?: string): Promise<string> {
  const s = await api.snapshot(meta.id, token);
  const link: CloudLink = {
    id: meta.id,
    account: u?.id ?? null,
    role: s.role,
    version: s.version,
    sv: s.data.length ? Y.encodeStateVectorFromUpdate(s.data) : new Uint8Array([0]),
    ...(token ? { token } : {}),
    ...(meta.owner ? { owner: meta.owner } : {}),
  };
  return createFromCloud(s.data, link, meta.name, CLOUD);
}
