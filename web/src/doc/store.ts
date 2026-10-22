// Documents in this browser: each is a Yjs document saved in IndexedDB with its whole history,
// and an index of them (name, created, last edited, last exported) for the document list.
import * as Y from 'yjs';
import { IndexeddbPersistence } from 'y-indexeddb';
import { Doc } from './model';

export interface DocInfo {
  id: string;
  name: string;
  created: number;
  edited: number;
  /** When the document was last exported to a file, if ever. */
  exported?: number;
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
      void putInfo(info);
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
