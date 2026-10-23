import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { b64url, equal, fromB64url, pkceChallenge, seal, sign, unseal, utf8, verify } from './crypto';
import { clearKeyCache, verifyIdToken } from './oidc';
import { CLIENT_ID, FakeGoogle, World } from './testing';

describe('crypto helpers', () => {
  it('computes the PKCE challenge of RFC 7636, appendix B', async () => {
    expect(await pkceChallenge('dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk')).toBe('E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM');
  });

  it('round-trips base64url and rejects other alphabets', () => {
    const bytes = new Uint8Array([0, 251, 255, 62, 63, 1]);
    expect(fromB64url(b64url(bytes))).toEqual(bytes);
    expect(fromB64url('a+b/')).toBeNull();
  });

  it('signs with a purpose, so a signature for one use does not pass for another', async () => {
    const s = await sign('secret', 'session', 'abc');
    expect(await verify('secret', 'session', 'abc', s)).toBe(true);
    expect(await verify('secret', 'csrf', 'abc', s)).toBe(false);
    expect(await verify('other', 'session', 'abc', s)).toBe(false);
    expect(await verify('secret', 'session', 'abd', s)).toBe(false);
    expect(await verify('secret', 'session', 'abc', 'not base64!')).toBe(false);
  });

  it('seals values so they can be neither read nor changed', async () => {
    const t = await seal('secret', 'signin', { verifier: 'plain-verifier' });
    expect(await unseal('secret', 'signin', t)).toEqual({ verifier: 'plain-verifier' });
    expect(t).not.toContain(b64url(utf8('plain-verifier')).slice(0, 8));
    const [iv, ct] = t.split('.');
    const flipped = fromB64url(ct)!;
    flipped[0] ^= 1;
    expect(await unseal('secret', 'signin', `${iv}.${b64url(flipped)}`)).toBeNull();
    expect(await unseal('secret', 'signin', `${iv}.${ct}.x`)).toBeNull();
    expect(await unseal('secret', 'other', t)).toBeNull();
    expect(await unseal('other secret', 'signin', t)).toBeNull();
    expect(await unseal('secret', 'signin', undefined)).toBeNull();
    expect(await unseal('secret', 'signin', 'garbage')).toBeNull();
  });

  it('compares strings fully', () => {
    expect(equal('abc', 'abc')).toBe(true);
    expect(equal('abc', 'abd')).toBe(false);
    expect(equal('abc', 'abcd')).toBe(false);
    expect(equal('', '')).toBe(true);
  });
});

describe('ID token validation', () => {
  let w: World;
  const nonce = 'the-nonce';
  const who = { sub: '1234', email: 'Ada@Example.org', name: 'Ada' };
  const check = (token: string) => verifyIdToken(token, { clientId: CLIENT_ID, nonce }, w.deps);
  const token = (over: Record<string, unknown> = {}) => w.google.sign({ ...w.google.claims(who, nonce, w.clock), ...over });

  beforeAll(async () => {
    w = await World.create();
  });
  afterAll(() => w.dispose());
  beforeEach(() => clearKeyCache());

  it('accepts a genuine token and lower-cases the address', async () => {
    expect(await check(await token())).toEqual({ sub: '1234', email: 'ada@example.org', name: 'Ada' });
  });

  it('accepts the bare issuer Google also uses', async () => {
    await expect(check(await token({ iss: 'accounts.google.com' }))).resolves.toBeTruthy();
  });

  it.each([
    ['issuer', { iss: 'https://accounts.example.org' }],
    ['audience', { aud: 'someone-else' }],
    ['audience', { aud: [CLIENT_ID, 'other'], azp: 'other' }],
    ['expired', { exp: Math.floor(Date.UTC(2026, 9, 23, 11) / 1000) }],
    ['issued in the future', { iat: Math.floor(Date.UTC(2026, 9, 23, 13) / 1000) }],
    ['nonce', { nonce: 'another' }],
    ['nonce', { nonce: undefined }],
    ['e-mail not verified', { email_verified: false }],
    ['e-mail not verified', { email_verified: 'true' }],
    ['subject', { sub: '' }],
  ])('rejects a token with a wrong %s', async (reason, over) => {
    await expect(check(await token(over))).rejects.toThrow(reason);
  });

  it('rejects a token signed by another key under a known key ID', async () => {
    const other = await FakeGoogle.keyPair();
    const t = await w.google.sign(w.google.claims(who, nonce, w.clock), undefined, other.privateKey);
    await expect(check(t)).rejects.toThrow('signature');
  });

  it('rejects altered claims', async () => {
    const [h, , s] = (await token()).split('.');
    const c = b64url(utf8(JSON.stringify({ ...w.google.claims({ ...who, sub: '9999' }, nonce, w.clock) })));
    await expect(check(`${h}.${c}.${s}`)).rejects.toThrow('signature');
  });

  it('rejects other algorithms and malformed tokens', async () => {
    const claims = w.google.claims(who, nonce, w.clock);
    const none = `${b64url(utf8(JSON.stringify({ alg: 'none', kid: w.google.kid })))}.${b64url(utf8(JSON.stringify(claims)))}.`;
    await expect(check(none)).rejects.toThrow('algorithm');
    const hs = `${b64url(utf8(JSON.stringify({ alg: 'HS256', kid: w.google.kid })))}.${b64url(utf8(JSON.stringify(claims)))}.abc`;
    await expect(check(hs)).rejects.toThrow('algorithm');
    await expect(check('a.b')).rejects.toThrow('malformed');
    await expect(check('!!.??.**')).rejects.toThrow('malformed');
  });

  it('fetches the keys again once for an unknown key ID, then caches them', async () => {
    const before = w.google.jwksFetches;
    await expect(check(await w.google.sign(w.google.claims(who, nonce, w.clock), { alg: 'RS256', kid: 'unknown' }))).rejects.toThrow('unknown key');
    expect(w.google.jwksFetches - before).toBe(2);
    await check(await token());
    await check(await token());
    expect(w.google.jwksFetches - before).toBe(2);
  });

  it('follows a key rotation', async () => {
    await check(await token());
    const next = await FakeGoogle.keyPair();
    w.google.extraJwks = [{ ...(await crypto.subtle.exportKey('jwk', next.publicKey)), kid: 'key-2' } as JsonWebKey];
    const t = await w.google.sign(w.google.claims(who, nonce, w.clock), { alg: 'RS256', kid: 'key-2' }, next.privateKey);
    await expect(check(t)).resolves.toMatchObject({ sub: '1234' });
    w.google.extraJwks = [];
  });
});
