import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import { beforeAll,afterAll,it,expect } from 'vitest';
import { Miniflare } from 'miniflare';
import { rolldown } from 'rolldown';
import * as Y from 'yjs';
import * as enc from 'lib0/encoding';
import * as dec from 'lib0/decoding';
import { applyMigrations } from '../server/testing';
import type { D1Database } from '../server/env';
let mf:Miniflare,db:D1Database,persist:string;let options:ConstructorParameters<typeof Miniflare>[0];
const id='abcdefghijklmnop';
const identity=(session='owner')=>JSON.stringify({session,link:null,nonce:crypto.randomUUID(),generation:1});
async function request(path:string,init:RequestInit={},session='owner'){
 return fetch(new URL(path,await mf.ready),{...init,headers:{'x-dc-doc':id,'x-dc-identity':identity(session),...Object.fromEntries(new Headers(init.headers))}});
}
async function connect(session='owner'){const url=new URL('/live',await mf.ready);url.protocol='ws:';url.searchParams.set('doc',id);url.searchParams.set('identity',identity(session));const ws=new WebSocket(url);ws.binaryType='arraybuffer';await event(ws);return ws;}
function packet(d:Y.Doc){const e=enc.createEncoder();enc.writeVarUint(e,0);enc.writeVarUint(e,2);enc.writeVarUint8Array(e,Y.encodeStateAsUpdate(d));return enc.toUint8Array(e);}
function event(ws:WebSocket,type='message',match:(e:any)=>boolean=()=>true):Promise<any>{return new Promise((resolve,reject)=>{const t=setTimeout(()=>reject(new Error('socket timeout '+type)),5000);const f=(e:any)=>{if(match(e)){clearTimeout(t);ws.removeEventListener(type as 'message',f);resolve(e);}};ws.addEventListener(type as 'message',f);});}
const ack=(ws:WebSocket)=>event(ws,'message',e=>dec.readVarUint(dec.createDecoder(new Uint8Array(e.data)))===2);
async function snapshot(){const row=await db.prepare('SELECT snapshot_key FROM documents WHERE id=?').bind(id).first<{snapshot_key:string}>();const obj=await (await mf.getR2Bucket('FILES') as unknown as import('../server/env').R2Bucket).get(row!.snapshot_key);const d=new Y.Doc();Y.applyUpdate(d,new Uint8Array(await obj!.arrayBuffer()));return d;}
beforeAll(async()=>{
 const bundle=await rolldown({input:new URL('./testing-entry.ts',import.meta.url).pathname});const out=await bundle.generate({format:'es'});await bundle.close();
 persist=await mkdtemp(join(tmpdir(),'diagcalc-room-'));options={modules:true,script:out.output[0].code,compatibilityDate:'2026-07-30',durableObjects:{LIVE:{className:'DocRoom',useSQLite:true}},d1Databases:['DB'],r2Buckets:['FILES'],durableObjectsPersist:join(persist,'do'),d1Persist:join(persist,'d1'),r2Persist:join(persist,'r2')};mf=new Miniflare(options);
 db=await mf.getD1Database('DB') as unknown as D1Database;await applyMigrations(db);
 const now=Date.now();for(const who of ['owner','viewer']){await db.prepare('INSERT INTO users(id,google_sub,email,name,created,last_login) VALUES (?,?,?,?,?,?)').bind(who,who,who+'@example.org',who,now,now).run();await db.prepare('INSERT INTO sessions(id_hash,user_id,created,expires,last_seen) VALUES (?,?,?,?,?)').bind(who,who,now,now+3600000,now).run();}
 await db.prepare('INSERT INTO documents(id,owner_id,name,created,edited,generation) VALUES (?,?,?,?,?,1)').bind(id,'owner','Test',now,now).run();
 await db.prepare('INSERT INTO shares(doc_id,email,role,created,invited_by,user_id) VALUES (?,?,?,?,?,?)').bind(id,'viewer@example.org','viewer',now,'owner','viewer').run();
});
afterAll(async()=>{await mf?.dispose();if(persist)await rm(persist,{recursive:true,force:true});});
it('real WebSocket accepts durable state, rejects staged malicious updates without mutation',async()=>{
 const ws=await connect(),d=new Y.Doc();d.getMap('meta').set('name','safe');const saved=ack(ws);ws.send(packet(d));await saved;
 expect((await snapshot()).getMap('meta').get('name')).toBe('safe');
 d.getMap('meta').set('name','poison');d.getMap('unknown').set('x',true);const closed=event(ws,'close');ws.send(packet(d));expect((await closed).code).toBe(4003);
 expect((await snapshot()).getMap('meta').get('name')).toBe('safe');
});
it('viewer cannot send even a valid update and revoked sessions disconnect',async()=>{
 const view=await connect('viewer'),d=new Y.Doc();d.getMap('meta').set('name','viewer');const closed=event(view,'close');view.send(packet(d));expect((await closed).code).toBe(4003);
 const ws=await connect();await db.prepare('DELETE FROM sessions WHERE id_hash=?').bind('owner').run();const ended=event(ws,'close');ws.send(packet(d));expect((await ended).code).toBe(4001);
 const now=Date.now();await db.prepare('INSERT INTO sessions(id_hash,user_id,created,expires,last_seen) VALUES (?,?,?,?,?)').bind('owner','owner',now,now+3600000,now).run();
});
it('seeded disconnected edits converge through the room',async()=>{
 const base=await snapshot(),a=new Y.Doc(),b=new Y.Doc();Y.applyUpdate(a,Y.encodeStateAsUpdate(base));Y.applyUpdate(b,Y.encodeStateAsUpdate(base));let seed=719;for(let i=0;i<12;i++){seed=(seed*1664525+1013904223)>>>0;(seed%2?a:b).getMap('hints').set('k'+i,String(seed));}
 const wa=await connect(),wb=await connect();let saved=ack(wa);wa.send(packet(a));await saved;saved=ack(wb);wb.send(packet(b));await saved;
 Y.applyUpdate(a,Y.encodeStateAsUpdate(b));expect((await snapshot()).getMap('hints').toJSON()).toEqual(a.getMap('hints').toJSON());const ca=event(wa,'close'),cb=event(wb,'close');wa.close();wb.close();await Promise.all([ca,cb]);
});
it('sanitizes identity in awareness and rejects oversized messages',async()=>{
 const a=await connect(),b=await connect('viewer');const payload=enc.createEncoder();enc.writeVarUint(payload,1);enc.writeVarUint(payload,31337);enc.writeVarUint(payload,1);enc.writeVarString(payload,JSON.stringify({user:{id:'forged',name:'Forged'},cell:'df1'}));const outer=enc.createEncoder();enc.writeVarUint(outer,1);enc.writeVarUint8Array(outer,enc.toUint8Array(payload));
 const received=event(b,'message',ev=>dec.readVarUint(dec.createDecoder(new Uint8Array(ev.data)))===1);a.send(enc.toUint8Array(outer));const message=dec.createDecoder(new Uint8Array((await received).data));dec.readVarUint(message);const awareness=dec.createDecoder(dec.readVarUint8Array(message));dec.readVarUint(awareness);dec.readVarUint(awareness);dec.readVarUint(awareness);expect(JSON.parse(dec.readVarString(awareness)).user).toMatchObject({id:'owner',name:'owner'});
 const closed=event(a,'close');a.send(new Uint8Array(1024*1024+1));expect((await closed).code).toBe(4008);const end=event(b,'close');b.close();await end;
});
it('persists through a fresh workerd process and arbitrates proposal decisions once',async()=>{
 const expected=Y.encodeStateAsUpdate(await snapshot());await mf.dispose();mf=new Miniflare(options);db=await mf.getD1Database('DB') as unknown as D1Database;
 const ws=await connect(),e=enc.createEncoder();enc.writeVarUint(e,0);enc.writeVarUint(e,0);enc.writeVarUint8Array(e,Y.encodeStateVector(new Y.Doc()));
 const got=event(ws,'message',ev=>{const d=dec.createDecoder(new Uint8Array(ev.data));return dec.readVarUint(d)===0&&dec.readVarUint(d)===1;});ws.send(enc.toUint8Array(e));const bytes=dec.createDecoder(new Uint8Array((await got).data));dec.readVarUint(bytes);dec.readVarUint(bytes);expect(dec.readVarUint8Array(bytes)).toEqual(expected);
 const base=await snapshot();base.getMap('proposals').set('p1',{id:'p1',author:'owner',time:1,title:'test',payload:{},status:'pending'});const saved=ack(ws);ws.send(packet(base));await saved;
 const current=await snapshot(),hash=createHash('sha256').update(Y.encodeStateAsUpdate(current)).digest('hex');current.getMap('proposals').set('p1',{...current.getMap<Record<string,unknown>>('proposals').get('p1'),status:'accepted',decidedBy:'owner'});
 const init={method:'POST',headers:{'x-proposal-state':'accepted','x-proposal-base':hash},body:new Uint8Array(Y.encodeStateAsUpdate(current))};
 expect((await request('/internal/proposal/p1/decision',init)).status).toBe(200);
 expect((await request('/internal/proposal/p1/decision',init)).status).toBe(409);
 expect((await snapshot()).getMap<Record<string,unknown>>('proposals').get('p1')?.status).toBe('accepted');const closed=event(ws,'close');ws.close();await closed;
});
