// Documents saved to accounts: metadata in D1, content in R2, and who may do what.
import type { Deps, ReadyEnv } from './env';
import type { Session } from './auth';
import { equal, randomToken } from './crypto';
import { binary, empty, HttpError, json, readJson } from './http';
import { sendShareMail } from './mail';

export type Role = 'owner' | 'editor' | 'viewer';
const RANK: Record<Role, number> = { viewer: 1, editor: 2, owner: 3 };

/** Limits that keep one account from filling the storage or the e-mail allowance. */
export const LIMITS = {
  snapshotBytes: 16 * 1024 * 1024,
  fileBytes: 16 * 1024 * 1024,
  docsPerOwner: 1000,
  sharesPerDoc: 100,
  mailsPerDay: 50,
  nameLength: 200,
};

export const DOC_ID = /^[A-Za-z0-9_-]{16}$/;
const FILE_NAME = /^[\w .()-]{1,120}\.(dcw|dch)$/;
const EMAIL = /^[^\s@<>()",;:]+@[^\s@<>()",;:]+\.[^\s@<>()",;:]+$/;

interface DocRow {
  id: string;
  owner_id: string;
  name: string;
  created: number;
  edited: number;
  version: number;
  size: number;
  snapshot_key: string | null;
  link_role: 'viewer' | 'editor' | null;
  link_token: string | null;
  owner_email: string;
  owner_name: string;
}

export interface Access {
  doc: DocRow;
  role: Role;
  /** Whether the role comes only from the document's link. */
  viaLink: boolean;
}

const notFound = () => new HttpError(404, 'no such document');

/**
 * The caller's role on a document: owner, a share to their e-mail address, or the document's link
 * (whose token comes in X-Link-Token). Anyone without a role gets 404, so a document's existence
 * is not revealed.
 */
export async function access(env: ReadyEnv, id: string, s: Session | null, linkToken: string | null): Promise<Access> {
  if (!DOC_ID.test(id)) throw notFound();
  const doc = await env.DB.prepare(
    'SELECT d.*, u.email AS owner_email, u.name AS owner_name FROM documents d JOIN users u ON u.id = d.owner_id WHERE d.id = ?',
  )
    .bind(id)
    .first<DocRow>();
  if (!doc) throw notFound();
  let role: Role | null = null;
  let viaLink = false;
  if (s && doc.owner_id === s.user.id) role = 'owner';
  else if (s) {
    const share = await env.DB.prepare('SELECT role FROM shares WHERE doc_id = ? AND email = ?').bind(id, s.user.email).first<{ role: Role }>();
    role = share?.role ?? null;
  }
  if (linkToken && doc.link_token && doc.link_role && equal(linkToken, doc.link_token) && (!role || RANK[doc.link_role] > RANK[role])) {
    role = doc.link_role;
    viaLink = true;
  }
  if (!role) throw notFound();
  return { doc, role, viaLink };
}

function need(a: Access, role: Role, s: Session | null) {
  if (RANK[a.role] < RANK[role]) throw new HttpError(403, role === 'owner' ? 'only the owner can do this' : 'you can only view this document');
  // editing through a link needs an account, so every change has an author
  if (role !== 'viewer' && !s) throw new HttpError(401, 'sign in to edit');
}

function requireSession(s: Session | null): Session {
  if (!s) throw new HttpError(401, 'sign in first');
  return s;
}

function meta(a: Access) {
  const d = a.doc;
  return {
    id: d.id,
    name: d.name,
    created: d.created,
    edited: d.edited,
    version: d.version,
    size: d.size,
    role: a.role,
    owner: { email: d.owner_email, name: d.owner_name },
    ...(a.role === 'owner' ? { link: d.link_role ? { role: d.link_role, token: d.link_token } : null } : {}),
  };
}

function cleanName(v: unknown): string {
  if (typeof v !== 'string') throw new HttpError(400, 'name must be text');
  const n = v.replace(/[\u0000-\u001f\u007f]/g, ' ').trim().slice(0, LIMITS.nameLength);
  return n || 'Untitled';
}

/** Deletes every R2 object under a prefix. */
async function deletePrefix(env: ReadyEnv, prefix: string) {
  let cursor: string | undefined;
  do {
    const page = await env.FILES.list({ prefix, cursor });
    if (page.objects.length) await env.FILES.delete(page.objects.map((o) => o.key));
    cursor = page.truncated ? page.cursor : undefined;
  } while (cursor);
}

// --- routes ---------------------------------------------------------------------------------

/** GET /api/docs: the caller's documents and those shared with them. */
export async function listDocs(env: ReadyEnv, s: Session | null): Promise<Response> {
  const me = requireSession(s);
  const owned = await env.DB.prepare(
    'SELECT id, name, created, edited, version, size, link_role FROM documents WHERE owner_id = ? ORDER BY edited DESC',
  )
    .bind(me.user.id)
    .all();
  const shared = await env.DB.prepare(
    `SELECT d.id, d.name, d.created, d.edited, d.version, d.size, s.role, u.email AS owner_email, u.name AS owner_name
     FROM shares s JOIN documents d ON d.id = s.doc_id JOIN users u ON u.id = d.owner_id
     WHERE s.email = ? AND d.owner_id != ? ORDER BY d.edited DESC`,
  )
    .bind(me.user.email, me.user.id)
    .all<Record<string, unknown>>();
  return json({
    owned: owned.results.map((r) => ({ ...r, role: 'owner' })),
    shared: shared.results.map(({ owner_email, owner_name, ...r }) => ({ ...r, owner: { email: owner_email, name: owner_name } })),
  });
}

/** POST /api/docs {name}: a new, empty document owned by the caller. */
export async function createDoc(req: Request, env: ReadyEnv, s: Session | null, deps: Deps): Promise<Response> {
  const me = requireSession(s);
  const body = await readJson<{ name?: unknown }>(req);
  const name = cleanName(body.name ?? 'Untitled');
  const count = await env.DB.prepare('SELECT COUNT(*) AS n FROM documents WHERE owner_id = ?').bind(me.user.id).first<{ n: number }>();
  if ((count?.n ?? 0) >= LIMITS.docsPerOwner) throw new HttpError(409, 'too many documents');
  const id = randomToken(12);
  const now = deps.now();
  await env.DB.prepare('INSERT INTO documents (id, owner_id, name, created, edited) VALUES (?, ?, ?, ?, ?)').bind(id, me.user.id, name, now, now).run();
  return json(meta(await access(env, id, me, null)), 201);
}

export function getDoc(a: Access): Response {
  return json(meta(a));
}

/** PATCH /api/docs/:id {name?, link?}: rename (editors), or set link sharing (owner). */
export async function patchDoc(req: Request, env: ReadyEnv, a: Access, s: Session | null): Promise<Response> {
  const body = await readJson<{ name?: unknown; link?: unknown }>(req);
  if (body.name !== undefined) {
    need(a, 'editor', s);
    await env.DB.prepare('UPDATE documents SET name = ? WHERE id = ?').bind(cleanName(body.name), a.doc.id).run();
  }
  if (body.link !== undefined) {
    need(a, 'owner', s);
    if (body.link === null || body.link === 'off') {
      await env.DB.prepare('UPDATE documents SET link_role = NULL, link_token = NULL WHERE id = ?').bind(a.doc.id).run();
    } else if (body.link === 'viewer' || body.link === 'editor') {
      // changing the rights keeps the link; turning it on issues a new one
      await env.DB.prepare('UPDATE documents SET link_role = ?, link_token = COALESCE(link_token, ?) WHERE id = ?').bind(body.link, randomToken(), a.doc.id).run();
    } else throw new HttpError(400, 'link must be off, viewer or editor');
  }
  return json(meta(await access(env, a.doc.id, s, null)));
}

/** DELETE /api/docs/:id: the owner deletes the document, its shares and its stored files. */
export async function deleteDoc(env: ReadyEnv, a: Access, s: Session | null): Promise<Response> {
  need(a, 'owner', s);
  await env.DB.prepare('DELETE FROM documents WHERE id = ?').bind(a.doc.id).run();
  await deletePrefix(env, `docs/${a.doc.id}/`);
  return empty();
}

/** GET /api/docs/:id/snapshot: the document's content, a Yjs update. */
export async function getSnapshot(env: ReadyEnv, a: Access): Promise<Response> {
  const headers = { 'content-type': 'application/octet-stream', 'x-doc-version': String(a.doc.version), 'x-doc-role': a.role };
  if (!a.doc.snapshot_key) return binary(new ArrayBuffer(0), headers);
  const obj = await env.FILES.get(a.doc.snapshot_key);
  if (!obj) throw new HttpError(500, 'snapshot missing');
  return binary(await obj.arrayBuffer(), headers);
}

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
  const body = await req.arrayBuffer();
  if (body.byteLength === 0) throw new HttpError(400, 'empty snapshot');
  if (body.byteLength > LIMITS.snapshotBytes) throw new HttpError(413, 'document too large to save');
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
  const r = await env.DB.prepare(
    `UPDATE documents SET version = version + 1, edited = ?, size = ?, snapshot_key = ?, name = COALESCE(?, name)
     WHERE id = ? AND version = ?`,
  )
    .bind(deps.now(), body.byteLength, key, name, a.doc.id, expected)
    .run();
  if (r.meta.changes !== 1) {
    await env.FILES.delete(key);
    const cur = await env.DB.prepare('SELECT version FROM documents WHERE id = ?').bind(a.doc.id).first<{ version: number }>();
    if (!cur) throw notFound();
    throw new HttpError(409, 'the document changed; merge and try again', { version: cur.version });
  }
  if (a.doc.snapshot_key && a.doc.version === expected) await env.FILES.delete(a.doc.snapshot_key);
  return json({ version: expected + 1 });
}

/** GET /api/docs/:id/shares: who has access (owner and editors may look). */
export async function listShares(env: ReadyEnv, a: Access, s: Session | null): Promise<Response> {
  need(a, 'editor', s);
  const rows = await env.DB.prepare('SELECT email, role, created FROM shares WHERE doc_id = ? ORDER BY created').bind(a.doc.id).all();
  return json({ owner: { email: a.doc.owner_email, name: a.doc.owner_name }, shares: rows.results });
}

/** PUT /api/docs/:id/shares {email, role, notify}: the owner shares the document with someone. */
export async function putShare(req: Request, env: ReadyEnv, a: Access, s: Session | null, deps: Deps): Promise<Response> {
  need(a, 'owner', s);
  const me = s!;
  const body = await readJson<{ email?: unknown; role?: unknown; notify?: unknown }>(req);
  const email = typeof body.email === 'string' ? body.email.trim().toLowerCase() : '';
  if (email.length > 254 || !EMAIL.test(email)) throw new HttpError(400, 'not an e-mail address');
  if (email === me.user.email) throw new HttpError(400, 'that is your own address: as the owner you already have full access');
  if (body.role !== 'viewer' && body.role !== 'editor') throw new HttpError(400, 'role must be viewer or editor');
  const now = deps.now();
  const existing = await env.DB.prepare('SELECT role FROM shares WHERE doc_id = ? AND email = ?').bind(a.doc.id, email).first();
  if (!existing) {
    const n = await env.DB.prepare('SELECT COUNT(*) AS n FROM shares WHERE doc_id = ?').bind(a.doc.id).first<{ n: number }>();
    if ((n?.n ?? 0) >= LIMITS.sharesPerDoc) throw new HttpError(409, 'too many people');
  }
  await env.DB.prepare(
    `INSERT INTO shares (doc_id, email, role, created, invited_by) VALUES (?, ?, ?, ?, ?)
     ON CONFLICT (doc_id, email) DO UPDATE SET role = excluded.role`,
  )
    .bind(a.doc.id, email, body.role, now, me.user.id)
    .run();

  let notified = false;
  if (!existing && body.notify !== false && env.RESEND_API_KEY && env.EMAIL_FROM) {
    const sent = await env.DB.prepare('SELECT COUNT(*) AS n FROM sent_mail WHERE sender_id = ? AND created > ?').bind(me.user.id, now - 86400_000).first<{ n: number }>();
    if ((sent?.n ?? 0) < LIMITS.mailsPerDay) {
      const url = new URL(`/#/s/${a.doc.id}`, req.url).toString();
      notified = await sendShareMail(env, deps, { to: email, inviter: me.user.name || me.user.email, docName: a.doc.name, role: body.role, url });
      if (notified) await env.DB.prepare('INSERT INTO sent_mail (sender_id, created) VALUES (?, ?)').bind(me.user.id, now).run();
    }
  }
  return json({ email, role: body.role, notified });
}

/** DELETE /api/docs/:id/shares/:email: the owner removes someone, or someone removes themselves. */
export async function deleteShare(env: ReadyEnv, a: Access, s: Session | null, email: string): Promise<Response> {
  const me = requireSession(s);
  const target = email.toLowerCase();
  if (a.role !== 'owner' && target !== me.user.email) throw new HttpError(403, 'only the owner can do this');
  await env.DB.prepare('DELETE FROM shares WHERE doc_id = ? AND email = ?').bind(a.doc.id, target).run();
  return empty();
}

/** GET /api/docs/:id/files: exported files kept with the document. */
export async function listFiles(env: ReadyEnv, a: Access): Promise<Response> {
  const prefix = `docs/${a.doc.id}/files/`;
  const page = await env.FILES.list({ prefix });
  return json({ files: page.objects.map((o) => ({ name: o.key.slice(prefix.length), size: o.size, uploaded: o.uploaded.getTime() })) });
}

/** PUT /api/docs/:id/files/:name: keeps an exported .dcw or .dch file with the document. */
export async function putFile(req: Request, env: ReadyEnv, a: Access, s: Session | null, name: string): Promise<Response> {
  need(a, 'editor', s);
  if (!FILE_NAME.test(name)) throw new HttpError(400, 'file names end in .dcw or .dch');
  const body = await req.arrayBuffer();
  if (body.byteLength > LIMITS.fileBytes) throw new HttpError(413, 'file too large');
  const type = name.endsWith('.dch') ? 'application/json' : 'text/plain; charset=utf-8';
  await env.FILES.put(`docs/${a.doc.id}/files/${name}`, body, { httpMetadata: { contentType: type } });
  return json({ name, size: body.byteLength }, 201);
}

export async function getFile(env: ReadyEnv, a: Access, name: string): Promise<Response> {
  if (!FILE_NAME.test(name)) throw new HttpError(404, 'no such file');
  const obj = await env.FILES.get(`docs/${a.doc.id}/files/${name}`);
  if (!obj) throw new HttpError(404, 'no such file');
  return binary(await obj.arrayBuffer(), {
    'content-type': obj.httpMetadata?.contentType ?? 'application/octet-stream',
    'content-disposition': `attachment; filename="${name}"`,
  });
}

/** DELETE /api/me: deletes the account, its documents and their files, and shares to it. */
export async function deleteAccount(env: ReadyEnv, s: Session): Promise<void> {
  const owned = await env.DB.prepare('SELECT id FROM documents WHERE owner_id = ?').bind(s.user.id).all<{ id: string }>();
  for (const d of owned.results) await deletePrefix(env, `docs/${d.id}/`);
  await env.DB.batch([
    env.DB.prepare('DELETE FROM shares WHERE email = ?').bind(s.user.email),
    env.DB.prepare('DELETE FROM users WHERE id = ?').bind(s.user.id),
  ]);
}
