// Keeping a local document and its copy in an account in step. The local copy (IndexedDB) stays
// the working copy and the offline cache; the account holds a Yjs update of the whole document.
// Syncing brings the server's content in (a CRDT merge, so nothing is lost on either side) and
// saves the merged state when this copy has anything the server lacks.
import * as Y from 'yjs';
import { ApiError, type Api, type CloudDoc, type User } from './api';
import type { CloudLink as StoredCloudLink } from '../doc/store';

export type CloudLink = StoredCloudLink & { generation?: number; deletions?: string };
const deletions = (update: Uint8Array) => JSON.stringify([...Y.decodeUpdate(update).ds.clients].sort(([a], [b]) => a - b));
function fence(link: CloudLink, generation: number | undefined) {
  if (generation !== undefined && generation !== (link.generation ?? 0)) throw new ApiError(409, 'generation mismatch');
}

/** The origin of content merged in from the account: not a step of this browser, so not undoable. */
export const CLOUD = { author: 'cloud' };

/** Whether `local` holds updates that `remote` (a state vector) does not. */
export function ahead(local: Uint8Array, remote: Uint8Array | undefined): boolean {
  const r = remote && remote.length ? Y.decodeStateVector(remote) : new Map<number, number>();
  for (const [client, clock] of Y.decodeStateVector(local)) if (clock > (r.get(client) ?? 0)) return true;
  return false;
}

/** Brings the server's content into `ydoc`, returning the link at the server's version. */
async function pull(api: Api, ydoc: Y.Doc, link: CloudLink): Promise<CloudLink> {
  const s = await api.snapshot(link.id, link.token);
  fence(link, s.generation);
  if (s.data.length) {
    try {
      Y.applyUpdate(ydoc, s.data, CLOUD);
    } catch {
      throw new Error('the copy in the account cannot be read');
    }
  }
  return { ...link, generation: s.generation ?? link.generation, deletions: s.data.length ? deletions(s.data) : '[]', version: s.version, role: s.role, sv: s.data.length ? Y.encodeStateVectorFromUpdate(s.data) : new Uint8Array([0]) };
}

/**
 * Syncs one document both ways and returns the new link. `known` is the server's metadata when
 * the caller already has it (from the document list), which saves a request.
 */
export async function syncDoc(api: Api, ydoc: Y.Doc, link: CloudLink, name: string, known?: Pick<CloudDoc, 'version' | 'role' | 'generation'>): Promise<CloudLink> {
  const meta = known ?? (await api.get(link.id, link.token));
  fence(link, meta.generation);
  // A viewer cache is not an editable baseline after promotion: reopen from the account.
  if (link.role === 'viewer' && meta.role !== 'viewer') throw new ApiError(409, 'viewer copy requires reopen');
  let l: CloudLink = { ...link, role: meta.role, generation: meta.generation ?? link.generation };
  if (meta.version !== l.version || !l.sv || l.deletions === undefined) l = await pull(api, ydoc, l);
  if (meta.role === 'viewer') l = { ...l, role: 'viewer' };
  if (l.role !== 'viewer' && l.generation !== 0) {
    for (let attempt = 0; l.role !== 'viewer' && (ahead(Y.encodeStateVector(ydoc), l.sv) || (l.deletions !== undefined && deletions(Y.encodeStateAsUpdate(ydoc)) !== l.deletions)); attempt++) {
      if (attempt === 5) throw new Error('the document keeps changing in the account; try again');
      const sv = Y.encodeStateVector(ydoc);
      const data = Y.encodeStateAsUpdate(ydoc);
      try {
        const r = await api.save(l.id, data, l.version, name, l.token, l.generation);
        fence(l, r.generation);
        l = { ...l, version: r.version, sv, deletions: deletions(data) };
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 409) || /generation|reopen|migration/i.test(e.code)) throw e;
        l = await pull(api, ydoc, l);
      }
    }
  }
  return { ...l, synced: Date.now() };
}

/** Saves a document that is only in this browser to the account, returning its new link. */
export async function upload(api: Api, ydoc: Y.Doc, name: string, user: User): Promise<CloudLink> {
  const d = await api.create(name);
  return syncDoc(api, ydoc, { id: d.id, account: user.id, role: 'owner', version: d.version, generation: d.generation, deletions: '[]', sv: new Uint8Array([0]) }, name, d);
}
