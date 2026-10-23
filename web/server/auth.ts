// Sign-in with Google (authorization code flow with PKCE), sessions and CSRF protection.
import type { Deps, ReadyEnv } from './env';
import { equal, pkceChallenge, randomToken, seal, sha256, sign, unseal, verify } from './crypto';
import { clearCookie, cookies, redirect, setCookie } from './http';
import { GOOGLE_AUTH, GOOGLE_TOKEN, verifyIdToken } from './oidc';

export const SESSION_COOKIE = '__Host-dc_session';
export const FLOW_COOKIE = '__Host-dc_signin';
const SESSION_SECONDS = 30 * 86400;
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

export const redirectUri = (req: Request) => new URL('/api/auth/callback', req.url).toString();

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
    redirect_uri: redirectUri(req),
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
      redirect_uri: redirectUri(req),
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
    env.DB.prepare('INSERT INTO sessions (id_hash, user_id, created, expires) VALUES (?, ?, ?, ?)').bind(await sha256(id), user!.id, now, now + SESSION_SECONDS * 1000),
  ]);
  const value = `${id}.${await sign(env.SESSION_SECRET, 'session', id)}`;
  return redirect(flow.ret, [setCookie(SESSION_COOKIE, value, SESSION_SECONDS), clearCookie(FLOW_COOKIE)]);
}

/** The session of a request, if its cookie is genuine and unexpired. */
export async function currentSession(req: Request, env: ReadyEnv, deps: Deps): Promise<Session | null> {
  const value = cookies(req)[SESSION_COOKIE];
  if (!value) return null;
  const [id, sig] = value.split('.');
  if (!id || !sig || !(await verify(env.SESSION_SECRET, 'session', id, sig))) return null;
  const row = await env.DB.prepare(
    'SELECT u.id, u.email, u.name, s.expires FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.id_hash = ?',
  )
    .bind(await sha256(id))
    .first<User & { expires: number }>();
  if (!row || row.expires < deps.now()) return null;
  return { id, user: { id: row.id, email: row.email, name: row.name } };
}

/** The CSRF token of a session: the page reads it from /api/me and sends it as X-CSRF-Token. */
export const csrfToken = (env: ReadyEnv, s: Session) => sign(env.SESSION_SECRET, 'csrf', s.id);

/**
 * Whether a state-changing request really comes from our page: not from another site (Origin
 * and Sec-Fetch-Site, when the browser sends them), and carrying the session's CSRF token.
 */
export async function csrfOk(req: Request, env: ReadyEnv, s: Session): Promise<boolean> {
  const origin = req.headers.get('origin');
  if (origin !== null && origin !== new URL(req.url).origin) return false;
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
