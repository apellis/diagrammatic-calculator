// Test harness for the accounts API: D1 and R2 from Miniflare (the same SQLite and object store
// the Workers runtime uses), the migrations applied as wrangler applies them, a fake Google that
// checks PKCE and signs real RS256 ID tokens, a fake Resend, and browsers with cookie jars.
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { Miniflare } from 'miniflare';
import type { D1Database, Deps, Env, R2Bucket } from './env';
import { b64url, pkceChallenge, randomToken, utf8 } from './crypto';
import { GOOGLE_AUTH, GOOGLE_JWKS, GOOGLE_TOKEN } from './oidc';
import { RESEND_API } from './mail';
import { handle } from './router';
import { access } from './docs';
import { putSnapshot } from './testing-snapshots';
import type { ReadyEnv } from './env';
import type { Session } from './auth';

export const ORIGIN = 'https://diagcalc.example.org';
export const CLIENT_ID = 'test-client.apps.example';
const CLIENT_SECRET = 'test-client-secret';

const migrationsDir = fileURLToPath(new URL('../migrations/', import.meta.url));

/** The migration files in the order wrangler applies them. */
export function migrations(): { name: string; sql: string }[] {
  return readdirSync(migrationsDir)
    .filter((f) => /^\d{4}_[a-z0-9_]+\.sql$/.test(f))
    .sort()
    .map((name) => ({ name, sql: readFileSync(migrationsDir + name, 'utf8') }));
}

/** Splits a migration into statements (our migrations have no semicolons inside literals). */
export function statements(sql: string): string[] {
  return sql
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('--'))
    .join('\n')
    .split(/;\s*$/m)
    .map((s) => s.trim())
    .filter(Boolean);
}

export async function applyMigrations(db: D1Database) {
  for (const m of migrations()) await db.batch(statements(m.sql).map((s) => db.prepare(s)));
}

export async function storage(): Promise<{ mf: Miniflare; DB: D1Database; FILES: R2Bucket }> {
  const mf = new Miniflare({ modules: true, script: 'export default { fetch() { return new Response(null) } }', d1Databases: ['DB'], r2Buckets: ['FILES'] });
  const DB = (await mf.getD1Database('DB')) as unknown as D1Database;
  const FILES = (await mf.getR2Bucket('FILES')) as unknown as R2Bucket;
  return { mf, DB, FILES };
}

export interface Identity {
  sub: string;
  email: string;
  name?: string;
  email_verified?: boolean;
}

/** A Google account server: an RSA key, its JWKS, authorization codes, and the token endpoint. */
export class FakeGoogle {
  kid = 'key-1';
  keys!: CryptoKeyPair;
  jwksFetches = 0;
  /** Extra keys published in the JWKS (for rotation tests). */
  extraJwks: JsonWebKey[] = [];
  private codes = new Map<string, { challenge: string; nonce: string; redirect: string; who: Identity; claims: Record<string, unknown> }>();

  static async create(): Promise<FakeGoogle> {
    const g = new FakeGoogle();
    g.keys = await FakeGoogle.keyPair();
    return g;
  }

  static keyPair(): Promise<CryptoKeyPair> {
    return crypto.subtle.generateKey({ name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' }, true, ['sign', 'verify']);
  }

  async jwks(): Promise<{ keys: JsonWebKey[] }> {
    const pub = await crypto.subtle.exportKey('jwk', this.keys.publicKey);
    return { keys: [{ ...pub, kid: this.kid, alg: 'RS256', use: 'sig' } as JsonWebKey, ...this.extraJwks] };
  }

  /** Signs a JWT with the given header and claims (by default with this server's key). */
  async sign(claims: Record<string, unknown>, header: Record<string, unknown> = { alg: 'RS256', kid: this.kid, typ: 'JWT' }, key = this.keys.privateKey): Promise<string> {
    const h = b64url(utf8(JSON.stringify(header)));
    const c = b64url(utf8(JSON.stringify(claims)));
    const sig = await crypto.subtle.sign('RSASSA-PKCS1-v1_5', key, utf8(`${h}.${c}`));
    return `${h}.${c}.${b64url(new Uint8Array(sig))}`;
  }

  claims(who: Identity, nonce: string, nowMs: number): Record<string, unknown> {
    const now = Math.floor(nowMs / 1000);
    return { iss: 'https://accounts.google.com', aud: CLIENT_ID, azp: CLIENT_ID, sub: who.sub, email: who.email, email_verified: who.email_verified ?? true, name: who.name ?? '', nonce, iat: now, exp: now + 3600 };
  }

  /** What Google does when the person picks an account: returns the code for the redirect. */
  authorize(authUrl: string, who: Identity, override: Record<string, unknown> = {}): { code: string; state: string; redirect: string } {
    const u = new URL(authUrl);
    if (u.origin + u.pathname !== GOOGLE_AUTH) throw new Error('not the Google authorization endpoint');
    const p = u.searchParams;
    if (p.get('client_id') !== CLIENT_ID || p.get('response_type') !== 'code' || p.get('code_challenge_method') !== 'S256') throw new Error('bad authorization request');
    const code = randomToken(16);
    this.codes.set(code, { challenge: p.get('code_challenge')!, nonce: p.get('nonce')!, redirect: p.get('redirect_uri')!, who, claims: override });
    return { code, state: p.get('state')!, redirect: p.get('redirect_uri')! };
  }

  async token(body: URLSearchParams, nowMs: number): Promise<Response> {
    const c = this.codes.get(body.get('code') ?? '');
    this.codes.delete(body.get('code') ?? '');
    if (!c) return Response.json({ error: 'invalid_grant' }, { status: 400 });
    if (body.get('client_id') !== CLIENT_ID || body.get('client_secret') !== CLIENT_SECRET) return Response.json({ error: 'invalid_client' }, { status: 401 });
    if (body.get('redirect_uri') !== c.redirect || body.get('grant_type') !== 'authorization_code') return Response.json({ error: 'invalid_request' }, { status: 400 });
    if ((await pkceChallenge(body.get('code_verifier') ?? '')) !== c.challenge) return Response.json({ error: 'invalid_grant' }, { status: 400 });
    return Response.json({ id_token: await this.sign({ ...this.claims(c.who, c.nonce, nowMs), ...c.claims }), access_token: 'unused' });
  }
}

export interface Mail {
  from: string;
  to: string[];
  subject: string;
  text: string;
  html: string;
  headers?: Record<string, string>;
  auth: string | null;
}

/** Everything a test needs: the API with its storage, Google, Resend, and a clock. */
export class World {
  clock = Date.UTC(2026, 9, 23, 12);
  mails: Mail[] = [];
  resendFails = false;
  env!: Env;
  deps: Deps;
  private constructor(
    readonly mf: Miniflare,
    readonly google: FakeGoogle,
  ) {
    this.deps = { now: () => this.clock, fetch: (input, init) => this.outside(new Request(input, init)) };
  }

  static async create(options: { email?: boolean; accounts?: boolean; appOrigin?: string } = {}): Promise<World> {
    const { mf, DB, FILES } = await storage();
    await applyMigrations(DB);
    const w = new World(mf, await FakeGoogle.create());
    w.env =
      options.accounts === false
        ? {}
        : {
            DB,
            FILES,
            GOOGLE_CLIENT_ID: CLIENT_ID,
            GOOGLE_CLIENT_SECRET: CLIENT_SECRET,
            SESSION_SECRET: 'a test secret that is long enough to sign with',
            ...(options.email === false ? {} : { RESEND_API_KEY: 're_test_key', EMAIL_FROM: 'Calculator <share@send.example.org>' }),
            ...(options.appOrigin ? { APP_ORIGIN: options.appOrigin } : {}),
          };
    if (options.accounts !== false) w.env.LIVE = {
      idFromName: id => id,
      get: () => ({ fetch: async req => {
        if (new URL(req.url).pathname === '/internal/revoke') return new Response(null, { status: 204 });
        const identity = JSON.parse(req.headers.get('x-dc-identity')!);
        const row = await DB.prepare('SELECT u.id, u.email, u.name, s.created, s.expires FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.id_hash = ?').bind(identity.session).first<{ id: string; email: string; name: string; created: number; expires: number }>();
        const s: Session | null = row ? { id: '', user: { id: row.id, email: row.email, name: row.name }, created: row.created, expires: row.expires } : null;
        const a = await access(w.env as ReadyEnv, req.headers.get('x-dc-doc')!, s, identity.link);
        return putSnapshot(req, w.env as ReadyEnv, a, s, w.deps);
      } }),
    };
    return w;
  }

  get DB() {
    return this.env.DB!;
  }
  get FILES() {
    return this.env.FILES!;
  }

  dispose() {
    return this.mf.dispose();
  }

  /** The outside world as the API sees it through `deps.fetch`. */
  private async outside(req: Request): Promise<Response> {
    const url = req.url.split('?')[0];
    if (url === GOOGLE_JWKS) {
      this.google.jwksFetches++;
      return Response.json(await this.google.jwks(), { headers: { 'cache-control': 'public, max-age=20000' } });
    }
    if (url === GOOGLE_TOKEN && req.method === 'POST') return this.google.token(new URLSearchParams(await req.text()), this.clock);
    if (url === RESEND_API && req.method === 'POST') {
      if (this.resendFails) return Response.json({ message: 'refused' }, { status: 422 });
      this.mails.push({ ...((await req.json()) as Omit<Mail, 'auth'>), auth: req.headers.get('authorization') });
      return Response.json({ id: 'mail' });
    }
    throw new Error(`unexpected request to ${url}`);
  }

  browser(): Browser {
    return new Browser(this);
  }

  /** A browser that has signed in as `who`. */
  async signedIn(who: Identity): Promise<Browser> {
    const b = this.browser();
    await b.signIn(who);
    return b;
  }
}

/** A browser on our origin: a cookie jar, the CSRF token, and same-origin request headers. */
export class Browser {
  jar = new Map<string, string>();
  csrf = '';
  /** Headers added to every request (tests set cross-site ones). */
  extra: Record<string, string> = {};
  /** The origin this browser talks to (tests set another host). */
  origin = ORIGIN;
  constructor(readonly world: World) {}

  async fetch(path: string, init: RequestInit & { json?: unknown } = {}): Promise<Response> {
    const headers = new Headers(init.headers);
    if (this.jar.size) headers.set('cookie', [...this.jar].map(([k, v]) => `${k}=${v}`).join('; '));
    const method = (init.method ?? 'GET').toUpperCase();
    if (method !== 'GET') {
      if (!headers.has('origin')) headers.set('origin', this.origin);
      if (this.csrf && !headers.has('x-csrf-token')) headers.set('x-csrf-token', this.csrf);
    }
    for (const [k, v] of Object.entries(this.extra)) headers.set(k, v);
    let body = init.body;
    if (init.json !== undefined) {
      body = JSON.stringify(init.json);
      headers.set('content-type', 'application/json');
    }
    const r = await handle(new Request(this.origin + path, { ...init, method, headers, body, ...(body instanceof ReadableStream ? { duplex: 'half' } : {}) } as RequestInit), this.world.env, this.world.deps);
    for (const c of r.headers.getSetCookie()) {
      const [pair, ...attrs] = c.split(';');
      const i = pair.indexOf('=');
      const name = pair.slice(0, i);
      const maxAge = attrs.map((a) => a.trim()).find((a) => a.startsWith('Max-Age='));
      if (maxAge === 'Max-Age=0') this.jar.delete(name);
      else this.jar.set(name, pair.slice(i + 1));
    }
    return r;
  }

  async signIn(who: Identity, ret = '/'): Promise<Response> {
    const start = await this.fetch(`/api/auth/login?return=${encodeURIComponent(ret)}`);
    const { code, state } = this.world.google.authorize(start.headers.get('location')!, who);
    const back = await this.fetch(`/api/auth/callback?code=${code}&state=${state}`);
    await this.refresh();
    return back;
  }

  /** Reads /api/me, keeping the CSRF token as the page does. */
  async refresh(): Promise<{ accounts: boolean; user: { id: string; email: string; name: string } | null; csrf?: string }> {
    const me = (await (await this.fetch('/api/me')).json()) as { accounts: boolean; user: { id: string; email: string; name: string } | null; csrf?: string };
    this.csrf = me.csrf ?? '';
    return me;
  }

  /** Creates a document and saves `content` as its first snapshot; returns its ID. */
  async newDoc(name = 'Notes', content: Uint8Array<ArrayBuffer> = new Uint8Array([1, 2, 3])): Promise<string> {
    const r = await this.fetch('/api/docs', { method: 'POST', json: { name } });
    if (r.status !== 201) throw new Error(`create failed: ${r.status}`);
    const { id } = (await r.json()) as { id: string };
    const put = await this.fetch(`/api/docs/${id}/snapshot`, { method: 'PUT', headers: { 'if-match': '"0"', 'x-doc-generation': '1' }, body: content });
    if (put.status !== 200) throw new Error(`save failed: ${put.status}`);
    return id;
  }
}
