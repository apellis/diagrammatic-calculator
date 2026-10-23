// Registers the accounts API as the receiver of Google's security events (Cross-Account
// Protection, RISC), so that a person's sessions end when Google revokes this app's access, ends
// their sessions or disables their account. Run once by the owner, and again when the site's
// address changes; see docs/deploy.md ("Revoked Google access").
//
//   node scripts/risc-register.mjs <service-account-key.json> <site origin>
//   node scripts/risc-register.mjs key.json https://diagcalc.pages.dev
//
// The key is a JSON key of a service account with the role RISC Configuration Admin in the Google
// Cloud project of the OAuth client. It is read here and sent nowhere; only a short-lived token
// signed with it goes to Google.
import { readFileSync } from 'node:fs';

const RISC = 'https://risc.googleapis.com/v1beta';
const AUDIENCE = 'https://risc.googleapis.com/google.identity.risc.v1beta.RiscManagementService';
const PUSH = 'https://schemas.openid.net/secevent/risc/delivery-method/push';
const EVENTS = [
  'https://schemas.openid.net/secevent/risc/event-type/sessions-revoked',
  'https://schemas.openid.net/secevent/oauth/event-type/tokens-revoked',
  'https://schemas.openid.net/secevent/risc/event-type/account-disabled',
  'https://schemas.openid.net/secevent/risc/event-type/verification',
];

const [keyFile, origin] = process.argv.slice(2);
if (!keyFile || !origin || !/^https:\/\/[^/]+$/.test(origin)) {
  console.error('usage: node scripts/risc-register.mjs <service-account-key.json> <https://site-origin>');
  process.exit(2);
}
const key = JSON.parse(readFileSync(keyFile, 'utf8'));
const b64url = (b) => Buffer.from(b).toString('base64url');

async function bearer() {
  const der = Buffer.from(key.private_key.replace(/-----[^-]+-----/g, '').replace(/\s/g, ''), 'base64');
  const k = await crypto.subtle.importKey('pkcs8', der, { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' }, false, ['sign']);
  const now = Math.floor(Date.now() / 1000);
  const head = b64url(JSON.stringify({ alg: 'RS256', typ: 'JWT', kid: key.private_key_id }));
  const body = b64url(JSON.stringify({ iss: key.client_email, sub: key.client_email, aud: AUDIENCE, iat: now, exp: now + 3600 }));
  const sig = await crypto.subtle.sign('RSASSA-PKCS1-v1_5', k, new TextEncoder().encode(`${head}.${body}`));
  return `${head}.${body}.${b64url(new Uint8Array(sig))}`;
}

async function call(path, payload, token) {
  const r = await fetch(`${RISC}/${path}`, { method: 'POST', headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' }, body: JSON.stringify(payload) });
  const text = await r.text();
  if (!r.ok) {
    console.error(`${path}: ${r.status} ${text}`);
    process.exit(1);
  }
  return text;
}

const token = await bearer();
const url = `${origin}/api/google/risc`;
await call('stream:update', { delivery: { delivery_method: PUSH, url }, events_requested: EVENTS }, token);
console.log(`Registered ${url} for: ${EVENTS.map((e) => e.split('/').pop()).join(', ')}.`);
await call('stream:verify', { state: `registered ${new Date().toISOString()}` }, token);
console.log('Asked Google for a verification event; it is delivered within a few minutes (the API answers it with 202 and logs nothing).');
