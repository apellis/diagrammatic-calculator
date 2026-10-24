import { afterAll, beforeAll, expect, it } from 'vitest';
import { Miniflare, type WebSocket as TestSocket } from 'miniflare';
import { rolldown } from 'rolldown';
import * as Y from 'yjs';
import * as enc from 'lib0/encoding';
import * as dec from 'lib0/decoding';
import { applyMigrations } from '../server/testing';
import { randomToken, sha256, sign } from '../server/crypto';
import type { D1Database } from '../server/env';
import { freshDoc } from './validation';

// Characterizes the real Pages -> DO binding, not Google login (covered separately).
const ORIGIN = 'https://diagcalc.example.org';
const SECRET = 'pages seam test-only session signing secret';
let mf: Miniflare, db: D1Database;
const sockets: TestSocket[] = [];
beforeAll(async () => {
  const bundle = await rolldown({ input: new URL('./pages-entry.test-fixture.ts', import.meta.url).pathname });
  const output = await bundle.generate({ format: 'es' });
  await bundle.close();
  mf = new Miniflare({
    modules: true, script: output.output[0].code, compatibilityDate: '2026-07-30',
    durableObjects: { LIVE: { className: 'DocRoom', useSQLite: true } },
    d1Databases: ['DB'], r2Buckets: ['FILES'],
    bindings: { APP_ORIGIN: ORIGIN, SESSION_SECRET: SECRET,
      GOOGLE_CLIENT_ID: 'pages-seam.apps.example', GOOGLE_CLIENT_SECRET: 'test-only-google-secret' },
  });
  db = await mf.getD1Database('DB') as unknown as D1Database;
  await applyMigrations(db);
});
afterAll(async () => {
  for (const socket of sockets) if (socket.readyState === 1) socket.close();
  await mf?.dispose();
});
type Browser = { cookie: string; csrf: string; user: string; email: string; hash: string };
function request(b: Browser, path: string, init: { method?: string; headers?: Record<string, string>; body?: string | Uint8Array } = {}) {
  return mf.dispatchFetch(ORIGIN + '/api/' + path, { ...init,
    headers: { cookie: b.cookie, origin: ORIGIN, 'x-csrf-token': b.csrf, ...init.headers },
  });
}
async function browser(): Promise<Browser> {
  const user = randomToken(12), id = randomToken(), now = Date.now(), hash = await sha256(id);
  const email = `${user.toLowerCase()}@example.org`;
  await db.prepare('INSERT INTO users(id,google_sub,email,name,created,last_login) VALUES (?,?,?,?,?,?)')
    .bind(user, user, email, 'Seam user', now, now).run();
  await db.prepare('INSERT INTO sessions(id_hash,user_id,created,expires,last_seen) VALUES (?,?,?,?,?)')
    .bind(hash, user, now, now + 3600000, now).run();
  // Same purpose-prefixed HMAC, cookie format, and hashed D1 key as auth.ts.
  const b = { user, email, hash, cookie: `__Host-dc_session=${id}.${await sign(SECRET, 'session', id)}`, csrf: '' };
  const me = await request(b, 'me');
  expect(me.status).toBe(200);
  const body = await me.json() as { accounts: boolean; user: { id: string }; csrf: string };
  expect(body).toMatchObject({ accounts: true, user: { id: user } });
  expect(body.csrf).toBe(await sign(SECRET, 'csrf', id));
  b.csrf = body.csrf;
  return b;
}
async function document(b: Browser) {
  const created = await request(b, 'docs', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ name: 'Pages seam' }) });
  expect(created.status).toBe(201);
  const { id } = await created.json() as { id: string };
  const doc = freshDoc();
  doc.getMap('meta').set('name', 'Pages seam');
  const cell = new Y.Map<unknown>();
  cell.set('id', 'c1'); cell.set('key', 'seam-cell'); cell.set('text', new Y.Text('before'));
  doc.getArray('cells').push([cell]); doc.getArray('cellOrder').push(['seam-cell']);
  const put = await request(b, `docs/${id}/snapshot`, { method: 'PUT',
    headers: { 'if-match': '"0"', 'x-doc-generation': '1' }, body: Y.encodeStateAsUpdate(doc) });
  expect(put.status).toBe(200);
  expect(await put.json()).toMatchObject({ version: 1, generation: 1 });
  return { id, doc };
}
async function ticket(b: Browser, id: string, role = 'owner') {
  const r = await request(b, `docs/${id}/live-ticket`, { method: 'POST' });
  expect(r.status).toBe(200);
  const body = await r.json() as { ticket: string; role: string; generation: number };
  expect(body).toMatchObject({ role, generation: 1 });
  return body.ticket;
}
function upgrade(b: Browser, id: string, token: string, headers: Record<string, string> = {}) {
  return request(b, `docs/${id}/live?ticket=${encodeURIComponent(token)}`, { headers: { upgrade: 'websocket', ...headers } });
}
function event<T>(ws: TestSocket, type: 'message' | 'close', matches: (event: T) => boolean = () => true): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { clean(); reject(new Error(`Pages seam socket timeout: ${type}`)); }, 5000);
    const listener = (e: unknown) => { if (matches(e as T)) { clean(); resolve(e as T); } };
    function clean() { clearTimeout(timer); ws.removeEventListener(type, listener); }
    ws.addEventListener(type, listener);
  });
}
async function connect(b: Browser, id: string, token: string, headers: Record<string, string> = {}) {
  const response = await upgrade(b, id, token, headers);
  expect(response.status).toBe(101);
  const ws = response.webSocket!; sockets.push(ws);
  const initial = event<MessageEvent>(ws, 'message');
  ws.accept();
  const data = dec.createDecoder(new Uint8Array((await initial).data));
  expect(dec.readVarUint(data)).toBe(0); expect(dec.readVarUint(data)).toBe(0);
  return ws;
}
function packet(update: Uint8Array) {
  const e = enc.createEncoder(); enc.writeVarUint(e, 0); enc.writeVarUint(e, 2); enc.writeVarUint8Array(e, update);
  return enc.toUint8Array(e);
}
async function snapshot(b: Browser, id: string, version: number) {
  const r = await request(b, `docs/${id}/snapshot`);
  expect(r.status).toBe(200); expect(r.headers.get('x-doc-version')).toBe(String(version));
  const d = freshDoc(); Y.applyUpdate(d, new Uint8Array(await r.arrayBuffer())); return d;
}
function text(d: Y.Doc) { return (d.getArray<Y.Map<unknown>>('cells').get(0).get('text') as Y.Text); }

it('round-trips Pages REST initialization, a ticketed durable edit, and REST readback; rejects nonce replay', async () => {
  const owner = await browser(), { id, doc } = await document(owner);
  const initial = await snapshot(owner, id, 1); expect(text(initial).toString()).toBe('before'); initial.destroy();
  const token = await ticket(owner, id), ws = await connect(owner, id, token);
  try {
    const replay = await upgrade(owner, id, token);
    expect(replay.status).toBe(401); expect(await replay.json()).toEqual({ error: 'ticket already used' });
    const vector = Y.encodeStateVector(doc); text(doc).insert(6, ' durable');
    const saved = event<MessageEvent>(ws, 'message', e => {
      const d = dec.createDecoder(new Uint8Array(e.data));
      return dec.readVarUint(d) === 2 && JSON.parse(dec.readVarString(d)).saved === true;
    });
    ws.send(packet(Y.encodeStateAsUpdate(doc, vector))); await saved;
    const readback = await snapshot(owner, id, 2);
    expect(text(readback).toString()).toBe('before durable'); readback.destroy();
    const ended = event(ws, 'close'); ws.close(); await ended;
    expect((await upgrade(owner, id, token)).status).toBe(401);
  } finally { if (ws.readyState === 1) ws.close(); doc.destroy(); }
});

it('broadcasts target-free AI activity with authenticated identity through the real room', async () => {
  const owner = await browser(), { id, doc } = await document(owner);
  const sender = await connect(owner, id, await ticket(owner, id));
  const peer = await connect(owner, id, await ticket(owner, id));
  try {
    const seen = event<MessageEvent>(peer, 'message', e => dec.readVarUint(dec.createDecoder(new Uint8Array(e.data))) === 1);
    const state = enc.createEncoder();
    enc.writeVarUint(state, 1); enc.writeVarUint(state, doc.clientID); enc.writeVarUint(state, 1);
    enc.writeVarString(state, JSON.stringify({ branch: 'personal branch', ai: { active: true }, user: { id: 'forged' } }));
    const frame = enc.createEncoder(); enc.writeVarUint(frame, 1); enc.writeVarUint8Array(frame, enc.toUint8Array(state));
    sender.send(enc.toUint8Array(frame));
    const frameIn = dec.createDecoder(new Uint8Array((await seen).data)); expect(dec.readVarUint(frameIn)).toBe(1);
    const body = dec.createDecoder(dec.readVarUint8Array(frameIn)); expect(dec.readVarUint(body)).toBe(1);
    expect(dec.readVarUint(body)).toBe(doc.clientID); dec.readVarUint(body);
    const received = JSON.parse(dec.readVarString(body));
    expect(received).toMatchObject({ user: { id: owner.user }, branch: 'personal branch', ai: { active: true }, cell: null, cursor: null, selection: null });
    expect(received.ai).toEqual({ active: true });
  } finally { sender.close(); peer.close(); doc.destroy(); }
});

it('rejects viewer writes in both REST and the real room, ignoring forged internal identity headers', async () => {
  const owner = await browser(), viewer = await browser(), { id, doc } = await document(owner);
  const share = await request(owner, `docs/${id}/shares`, { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: viewer.email, role: 'viewer' }) });
  expect(share.status).toBe(200);
  const token = await ticket(viewer, id, 'viewer');
  const forged = { 'x-dc-doc': id, 'x-dc-identity': JSON.stringify({ session: owner.hash, nonce: randomToken(), generation: 1, link: null }) };
  const ws = await connect(viewer, id, token, forged);
  try {
    text(doc).insert(0, 'forbidden ');
    expect((await request(viewer, `docs/${id}/snapshot`, { method: 'PUT', headers: { ...forged, 'if-match': '"1"', 'x-doc-generation': '1' }, body: Y.encodeStateAsUpdate(doc) })).status).toBe(403);
    const closed = event<{ code: number }>(ws, 'close'); ws.send(packet(Y.encodeStateAsUpdate(doc)));
    expect((await closed).code).toBe(4003);
    const unchanged = await snapshot(owner, id, 1); expect(text(unchanged).toString()).toBe('before'); unchanged.destroy();
  } finally { if (ws.readyState === 1) ws.close(); doc.destroy(); }
});

it('rejects tampered tickets, cross-document tickets, wrong origin, missing/mismatched/ended sessions', async () => {
  const owner = await browser(), other = await browser(), a = await document(owner), b = await document(owner);
  try {
    const token = await ticket(owner, a.id);
    expect((await upgrade(owner, a.id, token + 'x')).status).toBe(401);
    expect((await upgrade(owner, b.id, token)).status).toBe(401);
    expect((await upgrade(owner, a.id, token, { origin: 'https://attacker.example' })).status).toBe(403);
    expect((await request(owner, `docs/${a.id}/live-ticket`, { method: 'POST', headers: { origin: 'https://attacker.example' } })).status).toBe(403);
    expect((await upgrade(owner, a.id, token, { cookie: '' })).status).toBe(401);
    expect((await upgrade(owner, a.id, token, { cookie: owner.cookie + 'x' })).status).toBe(401);
    expect((await upgrade(other, a.id, token)).status).toBe(401);
    // Rejections at Pages must not burn the room nonce: the real owner still connects.
    const ws = await connect(owner, a.id, token);
    const ended = event(ws, 'close'); ws.close(); await ended;
    const pending = await ticket(owner, a.id);
    await db.prepare('DELETE FROM sessions WHERE id_hash=?').bind(owner.hash).run();
    expect((await upgrade(owner, a.id, pending)).status).toBe(401);
  } finally { a.doc.destroy(); b.doc.destroy(); }
});
