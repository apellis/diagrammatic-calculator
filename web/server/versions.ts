// Checkpoints are shared; branches belong to the signed-in person, never the share.
import type { Session } from './auth';
import { randomToken } from './crypto';
import { ACCOUNT_BYTES_SQL, LIMITS, sweep, type Access } from './docs';
import type { Deps, ReadyEnv } from './env';
import { binary, empty, HttpError, json, readBody } from './http';

const ID = /^[A-Za-z0-9_-]{1,100}$/;
function valid(id: string) { if (!ID.test(id)) throw new HttpError(400, 'invalid version or branch id'); }
function person(s: Session | null): string { if (!s) throw new HttpError(401, 'sign in first'); return s.user.id; }
async function object(env: ReadyEnv, key: string | undefined): Promise<Response> {
  if (!key) throw new HttpError(404, 'no such version or branch');
  const obj = await env.FILES.get(key);
  if (!obj) throw new HttpError(404, 'no such version or branch');
  return binary(await obj.arrayBuffer(), { 'content-type': 'application/octet-stream' });
}
export async function listVersions(env: ReadyEnv, a: Access): Promise<Response> {
  const r = await env.DB.prepare('SELECT id, time, author, note, size FROM doc_versions WHERE doc_id = ? ORDER BY time DESC, id DESC').bind(a.doc.id).all();
  return json({ versions: r.results });
}
export async function getVersion(env: ReadyEnv, a: Access, id: string): Promise<Response> {
  valid(id);
  const r = await env.DB.prepare('SELECT object_key FROM doc_versions WHERE doc_id = ? AND id = ?').bind(a.doc.id, id).first<{ object_key: string }>();
  return object(env, r?.object_key);
}
export async function listBranches(env: ReadyEnv, a: Access, s: Session | null): Promise<Response> {
  const r = await env.DB.prepare('SELECT id, name, updated, size FROM doc_branches WHERE doc_id = ? AND user_id = ? ORDER BY updated DESC, id DESC').bind(a.doc.id, person(s)).all();
  return json({ branches: r.results });
}
export async function getBranch(env: ReadyEnv, a: Access, s: Session | null, id: string): Promise<Response> {
  valid(id);
  const r = await env.DB.prepare('SELECT object_key FROM doc_branches WHERE doc_id = ? AND user_id = ? AND id = ?').bind(a.doc.id, person(s), id).first<{ object_key: string }>();
  return object(env, r?.object_key);
}
export async function putBranch(req: Request, env: ReadyEnv, a: Access, s: Session | null, id: string, deps: Deps): Promise<Response> {
  const user = person(s);
  valid(id);
  const body = await readBody(req, LIMITS.snapshotBytes);
  if (!body.byteLength) throw new HttpError(400, 'empty branch');
  let name: string;
  try { name = decodeURIComponent(req.headers.get('x-branch-name') ?? req.headers.get('x-doc-name') ?? id).replace(/[\u0000-\u001f\u007f]/g, ' ').trim().slice(0, LIMITS.nameLength) || 'Untitled'; }
  catch { throw new HttpError(400, 'bad X-Branch-Name'); }
  const old = await env.DB.prepare('SELECT object_key, size FROM doc_branches WHERE doc_id = ? AND user_id = ? AND id = ?').bind(a.doc.id, user, id).first<{ object_key: string; size: number }>();
  const key = `docs/${a.doc.id}/branches/${user}/${id}/${randomToken(16)}`;
  await env.FILES.put(key, body, { httpMetadata: { contentType: 'application/octet-stream' } });
  const updated = deps.now();
  try {
    // Quota and compare-and-swap share one SQLite statement: parallel personal saves cannot
    // overbook the account or delete the object another save just installed.
    const r = await env.DB.prepare(`INSERT INTO doc_branches (doc_id,user_id,id,name,updated,size,object_key)
      SELECT ?,?,?,?,?,?,? WHERE (${ACCOUNT_BYTES_SQL}) + ? - ? <= ?
        AND (SELECT COUNT(*) FROM doc_branches WHERE user_id = ?) < ?
        AND COALESCE((SELECT object_key FROM doc_branches WHERE doc_id = ? AND user_id = ? AND id = ?), '') = ?
      ON CONFLICT(doc_id,user_id,id) DO UPDATE SET name=excluded.name,updated=excluded.updated,size=excluded.size,object_key=excluded.object_key`)
      .bind(a.doc.id, user, id, name, updated, body.byteLength, key,
        user, '', user, user, body.byteLength, old?.size ?? 0, LIMITS.ownerBytes,
        user, old ? 1001 : 1000, a.doc.id, user, id, old?.object_key ?? '').run();
    if (r.meta.changes !== 1) throw new HttpError(409, 'branch changed or personal storage limit reached');
  } catch (e) { await env.FILES.delete(key); throw e; }
  if (old) {
    await env.DB.prepare('INSERT OR IGNORE INTO pending_deletes(prefix,created) VALUES (?,?)').bind(old.object_key, updated).run();
    await sweep(env, 2);
  }
  return json({ id, name, updated, size: body.byteLength }, 201);
}
export async function deleteBranch(env: ReadyEnv, a: Access, s: Session | null, id: string, deps: Deps): Promise<Response> {
  const user = person(s);
  valid(id);
  await env.DB.batch([
    env.DB.prepare('INSERT OR IGNORE INTO pending_deletes(prefix,created) SELECT object_key, ? FROM doc_branches WHERE doc_id = ? AND user_id = ? AND id = ?').bind(deps.now(), a.doc.id, user, id),
    env.DB.prepare('DELETE FROM doc_branches WHERE doc_id = ? AND user_id = ? AND id = ?').bind(a.doc.id, user, id),
  ]);
  await sweep(env, 2);
  return empty();
}

/** Room-only retention after a durable checkpoint: latest 50 plus latest/day for 30 days. */
export async function pruneVersions(env: ReadyEnv, docId: string, now: number): Promise<void> {
  const r = await env.DB.prepare('SELECT id, time, note FROM doc_versions WHERE doc_id = ? ORDER BY time DESC, id DESC').bind(docId).all<{ id: string; time: number; note: string }>();
  const days = new Set<number>();
  const stale: string[] = [];
  r.results.forEach((v, i) => {
    const day = Math.floor(v.time / 86_400_000);
    const daily = now - v.time <= 30 * 86_400_000 && !days.has(day);
    days.add(day);
    if (i >= 50 && !daily && v.note !== 'before migration') stale.push(v.id);
  });
  for (const id of stale) await env.DB.batch([
    env.DB.prepare('INSERT OR IGNORE INTO pending_deletes(prefix,created) SELECT object_key, ? FROM doc_versions WHERE doc_id = ? AND id = ?').bind(now, docId, id),
    env.DB.prepare('DELETE FROM doc_versions WHERE doc_id = ? AND id = ?').bind(docId, id),
  ]);
  await sweep(env, 2);
}
