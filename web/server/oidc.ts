// Google sign-in (OpenID Connect): the endpoints, and validation of the ID token against Google's
// published keys.
import type { Deps } from './env';
import { equal, fromB64url, fromUtf8, utf8 } from './crypto';

export const GOOGLE_AUTH = 'https://accounts.google.com/o/oauth2/v2/auth';
export const GOOGLE_TOKEN = 'https://oauth2.googleapis.com/token';
export const GOOGLE_JWKS = 'https://www.googleapis.com/oauth2/v3/certs';
export const GOOGLE_ISSUERS = ['https://accounts.google.com', 'accounts.google.com'];

/** Allowed clock difference, in seconds. */
const SKEW = 60;

export class TokenError extends Error {
  constructor(readonly reason: string) {
    super(`ID token rejected: ${reason}`);
    this.name = 'TokenError';
  }
}

interface Jwk {
  kid: string;
  kty: string;
  n: string;
  e: string;
}

let cache: { keys: Jwk[]; until: number } | null = null;

/** Forgets the cached keys (for tests). */
export function clearKeyCache() {
  cache = null;
}

async function googleKeys(deps: Deps, refresh: boolean): Promise<Jwk[]> {
  if (!refresh && cache && cache.until > deps.now()) return cache.keys;
  const r = await deps.fetch(GOOGLE_JWKS);
  if (!r.ok) throw new TokenError('cannot fetch keys');
  const body = (await r.json()) as { keys?: Jwk[] };
  if (!Array.isArray(body.keys)) throw new TokenError('cannot fetch keys');
  const maxAge = Number(/max-age=(\d+)/.exec(r.headers.get('cache-control') ?? '')?.[1] ?? 3600);
  cache = { keys: body.keys, until: deps.now() + Math.min(maxAge, 86400) * 1000 };
  return body.keys;
}

export interface Identity {
  sub: string;
  email: string;
  name: string;
}

function part(s: string): Record<string, unknown> {
  const b = fromB64url(s);
  if (!b) throw new TokenError('malformed');
  try {
    const v = JSON.parse(fromUtf8(b)) as unknown;
    if (typeof v !== 'object' || v === null) throw new Error();
    return v as Record<string, unknown>;
  } catch {
    throw new TokenError('malformed');
  }
}

/**
 * Checks an ID token from Google's token endpoint: an RS256 signature by one of Google's current
 * keys, the issuer, the audience (our client ID), the expiry and issue time, the nonce of this
 * sign-in, and a verified e-mail address. Returns who signed in.
 */
export async function verifyIdToken(token: string, expect: { clientId: string; nonce: string }, deps: Deps): Promise<Identity> {
  const parts = token.split('.');
  if (parts.length !== 3) throw new TokenError('malformed');
  const header = part(parts[0]);
  const claims = part(parts[1]);
  if (header.alg !== 'RS256' || typeof header.kid !== 'string') throw new TokenError('algorithm');
  let jwk = (await googleKeys(deps, false)).find((k) => k.kid === header.kid);
  // keys rotate: an unknown key ID fetches the list again once
  jwk ??= (await googleKeys(deps, true)).find((k) => k.kid === header.kid);
  if (!jwk || jwk.kty !== 'RSA') throw new TokenError('unknown key');
  const key = await crypto.subtle.importKey('jwk', { kty: 'RSA', n: jwk.n, e: jwk.e, alg: 'RS256', ext: true }, { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' }, false, ['verify']);
  const sig = fromB64url(parts[2]);
  if (!sig || !(await crypto.subtle.verify('RSASSA-PKCS1-v1_5', key, sig, utf8(`${parts[0]}.${parts[1]}`)))) throw new TokenError('signature');

  const now = deps.now() / 1000;
  if (typeof claims.iss !== 'string' || !GOOGLE_ISSUERS.includes(claims.iss)) throw new TokenError('issuer');
  const aud = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  if (!aud.includes(expect.clientId)) throw new TokenError('audience');
  if (aud.length > 1 && claims.azp !== expect.clientId) throw new TokenError('audience');
  if (typeof claims.exp !== 'number' || claims.exp < now - SKEW) throw new TokenError('expired');
  if (typeof claims.iat !== 'number' || claims.iat > now + SKEW) throw new TokenError('issued in the future');
  if (typeof claims.nonce !== 'string' || !equal(claims.nonce, expect.nonce)) throw new TokenError('nonce');
  if (typeof claims.sub !== 'string' || !claims.sub) throw new TokenError('subject');
  if (typeof claims.email !== 'string' || !claims.email || claims.email_verified !== true) throw new TokenError('e-mail not verified');
  return { sub: claims.sub, email: claims.email.toLowerCase(), name: typeof claims.name === 'string' ? claims.name.slice(0, 200) : '' };
}
