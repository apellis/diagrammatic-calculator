// Keeping a local document and its copy in an account in step. The local copy (IndexedDB) stays
// the working copy and the offline cache; the account holds a Yjs update of the whole document.
// Syncing brings the server's content in (a CRDT merge, so nothing is lost on either side) and
// saves the merged state when this copy has anything the server lacks.
import * as Y from 'yjs';
import { ApiError, type Api, type CloudDoc, type User } from './api';
import type { CloudLink } from '../doc/store';

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
  if (s.data.length) Y.applyUpdate(ydoc, s.data, CLOUD);
  return { ...link, version: s.version, role: s.role, sv: s.data.length ? Y.encodeStateVectorFromUpdate(s.data) : new Uint8Array([0]) };
}

/**
 * Syncs one document both ways and returns the new link. `known` is the server's metadata when
 * the caller already has it (from the document list), which saves a request.
 */
export async function syncDoc(api: Api, ydoc: Y.Doc, link: CloudLink, name: string, known?: Pick<CloudDoc, 'version' | 'role'>): Promise<CloudLink> {
  const meta = known ?? (await api.get(link.id, link.token));
  let l: CloudLink = { ...link, role: meta.role };
  if (meta.version !== l.version || !l.sv) l = await pull(api, ydoc, l);
  if (l.role !== 'viewer') {
    for (let attempt = 0; ahead(Y.encodeStateVector(ydoc), l.sv); attempt++) {
      if (attempt === 5) throw new Error('the document keeps changing in the account; try again');
      const sv = Y.encodeStateVector(ydoc);
      try {
        const r = await api.save(l.id, Y.encodeStateAsUpdate(ydoc), l.version, name, l.token);
        l = { ...l, version: r.version, sv };
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 409)) throw e;
        l = await pull(api, ydoc, l);
      }
    }
  }
  return { ...l, synced: Date.now() };
}

/** Saves a document that is only in this browser to the account, returning its new link. */
export async function upload(api: Api, ydoc: Y.Doc, name: string, user: User): Promise<CloudLink> {
  const d = await api.create(name);
  return syncDoc(api, ydoc, { id: d.id, account: user.id, role: 'owner', version: d.version, sv: new Uint8Array([0]) }, name, d);
}
