// Test-only phase-6 byte-store substitute for LIVE. The old accounts suite exercises CAS,
// quotas and auth with arbitrary bytes, not Yjs. Real room semantics live in live/*.test.ts.
import type { Session } from './auth';
import { randomToken } from './crypto';
import { LIMITS, need, ownerBytes, cleanName, type Access } from './docs';
import type { Deps, ReadyEnv } from './env';
import { HttpError, json, readBody } from './http';
const overQuota = () => new HttpError(413, "the owner's storage is full");
const notFound = () => new HttpError(404, 'no such document');
/**
 * PUT /api/docs/:id/snapshot, with If-Match: "<version>" and optionally X-Doc-Name: saves new
 * content if nobody saved since that version, else 409 with the current version (the page then
 * merges the current content into its own, which a CRDT does without loss, and tries again).
 * The content goes to a fresh key first, so a save that loses the race never overwrites another.
 */
export async function putSnapshot(req: Request, env: ReadyEnv, a: Access, s: Session | null, deps: Deps): Promise<Response> {
  need(a, 'editor', s);
  const expected = Number((req.headers.get('if-match') ?? '').replace(/^W\//, '').replace(/"/g, ''));
  if (!req.headers.get('if-match') || !Number.isInteger(expected) || expected < 0) throw new HttpError(428, 'If-Match with the version is required');
  const body = await readBody(req, LIMITS.snapshotBytes).catch((e: unknown) => {
    throw e instanceof HttpError && e.status === 413 ? new HttpError(413, 'document too large to save') : e;
  });
  if (body.byteLength === 0) throw new HttpError(400, 'empty snapshot');
  if ((await ownerBytes(env, a.doc.owner_id, a.doc.id)) + a.doc.files_size + body.byteLength > LIMITS.ownerBytes) throw overQuota();
  let name: string | null = null;
  const rawName = req.headers.get('x-doc-name');
  if (rawName) {
    try {
      name = cleanName(decodeURIComponent(rawName));
    } catch {
      throw new HttpError(400, 'bad X-Doc-Name');
    }
  }
  const key = `docs/${a.doc.id}/snapshots/${randomToken(12)}`;
  await env.FILES.put(key, body, { httpMetadata: { contentType: 'application/octet-stream' } });
  // one transaction: the key replaced is the one of exactly the version this save succeeds on
  const [prev, r] = await env.DB.batch([
    env.DB.prepare('SELECT snapshot_key FROM documents WHERE id = ? AND version = ?').bind(a.doc.id, expected),
    env.DB.prepare(
      `UPDATE documents SET version = version + 1, edited = ?, size = ?, snapshot_key = ?, name = COALESCE(?, name)
       WHERE id = ? AND version = ?`,
    ).bind(deps.now(), body.byteLength, key, name, a.doc.id, expected),
  ]);
  if (r.meta.changes !== 1) {
    await env.FILES.delete(key);
    const cur = await env.DB.prepare('SELECT version FROM documents WHERE id = ?').bind(a.doc.id).first<{ version: number }>();
    if (!cur) throw notFound();
    throw new HttpError(409, 'the document changed; merge and try again', { version: cur.version });
  }
  const old = (prev.results[0] as { snapshot_key?: string | null } | undefined)?.snapshot_key;
  if (old) await env.FILES.delete(old);
  // now and then, remove snapshots left behind by saves interrupted between storing and recording
  if ((expected + 1) % 32 === 0) await sweepSnapshots(env, a.doc.id, key, deps.now());
  return json({ version: expected + 1 });
}

/** Deletes a document's stored snapshots other than `keep` that are older than ten minutes. */
async function sweepSnapshots(env: ReadyEnv, id: string, keep: string, now: number) {
  const page = await env.FILES.list({ prefix: `docs/${id}/snapshots/` });
  const stale = page.objects.filter((o) => o.key !== keep && now - o.uploaded.getTime() > 600_000).map((o) => o.key);
  if (stale.length) await env.FILES.delete(stale);
}
