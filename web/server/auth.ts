// Sign-in with Google (authorization code flow with PKCE), sessions and CSRF protection.
import { appOrigin, type Deps, type ReadyEnv } from './env';
import { equal, pkceChallenge, randomToken, seal, sha256, sign, unseal, verify } from './crypto';
import { clearCookie, cookies, redirect, setCookie } from './http';
import { GOOGLE_AUTH, GOOGLE_ISSUERS, GOOGLE_TOKEN, TokenError, verifyIdToken, verifySecurityEvent } from './oidc';

export const SESSION_COOKIE = '__Host-dc_session';
export const FLOW_COOKIE = '__Host-dc_signin';
/** A session ends 30 days after sign-in at the latest, */
const SESSION_SECONDS = 30 * 86400;
/** or after 7 days without a request. */
export const IDLE_MS = 7 * 86400_000;
/** The last-seen time is written at most this often, not on every request. */
const TOUCH_MS = 3600_000;
/** /api/me replaces a session ID older than this with a fresh one (same expiry). */
export const ROTATE_MS = 86400_000;
/** How long a replaced session ID keeps working, for requests already on their way. */
const ROTATE_GRACE_MS = 60_000;
const FLOW_SECONDS = 600;

export interface User {
  id: string;
  email: string;
  name: string;
}

export interface Session {
  /** The random session ID from the cookie (never stored; the database has its hash). */
  id: string;
  user: User;
  created: number;
  expires: number;
}

/** What the browser keeps, sealed, between leaving for Google and coming back. */
interface Flow {
  state: string;
  nonce: string;
  verifier: string;
  ret: string;
  exp: number;
}

/** A path on this site to return to after signing in; anything else becomes the document list. */
export function safeReturn(ret: string | null): string {
  if (!ret || !/^\/(?![/\\])[^\s\\]*$/.test(ret) || ret.length > 500) return '/';
  return ret;
}

export const redirectUri = (env: ReadyEnv, req: Request) => `${appOrigin(env, req)}/api/auth/callback`;

/** GET /api/auth/login?return=<path>: off to Google, with fresh state, nonce and PKCE verifier. */
export async function login(req: Request, env: ReadyEnv, deps: Deps): Promise<Response> {
  const flow: Flow = {
    state: randomToken(),
    nonce: randomToken(),
    verifier: randomToken(48),
    ret: safeReturn(new URL(req.url).searchParams.get('return')),
    exp: deps.now() + FLOW_SECONDS * 1000,
  };
  const url = new URL(GOOGLE_AUTH);
  url.search = new URLSearchParams({
    client_id: env.GOOGLE_CLIENT_ID,
    redirect_uri: redirectUri(env, req),
    response_type: 'code',
    scope: 'openid email profile',
    state: flow.state,
    nonce: flow.nonce,
    code_challenge: await pkceChallenge(flow.verifier),
    code_challenge_method: 'S256',
    prompt: 'select_account',
  }).toString();
  return redirect(url.toString(), [setCookie(FLOW_COOKIE, await seal(env.SESSION_SECRET, 'signin', flow), FLOW_SECONDS)]);
}

/** GET /api/auth/callback: exchanges the code, checks the ID token, and starts a session. */
export async function callback(req: Request, env: ReadyEnv, deps: Deps): Promise<Response> {
  const params = new URL(req.url).searchParams;
  const done = (outcome: string) => redirect(`/?signin=${outcome}#/`, [clearCookie(FLOW_COOKIE)]);
  const flow = await unseal<Flow>(env.SESSION_SECRET, 'signin', cookies(req)[FLOW_COOKIE]);
  if (!flow || flow.exp < deps.now()) return done('expired');
  if (params.get('error')) return done('cancelled');
  const state = params.get('state');
  const code = params.get('code');
  if (!state || !code || !equal(state, flow.state)) return done('failed');

  const token = await deps.fetch(GOOGLE_TOKEN, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      code,
      client_id: env.GOOGLE_CLIENT_ID,
      client_secret: env.GOOGLE_CLIENT_SECRET,
      redirect_uri: redirectUri(env, req),
      grant_type: 'authorization_code',
      code_verifier: flow.verifier,
    }).toString(),
  });
  if (!token.ok) return done('failed');
  const idToken = ((await token.json()) as { id_token?: unknown }).id_token;
  if (typeof idToken !== 'string') return done('failed');
  let who;
  try {
    who = await verifyIdToken(idToken, { clientId: env.GOOGLE_CLIENT_ID, nonce: flow.nonce }, deps);
  } catch {
    return done('failed');
  }

  const now = deps.now();
  const user = await env.DB.prepare(
    `INSERT INTO users (id, google_sub, email, name, created, last_login) VALUES (?, ?, ?, ?, ?, ?)
     ON CONFLICT (google_sub) DO UPDATE SET email = excluded.email, name = excluded.name, last_login = excluded.last_login
     RETURNING id`,
  )
    .bind(randomToken(12), who.sub, who.email, who.name, now, now)
    .first<{ id: string }>();
  const id = randomToken();
  await env.DB.batch([
    env.DB.prepare('DELETE FROM sessions WHERE expires < ?').bind(now),
    env.DB.prepare('INSERT INTO sessions (id_hash, user_id, created, expires, last_seen) VALUES (?, ?, ?, ?, ?)').bind(await sha256(id), user!.id, now, now + SESSION_SECONDS * 1000, now),
  ]);
  return redirect(flow.ret, [await sessionCookie(env, id, SESSION_SECONDS), clearCookie(FLOW_COOKIE)]);
}

async function sessionCookie(env: ReadyEnv, id: string, seconds: number): Promise<string> {
  return setCookie(SESSION_COOKIE, `${id}.${await sign(env.SESSION_SECRET, 'session', id)}`, Math.max(0, Math.floor(seconds)));
}

/**
 * The session of a request, if its cookie is genuine, unexpired and not idle for longer than
 * IDLE_MS. Marks the session as seen (at most once per TOUCH_MS).
 */
export async function currentSession(req: Request, env: ReadyEnv, deps: Deps): Promise<Session | null> {
  const value = cookies(req)[SESSION_COOKIE];
  if (!value) return null;
  const [id, sig] = value.split('.');
  if (!id || !sig || !(await verify(env.SESSION_SECRET, 'session', id, sig))) return null;
  const hash = await sha256(id);
  const row = await env.DB.prepare(
    'SELECT u.id, u.email, u.name, s.created, s.expires, s.last_seen FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.id_hash = ?',
  )
    .bind(hash)
    .first<User & { created: number; expires: number; last_seen: number }>();
  const now = deps.now();
  if (!row || row.expires < now) return null;
  if (now - (row.last_seen || row.created) > IDLE_MS) {
    await env.DB.prepare('DELETE FROM sessions WHERE id_hash = ?').bind(hash).run();
    return null;
  }
  if (now - row.last_seen > TOUCH_MS) await env.DB.prepare('UPDATE sessions SET last_seen = ? WHERE id_hash = ?').bind(now, hash).run();
  return { id, user: { id: row.id, email: row.email, name: row.name }, created: row.created, expires: row.expires };
}

/**
 * Replaces a session ID older than ROTATE_MS with a fresh one that keeps the same expiry; the old
 * ID keeps working for ROTATE_GRACE_MS so requests already sent with it still pass. Returns the
 * session to use from now on and the cookie to set, if any. Called from GET /api/me, which is
 * where the page reads the CSRF token that goes with the session.
 */
export async function rotate(env: ReadyEnv, s: Session, deps: Deps): Promise<{ s: Session; cookies: string[] }> {
  const now = deps.now();
  if (now - s.created < ROTATE_MS) return { s, cookies: [] };
  const id = randomToken();
  await env.DB.batch([
    env.DB.prepare('INSERT INTO sessions (id_hash, user_id, created, expires, last_seen) VALUES (?, ?, ?, ?, ?)').bind(await sha256(id), s.user.id, now, s.expires, now),
    env.DB.prepare('UPDATE sessions SET expires = MIN(expires, ?) WHERE id_hash = ?').bind(now + ROTATE_GRACE_MS, await sha256(s.id)),
  ]);
  return { s: { ...s, id, created: now }, cookies: [await sessionCookie(env, id, (s.expires - now) / 1000)] };
}

/** The CSRF token of a session: the page reads it from /api/me and sends it as X-CSRF-Token. */
export const csrfToken = (env: ReadyEnv, s: Session) => sign(env.SESSION_SECRET, 'csrf', s.id);

/**
 * Whether a state-changing request really comes from our page: not from another site (Origin
 * and Sec-Fetch-Site, when the browser sends them), and carrying the session's CSRF token.
 */
export async function csrfOk(req: Request, env: ReadyEnv, s: Session): Promise<boolean> {
  const origin = req.headers.get('origin');
  if (origin !== null && origin !== appOrigin(env, req)) return false;
  const site = req.headers.get('sec-fetch-site');
  if (site !== null && site !== 'same-origin') return false;
  const token = req.headers.get('x-csrf-token');
  return !!token && verify(env.SESSION_SECRET, 'csrf', s.id, token);
}

/** POST /api/auth/logout: ends the session here and in the database. */
export async function logout(env: ReadyEnv, s: Session | null): Promise<string[]> {
  if (s) await env.DB.prepare('DELETE FROM sessions WHERE id_hash = ?').bind(await sha256(s.id)).run();
  return [clearCookie(SESSION_COOKIE)];
}

/** POST /api/auth/logout-all: ends every session of the account, in every browser. */
export async function logoutEverywhere(env: ReadyEnv, s: Session): Promise<string[]> {
  await env.DB.prepare('DELETE FROM sessions WHERE user_id = ?').bind(s.user.id).run();
  return [clearCookie(SESSION_COOKIE)];
}

/**
 * The events from Google's Cross-Account Protection (RISC) that end a person's sessions here: Google
 * ended their sessions, revoked the tokens of this app (the person removed its access), or
 * disabled the account. Other events (the stream's verification, an account enabled again, a
 * suggested credential change) are acknowledged and need nothing.
 */
export const ENDING_EVENTS = [
  'https://schemas.openid.net/secevent/risc/event-type/sessions-revoked',
  'https://schemas.openid.net/secevent/oauth/event-type/tokens-revoked',
  'https://schemas.openid.net/secevent/risc/event-type/account-disabled',
];

/** The largest security event token accepted. */
const EVENT_BYTES = 16 * 1024;

/**
 * POST /api/google/risc: the receiver of Google's security events (push delivery). The body is a
 * security event token; once it checks out, every session of the person it names is deleted, in
 * every browser (as Sign out everywhere does). A delivery that comes again later at most signs
 * them out once more. Nothing about the event is stored. Answers 202, or 400 for a token that
 * does not check.
 */
export async function securityEvent(req: Request, env: ReadyEnv, deps: Deps): Promise<Response> {
  const bad = (err: string) => Response.json({ err, description: 'the security event token was not accepted' }, { status: 400 });
  const text = await req.text();
  if (!text || text.length > EVENT_BYTES) return bad('invalid_request');
  let event;
  try {
    event = await verifySecurityEvent(text.trim(), env.GOOGLE_CLIENT_ID, deps);
  } catch (e) {
    if (!(e instanceof TokenError)) throw e;
    return bad(e.reason === 'issuer' ? 'invalid_issuer' : e.reason === 'audience' ? 'invalid_audience' : e.reason === 'unknown key' ? 'invalid_key' : 'invalid_request');
  }
  for (const type of ENDING_EVENTS) {
    const subject = event.events[type]?.subject as { iss?: unknown; sub?: unknown } | undefined;
    if (typeof subject?.sub !== 'string' || !subject.sub) continue;
    // a subject is named by Google's issuer and the Google account ID that sign-in also uses
    if (subject.iss !== undefined && (typeof subject.iss !== 'string' || !GOOGLE_ISSUERS.includes(subject.iss.replace(/\/$/, '')))) continue;
    await env.DB.prepare('DELETE FROM sessions WHERE user_id IN (SELECT id FROM users WHERE google_sub = ?)').bind(subject.sub).run();
  }
  return new Response(null, { status: 202 });
}
