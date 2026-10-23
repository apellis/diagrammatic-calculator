// Reuse the account access policy for every room connection and message.
import { IDLE_MS, type Session, type User } from './auth';
import { access } from './docs';
import type { ReadyEnv } from './env';
import { HttpError } from './http';
export async function roomAccess(env: ReadyEnv, docId: string, identity: { session: string | null; link: string | null }) {
  let s: Session | null = null;
  if (identity.session) {
    const row = await env.DB.prepare('SELECT u.id,u.email,u.name,s.created,s.expires,s.last_seen FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.id_hash=?').bind(identity.session).first<User & {created:number;expires:number;last_seen:number}>();
    const now = Date.now();
    if (!row || row.expires <= now || now - (row.last_seen || row.created) > IDLE_MS) throw new HttpError(401, 'session ended');
    s = { id: '', user: {id:row.id,email:row.email,name:row.name},created:row.created,expires:row.expires };
  }
  const a = await access(env,docId,s,identity.link);
  if (!s && a.role === 'editor') a.role = 'viewer';
  return {a,s};
}
