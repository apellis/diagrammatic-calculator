// Pages-to-room boundary. Never relay caller-supplied internal identity headers.
import type { Session } from './auth';
import { randomToken, seal, sha256, unseal, sign, verify } from './crypto';
import { access, LIMITS, need, type Access } from './docs';
import { appOrigin, type Deps, type ReadyEnv } from './env';
import { HttpError, json, readBody } from './http';

export interface RoomIdentity { session: string | null; link: string | null; nonce: string; generation: number }
interface Ticket extends RoomIdentity { doc: string; expires: number }

export function sameOrigin(req: Request, env: ReadyEnv): void {
  if (req.headers.get('origin') !== appOrigin(env, req) || (req.headers.has('sec-fetch-site') && req.headers.get('sec-fetch-site') !== 'same-origin')) throw new HttpError(403, 'wrong origin');
}

function stub(env: ReadyEnv, id: string) {
  if (!env.LIVE) throw new HttpError(503, 'live editing is not available');
  return env.LIVE.get(env.LIVE.idFromName(id));
}

async function identity(req: Request, a: Access, s: Session | null): Promise<RoomIdentity> {
  return { session: s ? await sha256(s.id) : null, link: a.viaLink ? req.headers.get('x-link-token') : null, nonce: randomToken(16), generation: a.doc.generation };
}

export async function liveTicket(req: Request, env: ReadyEnv, a: Access, s: Session | null, deps: Deps): Promise<Response> {
  sameOrigin(req, env);
  stub(env, a.doc.id);
  if (a.doc.generation === 0) throw new HttpError(409, 'owner migration required', { generation: 0 });
  // An unsigned-in holder of an edit link receives only viewing rights in the room.
  const role = !s && a.role === 'editor' ? 'viewer' : a.role;
  const ticket = await seal(env.SESSION_SECRET, 'live-ticket', { ...await identity(req, a, s), doc: a.doc.id, expires: deps.now() + 60_000 } satisfies Ticket);
  return json({ ticket:ticket+'.'+await sign(env.SESSION_SECRET,'live-ticket',ticket), role, generation: a.doc.generation });
}

export async function openLive(req: Request, env: ReadyEnv, id: string, s: Session | null, deps: Deps): Promise<Response> {
  sameOrigin(req, env);
  if (req.headers.get('upgrade')?.toLowerCase() !== 'websocket') throw new HttpError(426, 'WebSocket upgrade required');
  const token = new URL(req.url).searchParams.get('ticket');
  const dot=token?.lastIndexOf('.')??-1;
  const sealed=token?.slice(0,dot)??'',signature=token?.slice(dot+1)??'';
  const t = token && token.length <= 4096 && dot>0 && await verify(env.SESSION_SECRET,'live-ticket',sealed,signature) ? await unseal<Ticket>(env.SESSION_SECRET, 'live-ticket', sealed) : null;
  if (!t || t.doc !== id || !Number.isFinite(t.expires) || t.expires <= deps.now() || t.expires > deps.now() + 60_000 || typeof t.nonce !== 'string' || !Number.isSafeInteger(t.generation)) throw new HttpError(401, 'invalid or expired ticket');
  if (t.session !== null && (!s || t.session !== await sha256(s.id))) throw new HttpError(401, 'ticket session ended');
  // The ticket identity, not any additional cookie or X-Link-Token, determines access.
  const a = await access(env, id, t.session ? s : null, t.link);
  if (a.doc.generation !== t.generation) throw new HttpError(409, 'document generation changed', { generation: a.doc.generation });
  const trusted: RoomIdentity = { session: t.session, link: t.link, nonce: t.nonce, generation: t.generation };
  return stub(env, id).fetch(new Request('https://room/live', { headers: { upgrade: 'websocket', 'x-dc-doc': id, 'x-dc-identity': JSON.stringify(trusted) } }));
}

export async function forwardWrite(req: Request, env: ReadyEnv, a: Access, s: Session | null, migrate = false): Promise<Response> {
  need(a, migrate ? 'owner' : 'editor', s);
  const raw = req.headers.get('x-doc-generation');
  if (raw === null || !/^\d+$/.test(raw)) throw new HttpError(428, 'X-Doc-Generation is required');
  if (Number(raw) !== a.doc.generation) throw new HttpError(409, 'document generation changed', { generation: a.doc.generation });
  if (!migrate && a.doc.generation === 0) throw new HttpError(409, 'owner migration required', { generation: 0 });
  const room = stub(env, a.doc.id);
  const body = await readBody(req, LIMITS.snapshotBytes);
  if (!body.byteLength) throw new HttpError(400, 'empty snapshot');
  const headers = new Headers({ 'content-type': 'application/octet-stream', 'x-dc-doc': a.doc.id, 'x-dc-identity': JSON.stringify(await identity(req, a, s)) });
  for (const h of ['if-match', 'x-doc-name', 'x-doc-generation']) if (req.headers.has(h)) headers.set(h, req.headers.get(h)!);
  return room.fetch(new Request(`https://room/internal/${migrate ? 'migrate' : 'snapshot'}`, { method: migrate ? 'POST' : 'PUT', headers, body }));
}

/** Durable permission changes happen first; room failure is reported, not silently acknowledged. */
export async function decideProposal(req:Request,env:ReadyEnv,a:Access,s:Session|null,key:string):Promise<Response>{
 need(a,'editor',s);
 if(!/^[A-Za-z0-9_-]{1,100}$/.test(key))throw new HttpError(400,'invalid proposal key');
 if(req.headers.get('x-doc-generation')!==String(a.doc.generation))throw new HttpError(409,'generation changed');
 const body=await readBody(req,LIMITS.snapshotBytes);
 const headers=new Headers({'x-dc-doc':a.doc.id,'x-dc-identity':JSON.stringify(await identity(req,a,s))});
 for(const h of ['x-proposal-state','x-proposal-base'])if(req.headers.has(h))headers.set(h,req.headers.get(h)!);
 return stub(env,a.doc.id).fetch(new Request('https://room/internal/proposal/'+key+'/decision',{method:'POST',headers,body}));
}

export async function revokeDoc(env: ReadyEnv, id: string): Promise<void> {
  if (!env.LIVE) return;
  const r = await stub(env, id).fetch(new Request('https://room/internal/revoke', { method: 'POST', headers: { 'x-dc-doc': id } }));
  if (!r.ok) throw new HttpError(503, 'live revocation pending');
}

export async function userRooms(env: ReadyEnv, userId: string): Promise<string[]> {
  if (!env.LIVE) return [];
  const rows = await env.DB.prepare('SELECT DISTINCT doc_id FROM live_sockets WHERE user_id = ?').bind(userId).all<{ doc_id: string }>();
  return rows.results.map(r => r.doc_id);
}
export async function revokeRooms(env: ReadyEnv, ids: string[]): Promise<void> {
  await Promise.all([...new Set(ids)].map(id => revokeDoc(env, id)));
}
