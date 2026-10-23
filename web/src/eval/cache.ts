// The result cache: results by the hash of their inputs, in memory and in IndexedDB, with a size
// cap and least-recently-used eviction. It is separate from documents and can be cleared without
// losing anything.

export interface CacheStore {
  get(key: string): Promise<unknown | undefined>;
  /** Optional for older/custom stores; ResultCache uses one transaction. */
  getMany?(keys: string[]): Promise<Map<string, unknown>>;
  put(key: string, value: unknown): Promise<void>;
  clear(): Promise<void>;
}

const DB = 'diagcalc-cache';
const STORE = 'results';

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

interface Row {
  key: string;
  value: unknown;
  size: number;
  used: number;
}

export class ResultCache implements CacheStore {
  private memory = new Map<string, unknown>();
  private db: Promise<IDBDatabase | null>;
  private total: Promise<number>;

  constructor(
    private capBytes = 500 * 1024 * 1024,
    private memoryEntries = 2000,
  ) {
    this.db = typeof indexedDB === 'undefined'
      ? Promise.resolve(null)
      : new Promise<IDBDatabase | null>((resolve) => {
          const open = indexedDB.open(DB, 1);
          open.onupgradeneeded = () => {
            const s = open.result.createObjectStore(STORE, { keyPath: 'key' });
            s.createIndex('used', 'used');
          };
          open.onsuccess = () => resolve(open.result);
          open.onerror = () => resolve(null); // private windows: memory only
        });
    this.total = this.db.then(async (db) => {
      if (!db) return 0;
      let total = 0;
      const rows = await req(db.transaction(STORE).objectStore(STORE).getAll()) as Row[];
      for (const r of rows) total += r.size;
      return total;
    });
  }

  /** A result already in memory, without waiting. */
  peek(key: string): unknown | undefined {
    return this.memory.get(key);
  }

  async get(key: string): Promise<unknown | undefined> {
    return (await this.getMany([key])).get(key);
  }

  async getMany(keys: string[]): Promise<Map<string, unknown>> {
    const hits = new Map<string, unknown>();
    const missing = [...new Set(keys)].filter((key) => {
      if (!this.memory.has(key)) return true;
      hits.set(key, this.memory.get(key));
      return false;
    });
    if (!missing.length) return hits;
    const db = await this.db;
    if (!db) return hits;
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE, 'readwrite');
      const store = tx.objectStore(STORE);
      tx.oncomplete = () => resolve();
      tx.onerror = tx.onabort = () => reject(tx.error);
      // Queue every request synchronously; touch LRU rows in the same transaction.
      for (const key of missing) {
        const request = store.get(key);
        request.onsuccess = () => {
          const row = request.result as Row | undefined;
          if (!row) return;
          hits.set(key, row.value);
          row.used = Date.now();
          store.put(row);
        };
      }
    });
    for (const [key, value] of hits) this.remember(key, value);
    return hits;
  }

  async put(key: string, value: unknown): Promise<void> {
    this.remember(key, value);
    const db = await this.db;
    if (!db) return;
    const size = JSON.stringify(value).length * 2;
    const store = db.transaction(STORE, 'readwrite').objectStore(STORE);
    await req(store.put({ key, value, size, used: Date.now() } satisfies Row));
    this.total = this.total.then((t) => t + size).then((t) => (t > this.capBytes ? this.evict(db, t) : t));
  }

  async clear(): Promise<void> {
    this.memory.clear();
    const db = await this.db;
    if (db) await req(db.transaction(STORE, 'readwrite').objectStore(STORE).clear());
    this.total = Promise.resolve(0);
  }

  private remember(key: string, value: unknown) {
    this.memory.delete(key);
    this.memory.set(key, value);
    if (this.memory.size > this.memoryEntries) this.memory.delete(this.memory.keys().next().value!);
  }

  /** Deletes the least recently used rows until the cache is under 90% of its cap. */
  private async evict(db: IDBDatabase, total: number): Promise<number> {
    const store = db.transaction(STORE, 'readwrite').objectStore(STORE);
    const cursor = store.index('used').openCursor();
    return new Promise((resolve) => {
      cursor.onsuccess = () => {
        const c = cursor.result;
        if (!c || total <= this.capBytes * 0.9) return resolve(total);
        total -= (c.value as Row).size;
        this.memory.delete((c.value as Row).key);
        c.delete();
        c.continue();
      };
      cursor.onerror = () => resolve(total);
    });
  }
}
