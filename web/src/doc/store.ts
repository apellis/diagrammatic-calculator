// Documents in this browser: each is a Yjs document saved in IndexedDB with its whole history,
// and an index of them (name, created, last edited, last exported) for the document list.
import * as Y from 'yjs';
import { IndexeddbPersistence } from 'y-indexeddb';
import { Doc } from './model';

/** A local document's copy in an account (see src/account/sync.ts). */
export interface CloudLink {
  /** The document's ID on the server. */
  id: string;
  /** The account it was saved or opened with in this browser; null when opened by link without one. */
  account: string | null;
  role: 'owner' | 'editor' | 'viewer';
  /** The server's version this copy last matched. */
  version: number;
  /** The Yjs state vector of that version, to tell whether this copy has anything new. */
  sv?: Uint8Array;
  /** When this copy last matched the server. */
  synced?: number;
  /** The link token, for a document opened by its link. */
  token?: string;
  owner?: { email: string; name: string };
}

export interface DocInfo {
  id: string;
  name: string;
  created: number;
  edited: number;
  /** When the document was last exported to a file, if ever. */
  exported?: number;
  /** Set once the document is saved to an account or was opened from one. */
  cloud?: CloudLink;
}

const INDEX_DB = 'diagcalc';
const INDEX = 'docs';

let indexDb: Promise<IDBDatabase> | null = null;
function index(): Promise<IDBDatabase> {
  indexDb ??= new Promise((resolve, reject) => {
    const open = indexedDB.open(INDEX_DB, 1);
    open.onupgradeneeded = () => open.result.createObjectStore(INDEX, { keyPath: 'id' });
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
  return indexDb;
}

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

export async function listDocs(): Promise<DocInfo[]> {
  const db = await index();
  const all = (await req(db.transaction(INDEX).objectStore(INDEX).getAll())) as DocInfo[];
  return all.sort((a, b) => b.edited - a.edited);
}

async function putInfo(info: DocInfo) {
  const db = await index();
  await req(db.transaction(INDEX, 'readwrite').objectStore(INDEX).put(info));
}

/** Changes some fields of a document's entry, leaving the others as they are stored now. */
export async function updateInfo(id: string, patch: Partial<Omit<DocInfo, 'id'>>): Promise<DocInfo | undefined> {
  const db = await index();
  const tx = db.transaction(INDEX, 'readwrite');
  const store = tx.objectStore(INDEX);
  const cur = (await req(store.get(id))) as DocInfo | undefined;
  if (!cur) return undefined;
  const next = { ...cur, ...patch };
  await req(store.put(next));
  return next;
}

/** The local copy of a document saved in an account, if this browser has one. */
export async function findByCloud(cloudId: string): Promise<DocInfo | undefined> {
  return (await listDocs()).find((d) => d.cloud?.id === cloudId);
}

export async function getInfo(id: string): Promise<DocInfo | undefined> {
  const db = await index();
  return (await req(db.transaction(INDEX).objectStore(INDEX).get(id))) as DocInfo | undefined;
}

const dbName = (id: string) => `diagcalc-doc-${id}`;
const newId = () => Math.random().toString(36).slice(2, 10) + Date.now().toString(36);

export interface OpenDoc {
  info: DocInfo;
  doc: Doc;
  close(): void;
}

/** Opens a stored document; changes are saved on every step. */
export async function openDoc(id: string): Promise<OpenDoc> {
  const info = await getInfo(id);
  if (!info) throw new Error('no such document');
  const ydoc = new Y.Doc({ gc: false });
  const persistence = new IndexeddbPersistence(dbName(id), ydoc);
  await persistence.whenSynced;
  const doc = new Doc(ydoc);
  const sync = () => {
    const edited = (doc.meta.get('edited') as number) ?? info.edited;
    if (doc.name !== info.name || edited !== info.edited) {
      info.name = doc.name;
      info.edited = edited;
      void updateInfo(id, { name: info.name, edited });
    }
  };
  doc.meta.observe(sync);
  return { info, doc, close: () => { doc.meta.unobserve(sync); void persistence.destroy(); } };
}

/** Creates a document, optionally filled by `fill`, and returns its ID. */
export async function createDoc(name: string, fill?: (d: Doc) => void): Promise<string> {
  const id = newId();
  const ydoc = new Y.Doc({ gc: false });
  const persistence = new IndexeddbPersistence(dbName(id), ydoc);
  await persistence.whenSynced;
  const doc = new Doc(ydoc);
  doc.meta.set('name', name);
  fill?.(doc);
  await new Promise((r) => setTimeout(r, 0)); // let the history record the first steps
  const now = Date.now();
  await putInfo({ id, name: doc.name, created: now, edited: now });
  await persistence.destroy();
  void askPersistence();
  return id;
}

/**
 * Creates a local copy of a document from an account: its content as a Yjs update, applied as
 * it is (the name comes from the content), and the link to the account.
 */
export async function createFromCloud(update: Uint8Array, cloud: CloudLink, fallbackName: string, origin: unknown): Promise<string> {
  const id = newId();
  const ydoc = new Y.Doc({ gc: false });
  const persistence = new IndexeddbPersistence(dbName(id), ydoc);
  await persistence.whenSynced;
  if (update.length) Y.applyUpdate(ydoc, update, origin);
  const doc = new Doc(ydoc);
  if (!doc.meta.has('name')) doc.meta.set('name', fallbackName);
  await new Promise((r) => setTimeout(r, 0));
  const now = Date.now();
  await putInfo({ id, name: doc.name, created: now, edited: (doc.meta.get('edited') as number) ?? now, cloud: { ...cloud, synced: now } });
  await persistence.destroy();
  void askPersistence();
  return id;
}

export async function duplicateDoc(id: string): Promise<string> {
  const src = await openDoc(id);
  const update = Y.encodeStateAsUpdate(src.doc.ydoc);
  src.close();
  return createDoc(`${src.info.name} (copy)`, (d) => {
    Y.applyUpdate(d.ydoc, update);
    d.meta.set('name', `${src.info.name} (copy)`);
  });
}

export async function deleteDoc(id: string) {
  const ydoc = new Y.Doc();
  const persistence = new IndexeddbPersistence(dbName(id), ydoc);
  await persistence.whenSynced;
  await persistence.clearData();
  const db = await index();
  await req(db.transaction(INDEX, 'readwrite').objectStore(INDEX).delete(id));
}

export async function markExported(id: string) {
  const info = await getInfo(id);
  if (info) await putInfo({ ...info, exported: Date.now() });
}

/** Asks the browser to keep this site's data when disk space runs low. */
export async function askPersistence(): Promise<boolean> {
  if (!navigator.storage?.persist) return false;
  if (await navigator.storage.persisted()) return true;
  return navigator.storage.persist();
}

export async function isPersisted(): Promise<boolean> {
  return (await navigator.storage?.persisted?.()) ?? false;
}
