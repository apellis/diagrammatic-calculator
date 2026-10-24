// Documents saved to accounts: metadata in D1, content in R2, and who may do what.
import { appOrigin, type Deps, type ReadyEnv } from './env';
import type { Session } from './auth';
import { equal, randomToken } from './crypto';
import { binary, empty, HttpError, json, readBody, readJson } from './http';
import { recipientKey, sendShareMail } from './mail';
import { revokeDoc, revokeRooms, userRooms } from './live';

export type Role = 'owner' | 'editor' | 'viewer';
const RANK: Record<Role, number> = { viewer: 1, editor: 2, owner: 3 };

/** Limits that keep one account from filling the storage or the e-mail allowance. */
export const LIMITS = {
  snapshotBytes: 16 * 1024 * 1024,
  fileBytes: 16 * 1024 * 1024,
  /** Exported files kept with one document. */
  filesPerDoc: 50,
  /** Snapshots and files of all of an owner's documents, whoever wrote them. */
  ownerBytes: 2 * 1024 * 1024 * 1024,
  docsPerOwner: 1000,
  sharesPerDoc: 100,
  /** Share notifications per sender, per recipient, and in all, in any 24 hours. */
  mailsPerDay: 20,
  mailsPerRecipientPerDay: 3,
  mailsGlobalPerDay: 100,
  /** How old an account must be before its shares send e-mail. */
  mailAccountAgeMs: 86400_000,
  /** R2 list-and-delete rounds one request spends on finishing deletions. */
  deleteRounds: 20,
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
  generation: number;
  size: number;
  snapshot_key: string | null;
  link_role: 'viewer' | 'editor' | null;
  link_token: string | null;
  files_size: number;
  owner_email: string;
  owner_name: string;
}

export interface Access {
  doc: DocRow;
  role: Role;
  /** Whether the role comes from the document's link (it is higher than any role of the caller's own). */
  viaLink: boolean;
  /** The caller's role without the link: as owner or by a share; null for a link holder only. */
  direct: Role | null;
}

const notFound = () => new HttpError(404, 'no such document');

/**
 * The caller's role on a document: owner, a share to their e-mail address, or the document's link
 * (whose token comes in X-Link-Token). Anyone without a role gets 404, so a document's existence
 * is not revealed. A share is bound to the first account that uses it: later, another account
 * presenting the same address (a reassigned work address, say) does not get it.
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
    const shares = await env.DB.prepare('SELECT email, role, user_id FROM shares WHERE doc_id = ? AND (user_id = ? OR (user_id IS NULL AND email = ?))')
      .bind(id, s.user.id, s.user.email)
      .all<{ email: string; role: Role; user_id: string | null }>();
    for (const sh of shares.results) {
      if (!role || RANK[sh.role] > RANK[role]) role = sh.role;
      if (!sh.user_id) await env.DB.prepare('UPDATE shares SET user_id = ? WHERE doc_id = ? AND email = ? AND user_id IS NULL').bind(s.user.id, id, sh.email).run();
    }
  }
  const direct = role;
  if (linkToken && doc.link_token && doc.link_role && equal(linkToken, doc.link_token) && (!role || RANK[doc.link_role] > RANK[role])) {
    role = doc.link_role;
    viaLink = true;
  }
  if (!role) throw notFound();
  return { doc, role, viaLink, direct };
}

export function need(a: Access, role: Role, s: Session | null) {
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
    generation: d.generation,
    size: d.size,
    role: a.role,
    // someone who has the document only through its link sees the owner's name, not their address
    owner: a.direct ? { email: d.owner_email, name: d.owner_name } : { name: d.owner_name },
    ...(a.role === 'owner' ? { link: d.link_role ? { role: d.link_role, token: d.link_token } : null } : {}),
  };
}

export function cleanName(v: unknown): string {
  if (typeof v !== 'string') throw new HttpError(400, 'name must be text');
  const n = v.replace(/[\u0000-\u001f\u007f]/g, ' ').trim().slice(0, LIMITS.nameLength);
  return n || 'Untitled';
}

/**
 * Works off pending deletions of stored objects (documents already gone from the database) in at
 * most `rounds` list-and-delete rounds, so no request exceeds its subrequest allowance; what is
 * left is finished by later requests. Never throws: a failure leaves the work for later.
 */
export async function sweep(env: ReadyEnv, rounds = LIMITS.deleteRounds): Promise<boolean> {
  try {
    const pending = await env.DB.prepare('SELECT prefix FROM pending_deletes ORDER BY created LIMIT 20').all<{ prefix: string }>();
    for (const { prefix } of pending.results) {
      for (;;) {
        if (rounds-- <= 0) return false;
        const page = await env.FILES.list({ prefix });
        if (page.objects.length) await env.FILES.delete(page.objects.map((o) => o.key));
        if (!page.truncated) break;
      }
      await env.DB.prepare('DELETE FROM pending_deletes WHERE prefix = ?').bind(prefix).run();
    }
    return pending.results.length < 20;
  } catch {
    return false;
  }
}

/** Bytes stored for an owner's documents other than `except`. */
export const ACCOUNT_BYTES_SQL = `(SELECT COALESCE(SUM(size + files_size), 0) FROM documents WHERE owner_id = ? AND id != ?)
  + (SELECT COALESCE(SUM(v.size), 0) FROM doc_versions v JOIN documents d ON d.id = v.doc_id WHERE d.owner_id = ?)
  + (SELECT COALESCE(SUM(size), 0) FROM doc_branches WHERE user_id = ?)`;
export async function ownerBytes(env: ReadyEnv, ownerId: string, except = ''): Promise<number> {
  const r = await env.DB.prepare(`SELECT ${ACCOUNT_BYTES_SQL} AS n`).bind(ownerId, except, ownerId, ownerId).first<{ n: number }>();
  return r?.n ?? 0;
}

const overQuota = () => new HttpError(413, "the owner's storage is full");

// --- routes ---------------------------------------------------------------------------------

/** GET /api/docs: the caller's documents and those shared with them. */
export async function listDocs(env: ReadyEnv, s: Session | null): Promise<Response> {
  const me = requireSession(s);
  const owned = await env.DB.prepare(
    'SELECT id, name, created, edited, version, size, link_role FROM documents WHERE owner_id = ? ORDER BY edited DESC',
  )
    .bind(me.user.id)
    .all();
  // shares sent to this address and not yet used by another account become this account's
  await env.DB.prepare('UPDATE shares SET user_id = ? WHERE email = ? AND user_id IS NULL').bind(me.user.id, me.user.email).run();
  const shared = await env.DB.prepare(
    `SELECT d.id, d.name, d.created, d.edited, d.version, d.size, s.role, u.email AS owner_email, u.name AS owner_name
     FROM shares s JOIN documents d ON d.id = s.doc_id JOIN users u ON u.id = d.owner_id
     WHERE s.user_id = ? AND d.owner_id != ? ORDER BY d.edited DESC`,
  )
    .bind(me.user.id, me.user.id)
    .all<Record<string, unknown> & { id: string; role: Role }>();
  // one entry per document, with the highest role (two addresses of the account may be invited)
  const best = new Map<string, Record<string, unknown> & { id: string; role: Role }>();
  for (const r of shared.results) if (!best.has(r.id) || RANK[r.role] > RANK[best.get(r.id)!.role]) best.set(r.id, r);
  return json({
    owned: owned.results.map((r) => ({ ...r, role: 'owner' })),
    shared: [...best.values()].map(({ owner_email, owner_name, ...r }) => ({ ...r, owner: { email: owner_email, name: owner_name } })),
  });
}

/** POST /api/docs {name}: a new, empty document owned by the caller. */
export async function createDoc(req: Request, env: ReadyEnv, s: Session | null, deps: Deps): Promise<Response> {
  const me = requireSession(s);
  const body = await readJson<{ name?: unknown }>(req);
  const name = cleanName(body.name ?? 'Untitled');
  const count = await env.DB.prepare('SELECT COUNT(*) AS n FROM documents WHERE owner_id = ?').bind(me.user.id).first<{ n: number }>();
  if ((count?.n ?? 0) >= LIMITS.docsPerOwner) throw new HttpError(409, 'too many documents');
  await sweep(env, 2);
  const id = randomToken(12);
  const now = deps.now();
  await env.DB.prepare('INSERT INTO documents (id, owner_id, name, created, edited, generation) VALUES (?, ?, ?, ?, ?, 1)').bind(id, me.user.id, name, now, now).run();
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
    } else throw new HttpError(400, 'link must be off, viewer, or editor');
  }
  if (body.link !== undefined) await revokeDoc(env, a.doc.id);
  return json(meta(await access(env, a.doc.id, s, null)));
}

/**
 * DELETE /api/docs/:id: the owner deletes the document and its shares at once, and its stored
 * objects in bounded batches (finished by later requests if there are too many for this one).
 */
export async function deleteDoc(env: ReadyEnv, a: Access, s: Session | null, deps: Deps): Promise<Response> {
  need(a, 'owner', s);
  await env.DB.batch([
    env.DB.prepare('INSERT OR IGNORE INTO pending_deletes (prefix, created) VALUES (?, ?)').bind(`docs/${a.doc.id}/`, deps.now()),
    env.DB.prepare('DELETE FROM documents WHERE id = ?').bind(a.doc.id),
  ]);
  await revokeDoc(env, a.doc.id);
  await sweep(env);
  return empty();
}

/** GET /api/docs/:id/snapshot: the document's content, a Yjs update. */
export async function getSnapshot(env: ReadyEnv, a: Access): Promise<Response> {
  let { version, generation, snapshot_key: key } = a.doc;
  for (let attempt = 0; ; attempt++) {
    const headers = { 'content-type': 'application/octet-stream', 'x-doc-version': String(version), 'x-doc-role': a.role, 'x-doc-generation': String(generation) };
    if (!key) return binary(new ArrayBuffer(0), headers);
    const obj = await env.FILES.get(key);
    if (obj) return binary(await obj.arrayBuffer(), headers);
    // a save replaced the snapshot between reading the row and the object: read both again
    const cur = await env.DB.prepare('SELECT version, generation, snapshot_key FROM documents WHERE id = ?').bind(a.doc.id).first<{ version: number; generation: number; snapshot_key: string | null }>();
    if (!cur) throw notFound();
    if (attempt === 1 || cur.snapshot_key === key) throw new HttpError(500, 'snapshot missing');
    ({ version, generation, snapshot_key: key } = cur);
  }
}

/**
 * GET /api/docs/:id/shares: who has access. The owner and the editors the document is shared with
 * may look (not someone who has it only through its link). Each share says which account has
 * used it, if any.
 */
export async function listShares(env: ReadyEnv, a: Access, s: Session | null): Promise<Response> {
  need(a, 'editor', s);
  if (!a.direct || RANK[a.direct] < RANK.editor) throw new HttpError(403, 'only the owner and the editors it is shared with can see this');
  const rows = await env.DB.prepare(
    `SELECT s.email, s.role, s.created, u.id AS account_id, u.name AS account_name, u.email AS account_email
     FROM shares s LEFT JOIN users u ON u.id = s.user_id WHERE s.doc_id = ? ORDER BY s.created`,
  )
    .bind(a.doc.id)
    .all<{ email: string; role: Role; created: number; account_id: string | null; account_name: string | null; account_email: string | null }>();
  return json({
    owner: { id: a.doc.owner_id, email: a.doc.owner_email, name: a.doc.owner_name },
    shares: rows.results.map(({ account_id, account_name, account_email, ...r }) => ({ ...r, ...(account_id ? { accountId: account_id } : {}), account: account_email === null ? null : { name: account_name ?? '', email: account_email } })),
  });
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

  await revokeDoc(env, a.doc.id);
  let mail: MailOutcome | null = null;
  if (!existing && body.notify !== false) mail = await notify(req, env, deps, me, email, a.doc, body.role);
  return json({ email, role: body.role, notified: mail === 'sent', ...(mail && mail !== 'sent' ? { mail } : {}) });
}

/** What happened to a share notification, when one was asked for. */
export type MailOutcome = 'sent' | 'unavailable' | 'new-account' | 'opted-out' | 'limit' | 'failed';

/**
 * Sends a share notification within the limits: the sender's account must be old enough, the
 * recipient must not have opted out, and the per-sender, per-recipient, and global daily counts
 * must allow it. The count is reserved before sending (one transaction with the check), so
 * concurrent shares cannot pass the limit together, and released if sending fails.
 */
async function notify(req: Request, env: ReadyEnv, deps: Deps, me: Session, to: string, doc: DocRow, role: 'viewer' | 'editor'): Promise<MailOutcome> {
  if (!env.RESEND_API_KEY || !env.EMAIL_FROM) return 'unavailable';
  const now = deps.now();
  const sender = await env.DB.prepare('SELECT created FROM users WHERE id = ?').bind(me.user.id).first<{ created: number }>();
  if (!sender || now - sender.created < LIMITS.mailAccountAgeMs) return 'new-account';
  const rcpt = await recipientKey(env, to);
  if (await env.DB.prepare('SELECT 1 AS x FROM mail_optout WHERE recipient = ?').bind(rcpt).first()) return 'opted-out';
  const since = now - 86400_000;
  const [ins, counts] = await env.DB.batch([
    env.DB.prepare('INSERT INTO sent_mail (sender_id, created, recipient) VALUES (?, ?, ?) RETURNING id').bind(me.user.id, now, rcpt),
    env.DB.prepare(
      `SELECT (SELECT COUNT(*) FROM sent_mail WHERE sender_id = ? AND created > ?) AS mine,
              (SELECT COUNT(*) FROM sent_mail WHERE recipient = ? AND created > ?) AS theirs,
              (SELECT COUNT(*) FROM sent_mail WHERE created > ?) AS everyone`,
    ).bind(me.user.id, since, rcpt, since, since),
  ]);
  const reservation = (ins.results[0] as { id: number }).id;
  const release = () => env.DB.prepare('DELETE FROM sent_mail WHERE id = ?').bind(reservation).run();
  const c = counts.results[0] as { mine: number; theirs: number; everyone: number };
  if (c.mine > LIMITS.mailsPerDay || c.theirs > LIMITS.mailsPerRecipientPerDay || c.everyone > LIMITS.mailsGlobalPerDay) {
    await release();
    return 'limit';
  }
  const origin = appOrigin(env, req);
  const sent = await sendShareMail(env, deps, {
    to,
    inviterName: me.user.name,
    inviterEmail: me.user.email,
    docName: doc.name,
    role,
    url: `${origin}/#/s/${doc.id}`,
    unsubscribeUrl: `${origin}/api/mail/unsubscribe?r=${rcpt}`,
  });
  if (!sent) await release();
  return sent ? 'sent' : 'failed';
}

/** DELETE /api/docs/:id/shares/:email: the owner removes someone, or someone removes themselves. */
export async function deleteShare(env: ReadyEnv, a: Access, s: Session | null, email: string): Promise<Response> {
  const me = requireSession(s);
  const target = email.toLowerCase();
  if (a.role !== 'owner' && target !== me.user.email) throw new HttpError(403, 'only the owner can do this');
  await env.DB.prepare('DELETE FROM shares WHERE doc_id = ? AND email = ?').bind(a.doc.id, target).run();
  await revokeDoc(env, a.doc.id);
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
  const body = await readBody(req, LIMITS.fileBytes).catch((e: unknown) => {
    throw e instanceof HttpError && e.status === 413 ? new HttpError(413, 'file too large') : e;
  });
  const prefix = `docs/${a.doc.id}/files/`;
  const page = await env.FILES.list({ prefix });
  const others = page.objects.filter((o) => o.key !== prefix + name);
  if (page.truncated || others.length >= LIMITS.filesPerDoc) throw new HttpError(409, 'too many files kept with this document');
  const filesSize = others.reduce((n, o) => n + o.size, 0) + body.byteLength;
  if ((await ownerBytes(env, a.doc.owner_id, a.doc.id)) + a.doc.size + filesSize > LIMITS.ownerBytes) throw overQuota();
  const type = name.endsWith('.dch') ? 'application/json' : 'text/plain; charset=utf-8';
  await env.FILES.put(prefix + name, body, { httpMetadata: { contentType: type } });
  await env.DB.prepare('UPDATE documents SET files_size = ? WHERE id = ?').bind(filesSize, a.doc.id).run();
  return json({ name, size: body.byteLength }, 201);
}

/** DELETE /api/docs/:id/files/:name: removes a kept file (editors and the owner). */
export async function deleteFile(env: ReadyEnv, a: Access, s: Session | null, name: string): Promise<Response> {
  need(a, 'editor', s);
  if (!FILE_NAME.test(name)) throw new HttpError(404, 'no such file');
  const prefix = `docs/${a.doc.id}/files/`;
  await env.FILES.delete(prefix + name);
  const page = await env.FILES.list({ prefix });
  await env.DB.prepare('UPDATE documents SET files_size = ? WHERE id = ?').bind(page.objects.reduce((n, o) => n + o.size, 0), a.doc.id).run();
  return empty();
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

/**
 * DELETE /api/me: deletes the account, its documents, and shares to it at once (one transaction),
 * and the documents' stored objects in bounded batches, finished by later requests if need be.
 */
export async function deleteAccount(env: ReadyEnv, s: Session, deps: Deps): Promise<void> {
  const owned = await env.DB.prepare('SELECT id FROM documents WHERE owner_id = ?').bind(s.user.id).all<{ id: string }>();
  const rooms = [...await userRooms(env, s.user.id), ...owned.results.map(r => r.id)];
  await env.DB.batch([
    env.DB.prepare('INSERT OR IGNORE INTO pending_deletes (prefix, created) SELECT object_key, ? FROM doc_branches WHERE user_id = ?').bind(deps.now(), s.user.id),
    env.DB.prepare("INSERT OR IGNORE INTO pending_deletes (prefix, created) SELECT 'docs/' || id || '/', ? FROM documents WHERE owner_id = ?").bind(deps.now(), s.user.id),
    env.DB.prepare('DELETE FROM shares WHERE email = ? OR user_id = ?').bind(s.user.email, s.user.id),
    env.DB.prepare('DELETE FROM users WHERE id = ?').bind(s.user.id),
  ]);
  await revokeRooms(env, rooms);
  await sweep(env);
}
