// Encoding, random tokens, HMAC signing and PKCE, on Web Crypto (the same in Workers and Node).

const enc = new TextEncoder();
const dec = new TextDecoder();

export function b64url(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/** Decodes base64url; null when the text is not base64url. */
export function fromB64url(s: string): Uint8Array<ArrayBuffer> | null {
  if (!/^[A-Za-z0-9_-]*$/.test(s)) return null;
  const pad = s.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (s.length % 4)) % 4);
  try {
    const bin = atob(pad);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  } catch {
    return null;
  }
}

export const utf8 = (s: string) => enc.encode(s);
export const fromUtf8 = (b: Uint8Array) => dec.decode(b);

/** A random token of `bytes` bytes, base64url-encoded. */
export function randomToken(bytes = 32): string {
  return b64url(crypto.getRandomValues(new Uint8Array(bytes)));
}

export async function sha256(text: string): Promise<string> {
  return b64url(new Uint8Array(await crypto.subtle.digest('SHA-256', enc.encode(text))));
}

/** Compares two strings in time independent of where they differ. */
export function equal(a: string, b: string): boolean {
  const x = enc.encode(a);
  const y = enc.encode(b);
  let diff = x.length ^ y.length;
  for (let i = 0; i < Math.max(x.length, y.length); i++) diff |= (x[i] ?? 0) ^ (y[i] ?? 0);
  return diff === 0;
}

const keys = new Map<string, Promise<CryptoKey>>();
function hmacKey(secret: string): Promise<CryptoKey> {
  let k = keys.get(secret);
  if (!k) {
    k = crypto.subtle.importKey('raw', enc.encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign', 'verify']);
    keys.set(secret, k);
  }
  return k;
}

/** HMAC-SHA-256 of `purpose:data`; the purpose keeps signatures for one use from passing for another. */
export async function sign(secret: string, purpose: string, data: string): Promise<string> {
  const sig = await crypto.subtle.sign('HMAC', await hmacKey(secret), enc.encode(`${purpose}:${data}`));
  return b64url(new Uint8Array(sig));
}

export async function verify(secret: string, purpose: string, data: string, signature: string): Promise<boolean> {
  const sig = fromB64url(signature);
  if (!sig) return false;
  return crypto.subtle.verify('HMAC', await hmacKey(secret), sig, enc.encode(`${purpose}:${data}`));
}

const aesKeys = new Map<string, Promise<CryptoKey>>();
function aesKey(secret: string, purpose: string): Promise<CryptoKey> {
  const id = `${purpose}\u0000${secret}`;
  let k = aesKeys.get(id);
  if (!k) {
    k = (async () => {
      const base = await crypto.subtle.importKey('raw', enc.encode(secret), 'HKDF', false, ['deriveKey']);
      return crypto.subtle.deriveKey(
        { name: 'HKDF', hash: 'SHA-256', salt: new Uint8Array(32), info: enc.encode(`diagcalc ${purpose}`) },
        base,
        { name: 'AES-GCM', length: 256 },
        false,
        ['encrypt', 'decrypt'],
      );
    })();
    aesKeys.set(id, k);
  }
  return k;
}

/**
 * A JSON value encrypted and authenticated (AES-256-GCM, key derived from the secret for this
 * purpose): the holder can neither read nor change it.
 */
export async function seal(secret: string, purpose: string, value: unknown): Promise<string> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, await aesKey(secret, purpose), enc.encode(JSON.stringify(value))));
  return `${b64url(iv)}.${b64url(ct)}`;
}

export async function unseal<T>(secret: string, purpose: string, token: string | undefined): Promise<T | null> {
  if (!token) return null;
  const [ivText, ctText, extra] = token.split('.');
  const iv = fromB64url(ivText ?? '');
  const ct = fromB64url(ctText ?? '');
  if (!iv || iv.length !== 12 || !ct || extra !== undefined) return null;
  try {
    const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, await aesKey(secret, purpose), ct);
    return JSON.parse(dec.decode(plain)) as T;
  } catch {
    return null;
  }
}

/** The PKCE code challenge (method S256) of a code verifier, RFC 7636. */
export async function pkceChallenge(verifier: string): Promise<string> {
  return b64url(new Uint8Array(await crypto.subtle.digest('SHA-256', enc.encode(verifier))));
}
