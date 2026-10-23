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
  /** Server schema generation; old caches without it belong to generation zero. */
  generation?: number;
  /** Deletion-set fingerprint supplements the state vector for deletion-only commits. */
  deletions?: string;
  /** The Yjs state vector of that version, to tell whether this copy has anything new. */
  sv?: Uint8Array;
  /** When this copy last matched the server. */
  synced?: number;
  /** The link token, for a document opened by its link. */
  token?: string;
  owner?: { email?: string; name: string };
}

export interface DocInfo {
  id: string;
  /** Local storage generation; legacy database is left intact until migration succeeds. */
  localGeneration?: number;
  /** Recovery copies must never be implicitly published to an account. */
  localOnly?: boolean;
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

function committed(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onabort = tx.onerror = () => reject(tx.error ?? new Error('Local metadata transaction failed'));
  });
}

async function putInfo(info: DocInfo) {
  const db = await index();
  const tx = db.transaction(INDEX, 'readwrite');
  tx.objectStore(INDEX).put(info);
  await committed(tx);
}

/** Changes some fields of a document's entry, leaving the others as they are stored now. */
export async function updateInfo(id: string, patch: Partial<Omit<DocInfo, 'id'>>): Promise<DocInfo | undefined> {
  const db = await index();
  const tx = db.transaction(INDEX, 'readwrite');
  const store = tx.objectStore(INDEX);
  const cur = (await req(store.get(id))) as DocInfo | undefined;
  if (!cur) return undefined;
  const next = { ...cur, ...patch };
  store.put(next);
  await committed(tx);
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

const dbName = (id: string, generation = 0) => `diagcalc-doc-${id}${generation ? `-v${generation}` : ""}`;
const privateName = (id: string) => `diagcalc-private-${id}`;
const newId = () => Math.random().toString(36).slice(2, 10) + Date.now().toString(36);

export interface OpenDoc {
  info: DocInfo;
  doc: Doc;
  close(): void;
  /** Wait until local shared state and private recovery are persisted. */
  flush(): Promise<void>;
}

/** Opens a stored document; changes are saved on every step. */
export async function openDoc(id: string): Promise<OpenDoc> {
  const info = await getInfo(id);
  if (!info) throw new Error('no such document');
  const ydoc = new Y.Doc({ gc: false });
  let persistence = new IndexeddbPersistence(dbName(id, info.localGeneration), ydoc);
  await persistence.whenSynced;
  const privateDoc = new Y.Doc();
  let recovery = new IndexeddbPersistence(privateName(id), privateDoc);
  await recovery.whenSynced;
  let doc = new Doc(ydoc, privateDoc);
  // Cloud migration is coordinated by the owner/provider generation endpoint, never here.
  if (!info.cloud && doc.needsMigration) {
    const migrated = doc.migrateLegacy();
    await persistence.destroy();
    await recovery.destroy();
    info.localGeneration = (info.localGeneration ?? 0) + 1;
    persistence = new IndexeddbPersistence(dbName(id, info.localGeneration), migrated.ydoc);
    recovery = new IndexeddbPersistence(privateName(id), migrated.privateDoc);
    await Promise.all([persistence.whenSynced, recovery.whenSynced]);
    await putInfo(info);
    doc = migrated;
  }
  if (!doc.needsMigration) doc.ydoc.gc = true;
  const sync = () => {
    const edited = doc.edited ?? info.edited;
    if (doc.name !== info.name || edited !== info.edited) {
      info.name = doc.name;
      info.edited = edited;
      void updateInfo(id, { name: info.name, edited });
    }
  };
  doc.meta.observe(sync);
  return { info, doc, flush: async () => { await Promise.all([persistence.set('flushed', Date.now()), recovery.set('flushed', Date.now())]); }, close: () => { doc.meta.unobserve(sync); doc.undoManager.destroy(); void persistence.destroy(); void recovery.destroy(); } };
}

/** Creates a document, optionally filled by `fill`, and returns its ID. */
export async function createDoc(name: string, fill?: (d: Doc) => void, options: { localOnly?: boolean } = {}): Promise<string> {
  const id = newId();
  const ydoc = new Y.Doc({ gc: true });
  const persistence = new IndexeddbPersistence(dbName(id), ydoc);
  await persistence.whenSynced;
  const privateDoc = new Y.Doc();
  const recovery = new IndexeddbPersistence(privateName(id), privateDoc);
  await recovery.whenSynced;
  const doc = new Doc(ydoc, privateDoc);
  doc.meta.set('name', name);
  fill?.(doc);
  await new Promise((r) => setTimeout(r, 0)); // let the history record the first steps
  const now = Date.now();
  await putInfo({ id, name: doc.name, created: now, edited: now, ...options });
  await Promise.all([persistence.destroy(), recovery.destroy()]);
  void askPersistence();
  return id;
}

/**
 * Creates a local copy of a document from an account: its content as a Yjs update, applied as
 * it is (the name comes from the content), and the link to the account.
 */
export async function createFromCloud(update: Uint8Array, cloud: CloudLink, fallbackName: string, origin: unknown): Promise<string> {
  const id = newId();
  // Preserve legacy deleted structs until the owner has materialized old snapshots.
  const ydoc = new Y.Doc({ gc: false });
  const persistence = new IndexeddbPersistence(dbName(id), ydoc);
  await persistence.whenSynced;
  if (update.length) Y.applyUpdate(ydoc, update, origin);
  const privateDoc = new Y.Doc();
  const recovery = new IndexeddbPersistence(privateName(id), privateDoc);
  await recovery.whenSynced;
  const doc = new Doc(ydoc, privateDoc);
  if (!doc.meta.has('name')) doc.meta.set('name', fallbackName);
  await new Promise((r) => setTimeout(r, 0));
  const now = Date.now();
  await putInfo({ id, name: doc.name, created: now, edited: doc.edited ?? now, cloud: { ...cloud, synced: now } });
  await Promise.all([persistence.destroy(), recovery.destroy()]);
  void askPersistence();
  return id;
}

/** Persist a fresh generation after owner cloud migration succeeds. Close existing handles first. */
export async function replaceLocalDoc(id: string, doc: Doc, cloud?: CloudLink): Promise<void> {
  const info = await getInfo(id);
  if (!info) throw new Error('no such document');
  const generation = (info.localGeneration ?? 0) + 1;
  const persistence = new IndexeddbPersistence(dbName(id, generation), doc.ydoc);
  const recovery = new IndexeddbPersistence(privateName(id), doc.privateDoc);
  await Promise.all([persistence.whenSynced, recovery.whenSynced]);
  await Promise.all([persistence.set('flushed', Date.now()), recovery.set('flushed', Date.now())]);
  await putInfo({ ...info, localGeneration: generation, name: doc.name, ...(cloud ? { cloud } : {}) });
  await Promise.all([persistence.destroy(), recovery.destroy()]);
}

export async function duplicateDoc(id: string): Promise<string> {
  const src = await openDoc(id);
  const update = Y.encodeStateAsUpdate(src.doc.ydoc);
  const recovery = Y.encodeStateAsUpdate(src.doc.privateDoc);
  src.close();
  return createDoc(`${src.info.name} (copy)`, (d) => {
    Y.applyUpdate(d.ydoc, update);
    Y.applyUpdate(d.privateDoc, recovery);
    d.meta.set('name', `${src.info.name} (copy)`);
  });
}

/**
 * A new local document holding `src.cleanCopy()`: the current content only (no deleted text, no
 * history, nothing malformed). Returns its ID.
 */
export async function createCleanCopy(src: Doc, name: string): Promise<string> {
  const update = Y.encodeStateAsUpdate(src.cleanCopy(name).ydoc);
  return createDoc(name, (d) => Y.applyUpdate(d.ydoc, update));
}

export async function deleteDoc(id: string) {
  const info = await getInfo(id);
  // Replacement retains old generations for recovery; explicit deletion removes all of
  // them, not just the active generation and the original unsuffixed database.
  const names = [privateName(id)];
  for (let generation = 0; generation <= (info?.localGeneration ?? 0); generation++) names.push(dbName(id, generation));
  for (const name of names) await req(indexedDB.deleteDatabase(name));
  const db = await index();
  await req(db.transaction(INDEX, 'readwrite').objectStore(INDEX).delete(id));
  await saveLink(id, null);
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

// Files on disk linked to documents (Chrome, Edge): the file handle is kept in its own small
// database, so a linked document goes on saving to its file after a reload. The browser asks the
// person again for permission to write before the first save of a new visit.
const LINKS_DB = 'diagcalc-links';
let linksDb: Promise<IDBDatabase> | null = null;
function links(): Promise<IDBDatabase> {
  linksDb ??= new Promise((resolve, reject) => {
    const open = indexedDB.open(LINKS_DB, 1);
    open.onupgradeneeded = () => open.result.createObjectStore('links');
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
  return linksDb;
}

/** The file a document is linked to, if any (and if this browser can keep file handles). */
export async function loadLink(id: string): Promise<FileSystemFileHandle | undefined> {
  try {
    const db = await links();
    return (await req(db.transaction('links').objectStore('links').get(id))) as FileSystemFileHandle | undefined;
  } catch {
    return undefined;
  }
}

/** Links a document to a file, or unlinks it (null). */
export async function saveLink(id: string, h: FileSystemFileHandle | null): Promise<void> {
  try {
    const db = await links();
    const store = db.transaction('links', 'readwrite').objectStore('links');
    if (h) await req(store.put(h, id));
    else await req(store.delete(id));
  } catch {
    // the link lasts while the page is open
  }
}
