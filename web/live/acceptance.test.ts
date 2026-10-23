import {beforeAll,afterAll,it,expect} from 'vitest';
import {Miniflare} from 'miniflare';
import {rolldown} from 'rolldown';
import * as Y from 'yjs';
import * as enc from 'lib0/encoding';
import * as dec from 'lib0/decoding';
import {applyMigrations} from '../server/testing';
import type {D1Database,R2Bucket} from '../server/env';
import publicWorker from './room';
import { LiveProvider, type LiveSocket } from '../src/account/live';
import type { Api } from '../src/account/api';
import { DOC_BYTES } from './validation';
let mf:Miniflare,db:D1Database,bucket:R2Bucket;
const owner='acceptance-owner';
beforeAll(async()=>{const b=await rolldown({input:new URL('./testing-entry.ts',import.meta.url).pathname});const out=await b.generate({format:'es'});await b.close();mf=new Miniflare({modules:true,script:out.output[0].code,compatibilityDate:'2026-07-30',durableObjects:{LIVE:{className:'DocRoom',useSQLite:true}},d1Databases:['DB'],r2Buckets:['FILES']});db=await mf.getD1Database('DB') as unknown as D1Database;bucket=await mf.getR2Bucket('FILES') as unknown as R2Bucket;await applyMigrations(db);const now=Date.now();await db.prepare('INSERT INTO users(id,google_sub,email,name,created,last_login) VALUES (?,?,?,?,?,?)').bind(owner,owner,'acceptance@example.org','Owner',now,now).run();});
afterAll(async()=>{await mf?.dispose();});
async function fixture(generation=1){const id=crypto.randomUUID().replaceAll('-','').slice(0,16),session=crypto.randomUUID(),now=Date.now();await db.prepare('INSERT INTO documents(id,owner_id,name,created,edited,generation) VALUES (?,?,?,?,?,?)').bind(id,owner,'Acceptance',now,now,generation).run();await db.prepare('INSERT INTO sessions(id_hash,user_id,created,expires,last_seen) VALUES (?,?,?,?,?)').bind(session,owner,now,now+3600000,now).run();const identity=JSON.stringify({session,link:null,nonce:crypto.randomUUID(),generation});return {id,session,identity};}
type Fixture=Awaited<ReturnType<typeof fixture>>;
async function request(f:Fixture,path:string,init:RequestInit={}){const headers={'x-dc-doc':f.id,'x-dc-identity':f.identity,...Object.fromEntries(new Headers(init.headers))};if(new Headers(init.headers).has('upgrade'))return mf.dispatchFetch('http://localhost'+path,{headers});return fetch(new URL(path,await mf.ready),{...init,headers});}
function event(ws:WebSocket,type:string,match:(e:any)=>boolean=()=>true):Promise<any>{return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clean();reject(new Error('socket timeout '+type));},3000);const done=(e:Event)=>{if(match(e)){clean();resolve(e);}};function clean(){clearTimeout(timer);ws.removeEventListener(type,done);}ws.addEventListener(type,done);});}
async function connect(f:Fixture){const url=new URL('/live',await mf.ready);url.protocol='ws:';url.searchParams.set('doc',f.id);url.searchParams.set('identity',f.identity);const ws=new WebSocket(url);ws.binaryType='arraybuffer';await event(ws,'message');return ws;}
async function close(ws:WebSocket){if(ws.readyState===WebSocket.OPEN){const end=event(ws,'close');ws.close();await end;}}
async function state(f:Fixture){return (await request(f,'/test/state')).json() as Promise<{alarm:number|null;sockets:number[];meta:{generation:number;version:number;seq:number}}>}
function packet(d:Y.Doc){const e=enc.createEncoder();enc.writeVarUint(e,0);enc.writeVarUint(e,2);enc.writeVarUint8Array(e,Y.encodeStateAsUpdate(d));return enc.toUint8Array(e);}
const isAck=(e:MessageEvent)=>{const d=dec.createDecoder(new Uint8Array(e.data));return dec.readVarUint(d)===2&&JSON.parse(dec.readVarString(d)).saved===true;};
it('keeps the production public fetch forbidden',()=>expect(publicWorker.fetch().status).toBe(403));
it('rejects nonce replay while the original socket is live and after it closes',async()=>{const f=await fixture(),ws=await connect(f);try{expect((await request(f,'/live',{headers:{upgrade:'websocket'}})).status).toBe(401);}finally{await close(ws);}expect((await request(f,'/live',{headers:{upgrade:'websocket'}})).status).toBe(401);});
it('emits one durable acknowledgement per accepted message only to its sender',async()=>{const f=await fixture(),a=await connect(f),b=await connect({...f,identity:JSON.stringify({...JSON.parse(f.identity),nonce:crypto.randomUUID()})});let own=0,peer=0;a.addEventListener('message',e=>{if(isAck(e))own++;});b.addEventListener('message',e=>{if(isAck(e))peer++;});try{const d=new Y.Doc();d.getMap('meta').set('name','durable');const saved=event(a,'message',isAck);a.send(packet(d));await saved;const row=await db.prepare('SELECT snapshot_key,version FROM documents WHERE id=?').bind(f.id).first<{snapshot_key:string;version:number}>();const stored=new Y.Doc();Y.applyUpdate(stored,new Uint8Array(await (await bucket.get(row!.snapshot_key))!.arrayBuffer()));expect(stored.getMap('meta').get('name')).toBe('durable');expect((await state(f)).meta.version).toBe(row!.version);await new Promise(r=>setTimeout(r,50));expect({own,peer}).toEqual({own:1,peer:0});}finally{await close(a);await close(b);}});
it('retains the exact legacy checkpoint and fences old-generation requests after migration',async()=>{const f=await fixture(0),old=new Y.Doc({gc:false});const t=old.getText('legacy');t.insert(0,'deleted history');t.delete(0,7);old.getArray('history').push([{old:true}]);const bytes=Y.encodeStateAsUpdate(old),key='legacy/'+f.id;await bucket.put(key,bytes);await db.prepare('UPDATE documents SET snapshot_key=?,size=? WHERE id=?').bind(key,bytes.byteLength,f.id).run();const next=new Y.Doc();next.getMap('meta').set('name','migrated');const response=await request(f,'/internal/migrate',{method:'POST',body:new Uint8Array(Y.encodeStateAsUpdate(next))});expect(response.status).toBe(200);expect(await response.json()).toMatchObject({generation:1});const v=await db.prepare('SELECT object_key FROM doc_versions WHERE doc_id=? AND note=?').bind(f.id,'before migration').first<{object_key:string}>();expect(new Uint8Array(await (await bucket.get(v!.object_key))!.arrayBuffer())).toEqual(bytes);expect((await request(f,'/internal/migrate',{method:'POST',body:new Uint8Array(Y.encodeStateAsUpdate(next))})).status).toBe(409);expect((await request(f,'/live',{headers:{upgrade:'websocket'}})).status).toBe(409);});
it('migration fences existing sockets and stale writes without retaining an idle alarm',async()=>{const f=await fixture(),ws=await connect(f),next=new Y.Doc();next.getMap('meta').set('name','new generation');try{const migrated=await request(f,'/internal/migrate',{method:'POST',body:new Uint8Array(Y.encodeStateAsUpdate(next))});expect(migrated.status).toBe(200);expect(await migrated.json()).toMatchObject({generation:2});expect((await state(f)).sockets.filter(s=>s===1)).toHaveLength(0);expect((await request(f,'/internal/snapshot',{method:'POST',headers:{'if-match':'"1"'},body:new Uint8Array(Y.encodeStateAsUpdate(next))})).status).toBe(409);await request(f,'/test/alarm',{method:'POST'});expect((await state(f)).alarm).toBeNull();}finally{ws.close();}});
it('ceases alarm work after the final socket closes',async()=>{const f=await fixture(),ws=await connect(f);expect((await state(f)).alarm).not.toBeNull();await close(ws);await expect.poll(async()=> (await state(f)).alarm).toBeNull();const before=await db.prepare('SELECT count(*) n FROM doc_versions WHERE doc_id=?').bind(f.id).first<{n:number}>();await request(f,'/test/alarm',{method:'POST'});expect((await state(f)).alarm).toBeNull();expect(await db.prepare('SELECT count(*) n FROM doc_versions WHERE doc_id=?').bind(f.id).first()).toEqual(before);});
it('alarm revokes idle sessions and does not reschedule without authorized sockets',async()=>{const f=await fixture(),ws=await connect(f);await db.prepare('DELETE FROM sessions WHERE id_hash=?').bind(f.session).run();try{expect((await request(f,'/test/schedule-alarm',{method:'POST'})).status).toBe(204);await expect.poll(async()=>await db.prepare('SELECT socket_id FROM live_sockets WHERE doc_id=?').bind(f.id).first()).toBeNull();expect((await state(f)).alarm).toBeNull();expect((await state(f)).sockets.filter(s=>s===1)).toHaveLength(0);expect(await db.prepare('SELECT socket_id FROM live_sockets WHERE doc_id=?').bind(f.id).first()).toBeNull();}finally{ws.close();}});
it.each(['revoke', 'schedule-alarm'])('the real provider reports idle revocation from %s and closes even when a peer close handshake stalls', async (action) => {
  const f = await fixture();
  const url = new URL('/live', await mf.ready); url.protocol = 'ws:';
  url.searchParams.set('doc', f.id); url.searchParams.set('identity', f.identity);
  let socket: WebSocket | undefined;
  const doc = new Y.Doc();
  const api = { liveTicket: async () => ({ ticket: 'test', role: 'owner', generation: 1 }), liveUrl: () => url.href } as unknown as Api;
  const provider = new LiveProvider({ api, ydoc: doc, docId: f.id, generation: 1, role: 'owner',
    socketFactory: address => { socket = new WebSocket(address); return socket as unknown as LiveSocket; } });
  try {
    provider.start();
    await expect.poll(() => provider.status).toBe('live');
    await db.prepare('DELETE FROM sessions WHERE id_hash=?').bind(f.session).run();
    expect((await request(f, action === 'revoke' ? '/internal/revoke' : '/test/schedule-alarm', { method: 'POST' })).status).toBe(204);
    await expect.poll(() => provider.status, { timeout: 3000 }).toBe('auth-required');
    await expect.poll(() => socket?.readyState, { timeout: 3000 }).toBe(WebSocket.CLOSED);
    expect(await db.prepare('SELECT socket_id FROM live_sockets WHERE doc_id=?').bind(f.id).first()).toBeNull();
  } finally { provider.stop(); doc.destroy(); }
});

it('rejects a small update that would grow a chunked document past 16 MiB with 4009, without changing it', async () => {
  const f = await fixture();
  const original = new Y.Doc(); original.getMap('meta').set('payload', 'x'.repeat(DOC_BYTES - 4096));
  const bytes = Y.encodeStateAsUpdate(original), key = `boundary/${f.id}`;
  await bucket.put(key, bytes);
  await db.prepare('UPDATE documents SET snapshot_key=?,size=? WHERE id=?').bind(key, bytes.byteLength, f.id).run();
  const vector = Y.encodeStateVector(original); original.getMap('meta').set('extra', 'y'.repeat(8192));
  const e = enc.createEncoder(); enc.writeVarUint(e, 0); enc.writeVarUint(e, 2);
  enc.writeVarUint8Array(e, Y.encodeStateAsUpdate(original, vector));
  const ws = await connect(f);
  try {
    const ended = event(ws, 'close'); ws.send(enc.toUint8Array(e));
    expect((await ended).code).toBe(4009);
    expect(await db.prepare('SELECT snapshot_key,size,version FROM documents WHERE id=?').bind(f.id).first()).toEqual({ snapshot_key: key, size: bytes.byteLength, version: 0 });
  } finally { ws.close(); original.destroy(); }
}, 30_000);
