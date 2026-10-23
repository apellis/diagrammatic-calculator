// Hibernating SQLite-backed room. The public Worker deliberately has no ingress;
// only the authenticated Pages service binding can address a DocRoom.
import * as Y from 'yjs';
import {navigation,colour} from './presence';
import * as enc from 'lib0/encoding';
import * as dec from 'lib0/decoding';
import * as sync from 'y-protocols/sync';
import { roomAccess } from '../server/room-auth';
import { ACCOUNT_BYTES_SQL, LIMITS, type Access } from '../server/docs';
import type { ReadyEnv } from '../server/env';
import type { RoomIdentity } from '../server/live';
import { randomToken } from '../server/crypto';
import { HttpError, readBody } from '../server/http';
import { pruneVersions } from '../server/versions';
import { DOC_BYTES, MESSAGE_BYTES, DocumentTooLarge, freshDoc, stageUpdate } from './validation';

interface Socket extends WebSocket { serializeAttachment(value:unknown):void;deserializeAttachment():Attachment }
interface Store {
 get<T>(key:string):Promise<T|undefined>;put(key:string,value:unknown):Promise<void>;delete(key:string|string[]):Promise<unknown>;
 list<T>(options:{prefix:string}):Promise<Map<string,T>>;transaction<T>(f:(s:Store)=>Promise<T>):Promise<T>;
 getAlarm():Promise<number|null>;setAlarm(time:number):Promise<void>;deleteAlarm():Promise<void>;deleteAll():Promise<void>;
}
interface State { storage:Store;blockConcurrencyWhile<T>(f:()=>Promise<T>):Promise<T>;acceptWebSocket(s:Socket):void;getWebSockets():Socket[] }
interface Attachment { identity:RoomIdentity; socketId:string;userId:string|null;name:string;role:string;window:number;count:number;awarenessWindow?:number;awarenessCount?:number;client?:number;clock?:number;presence?:unknown }
declare const WebSocketPair: new()=>{0:Socket;1:Socket};
function join(parts:Uint8Array[]){const out=new Uint8Array(parts.reduce((n,p)=>n+p.byteLength,0));let offset=0;for(const p of parts){out.set(p,offset);offset+=p.byteLength;}return out;}
async function removeKeys(st:Store,keys:string[]){for(let i=0;i<keys.length;i+=128)await st.delete(keys.slice(i,i+128));}
async function chunks(st:Store,prefix:string,bytes:Uint8Array){for(let i=0;i<bytes.byteLength;i+=65536)await st.put(prefix+String(i).padStart(10,'0'),bytes.slice(i,i+65536));}
const status=(generation:number)=>{const e=enc.createEncoder();enc.writeVarUint(e,2);enc.writeVarString(e,JSON.stringify({generation,saved:true}));return enc.toUint8Array(e);};
const updateMessage=(u:Uint8Array)=>{const e=enc.createEncoder();enc.writeVarUint(e,0);sync.writeUpdate(e,u);return enc.toUint8Array(e);};

export class DocRoom {
 private doc=freshDoc();private id='';private generation=0;private seq=0;private initialized=false;
 constructor(private ctx:State,private env:ReadyEnv) {}
 private serial<T>(f:()=>Promise<T>):Promise<T>{return this.ctx.blockConcurrencyWhile(f);}
 private async load(id:string){
  if(this.initialized){if(this.id!==id)throw new HttpError(400,'wrong room');return;}
  const meta=await this.ctx.storage.get<{id:string;generation:number;seq:number;version:number}>('meta');
  if(meta&&meta.id!==id)throw new HttpError(400,'wrong room');
  this.id=id;
  const row=await this.env.DB.prepare('SELECT generation,snapshot_key,version FROM documents WHERE id=?').bind(id).first<{generation:number;snapshot_key:string|null;version:number}>();
  if(!row)throw new HttpError(404,'document deleted');
  this.generation=row.generation;this.seq=meta?.seq??0;
  // Legacy checkpoints must retain deleted structs referenced by old Y snapshots.
  this.doc.destroy();this.doc=freshDoc(row.generation!==0);
  const parts=await this.ctx.storage.list<Uint8Array>({prefix:'state:'});
  const stored=parts.size?join([...parts.values()]):undefined;
  if(stored&&meta?.version===row.version)Y.applyUpdate(this.doc,stored);
  else if(row.snapshot_key){const obj=await this.env.FILES.get(row.snapshot_key);if(!obj)throw new HttpError(503,'snapshot unavailable');Y.applyUpdate(this.doc,new Uint8Array(await obj.arrayBuffer()));}
  await this.ctx.storage.transaction(async st=>{if(!stored||meta?.version!==row.version){await removeKeys(st,[...parts.keys()]);await chunks(st,'state:',Y.encodeStateAsUpdate(this.doc));}await st.put('meta',{id,generation:this.generation,seq:this.seq,version:row.version});});
  this.initialized=true;
 }
 private async who(identity:RoomIdentity){const w=await roomAccess(this.env,this.id,identity);if(w.a.doc.generation!==identity.generation||identity.generation!==this.generation)throw new HttpError(409,'generation changed');return w;}
 async fetch(req:Request):Promise<Response>{return this.serial(async()=>{
  try {
   const id=req.headers.get('x-dc-doc');if(!id)throw new HttpError(400,'missing document');
   const path=new URL(req.url).pathname;
   if(path==='/internal/revoke'){this.id=id;const exists=await this.env.DB.prepare('SELECT id FROM documents WHERE id=?').bind(id).first();if(!exists){for(const ws of this.ctx.getWebSockets())await this.close(ws,4010,'document deleted');await this.ctx.storage.deleteAll();await this.ctx.storage.deleteAlarm();this.doc.destroy();this.doc=freshDoc();this.initialized=false;}else await this.recheck();return new Response(null,{status:204});}
   await this.load(id);
   const identity=JSON.parse(req.headers.get('x-dc-identity')??'null') as RoomIdentity;
   if(!identity||typeof identity.nonce!=='string')throw new HttpError(401,'missing identity');
   const {a,s}=await this.who(identity);
   if(path==='/live'&&req.headers.get('upgrade')?.toLowerCase()==='websocket'){
    const sockets=this.ctx.getWebSockets();
    if(sockets.length>=20)throw new HttpError(429,'socket limit');
    const used=await this.ctx.storage.get<number>('nonce:'+identity.nonce);if(used)throw new HttpError(401,'ticket already used');
    await this.ctx.storage.put('nonce:'+identity.nonce,Date.now());
    // Expired tickets cannot return through Pages. Bound replay bookkeeping without idle work.
    for(const [k,t] of await this.ctx.storage.list<number>({prefix:'nonce:'}))if(Date.now()-t>120000)await this.ctx.storage.delete(k);
    const pair=new WebSocketPair(),ws=pair[1];
    const at:Attachment={identity,socketId:randomToken(12),userId:s?.user.id??null,name:s?.user.name??'Guest',role:a.role,window:Date.now(),count:0};
    // One D1 statement reserves capacity across rooms without a count/insert race.
    const admitted=await this.env.DB.prepare('INSERT INTO live_sockets(doc_id,socket_id,user_id,since) SELECT ?,?,?,? WHERE (SELECT COUNT(*) FROM live_sockets WHERE doc_id=?)<20 AND (? IS NULL OR (SELECT COUNT(*) FROM live_sockets WHERE user_id=?)<10)').bind(id,at.socketId,at.userId,Date.now(),id,at.userId,at.userId).run();
    if(admitted.meta.changes!==1)throw new HttpError(429,'socket limit');
    ws.serializeAttachment(at);this.ctx.acceptWebSocket(ws);
    const e=enc.createEncoder();enc.writeVarUint(e,0);sync.writeSyncStep1(e,this.doc);ws.send(enc.toUint8Array(e));
    for(const other of sockets){const p=other.deserializeAttachment();if(p.client!==undefined)ws.send(this.awareness(p));}
    const alarm=await this.ctx.storage.getAlarm();if(alarm===null||alarm>Date.now()+60000)await this.ctx.storage.setAlarm(Date.now()+60000);
    return new Response(null,{status:101,webSocket:pair[0]} as ResponseInit);
   }
   if(a.role==='viewer'||!s)throw new HttpError(403,'read only');
   const migration=path==='/internal/migrate';
   const decision=path.match(/^\/internal\/proposal\/([^/]+)\/decision$/);
   if(!migration&&!decision&&path!=='/internal/snapshot')throw new HttpError(404,'no route');
   if(migration&&a.role!=='owner')throw new HttpError(403,'owner only');
   if(!migration&&this.generation===0)throw new HttpError(409,'owner migration required');
   if(!decision&&!migration&&req.headers.get('if-match')!==`"${a.doc.version}"`)throw new HttpError(409,'document changed');
   const body=new Uint8Array(await readBody(req,DOC_BYTES));
   let next;
   try{next=stageUpdate(this.doc,body,migration);}catch(e){throw e instanceof DocumentTooLarge?new HttpError(413,e.message):new HttpError(400,'invalid document update');}
   try {
    if(decision){
     const base=req.headers.get('x-proposal-base');
     const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new Uint8Array(Y.encodeStateAsUpdate(this.doc))))).map(n=>n.toString(16).padStart(2,'0')).join('');
     if(base!==digest)throw new HttpError(409,'proposal base changed');
     const key=decodeURIComponent(decision[1]),before=this.doc.getMap<Record<string,unknown>>('proposals').get(key),after=next.doc.getMap<Record<string,unknown>>('proposals').get(key),wanted=req.headers.get('x-proposal-state');
     if(!before||before.status!=='pending')throw new HttpError(409,'proposal already decided');
     if(!['accepted','rejected'].includes(wanted??'')||after?.status!==wanted||after?.decidedBy!==s.user.id)throw new HttpError(400,'invalid proposal decision');
    }
    this.checkProposalTransitions(next.doc,decision?decodeURIComponent(decision[1]):undefined);
    if(migration){await this.checkpoint(a,s.user.id,'before migration');this.generation++;}
    await this.persist(next,a,s.user.id);
    if(migration)for(const ws of this.ctx.getWebSockets())await this.close(ws,4011,'generation changed');
    else await this.broadcast(updateMessage(next.update));
    return Response.json({version:a.doc.version+1,generation:this.generation},{headers:{'x-doc-version':String(a.doc.version+1),'x-doc-generation':String(this.generation),'x-doc-role':a.role}});
   }finally{if(next.doc!==this.doc){next.doc.destroy();if(migration)this.generation=a.doc.generation;}}
  }catch(e){return Response.json({error:e instanceof HttpError?e.code:'room error'},{status:e instanceof HttpError?e.status:500});}
 });}
 private checkProposalTransitions(next:Y.Doc,allowed?:string){
  const before=this.doc.getMap<Record<string,unknown>>('proposals'),after=next.getMap<Record<string,unknown>>('proposals');
  for(const [key,p] of before){const n=after.get(key);if(key!==allowed&&JSON.stringify(n)!==JSON.stringify(p)&& (p.status!=='pending'||n?.status!=='pending'))throw new HttpError(409,'proposal decisions require arbitration');}
  for(const [key,p] of after)if(!before.has(key)&&p.status!=='pending')throw new HttpError(409,'new proposal must be pending');
 }
 private async persist(next:ReturnType<typeof stageUpdate>,a:Access,author:string){
  const old=this.doc,seq=this.seq+1,gen=this.generation;
  // SQLite-backed DO transaction is the durable authority, including a bounded update log.
  // R2 is the REST/checkpoint projection; acknowledgements wait for that projection as well.
  const key=`docs/${this.id}/snapshots/live-${gen}-${seq}-${randomToken(8)}`;
  await this.env.FILES.put(key,next.state,{httpMetadata:{contentType:'application/octet-stream'}});
  const r=await this.env.DB.prepare(`UPDATE documents SET snapshot_key=?,size=?,version=version+1,edited=?,generation=?,name=? WHERE id=? AND version=? AND (${ACCOUNT_BYTES_SQL}) + ? <= ?`).bind(key,next.state.byteLength,Date.now(),gen,typeof next.doc.getMap('meta').get('name')==='string'?String(next.doc.getMap('meta').get('name')).trim().slice(0,200)||a.doc.name:a.doc.name,this.id,a.doc.version,a.doc.owner_id,this.id,a.doc.owner_id,a.doc.owner_id,next.state.byteLength+a.doc.files_size,LIMITS.ownerBytes).run();
  if(r.meta.changes!==1){await this.env.FILES.delete(key);throw new HttpError(409,'document changed or storage limit reached');}
  try { await this.ctx.storage.transaction(async st=>{
   const previous=await st.list({prefix:'state:'});if(previous.size)await removeKeys(st,[...previous.keys()]);
   await chunks(st,'state:',next.state);await st.put('meta',{id:this.id,generation:gen,seq,version:a.doc.version+1});
   const log=await st.list<Uint8Array>({prefix:'log:'});
   if(seq%32===0||[...log.values()].reduce((n,u)=>n+u.byteLength,0)+next.update.byteLength>8*MESSAGE_BYTES){if(log.size)await removeKeys(st,[...log.keys()]);}
   await chunks(st,'log:'+String(seq).padStart(12,'0')+':',next.update);await st.put('author',author);
  }); }catch(e){this.initialized=false;await this.load(this.id);throw e;}
  this.doc=next.doc;this.seq=seq;old.destroy();
  if(a.doc.snapshot_key){await this.env.DB.prepare('INSERT OR IGNORE INTO pending_deletes(prefix,created) VALUES (?,?)').bind(a.doc.snapshot_key,Date.now()).run();try{await this.env.FILES.delete(a.doc.snapshot_key);await this.env.DB.prepare('DELETE FROM pending_deletes WHERE prefix=?').bind(a.doc.snapshot_key).run();}catch{/* durable cleanup queue */}}
 }
 private async checkpoint(a:Access,author:string,note='checkpoint'){
  const bytes=Y.encodeStateAsUpdate(this.doc),id=randomToken(12),key=`docs/${this.id}/versions/${id}`;
  await this.env.FILES.put(key,bytes);
  const r=await this.env.DB.prepare(`INSERT INTO doc_versions(id,doc_id,time,author,note,size,object_key) SELECT ?,?,?,?,?,?,? WHERE (${ACCOUNT_BYTES_SQL}) + ? <= ?`).bind(id,this.id,Date.now(),author,note,bytes.byteLength,key,a.doc.owner_id,'',a.doc.owner_id,a.doc.owner_id,bytes.byteLength,LIMITS.ownerBytes).run();
  if(r.meta.changes!==1){await this.env.FILES.delete(key);throw new HttpError(409,'checkpoint storage limit');}
  await pruneVersions(this.env,this.id,Date.now());await this.ctx.storage.put('checkpointTime',Date.now());
 }
 async webSocketMessage(ws:Socket,message:string|ArrayBuffer){return this.serial(async()=>{
  let next:ReturnType<typeof stageUpdate>|undefined;
  try{
   const at=ws.deserializeAttachment();await this.load((await this.ctx.storage.get<{id:string}>('meta'))?.id??this.id);
   const {a,s}=await this.who(at.identity);
   if(typeof message==='string'||message.byteLength>MESSAGE_BYTES){await this.close(ws,4008,'message too large');return;}
   const now=Date.now();if(now-at.window>=5000){at.window=now;at.count=0;}at.count++;ws.serializeAttachment(at);
   if(at.count>250){await this.close(ws,4008,'rate limit');return;}
   const d=dec.createDecoder(new Uint8Array(message)),type=dec.readVarUint(d);
   if(type===0){
    const subtype=dec.readVarUint(d);
    if(subtype===0){const vector=dec.readVarUint8Array(d);if(dec.hasContent(d))throw new Error('trailing data');const e=enc.createEncoder();enc.writeVarUint(e,0);sync.writeSyncStep2(e,this.doc,vector);ws.send(enc.toUint8Array(e));return;}
    if(subtype!==1&&subtype!==2)throw new Error('bad sync');
    if(a.role==='viewer'||!s){await this.close(ws,4003,'read only');return;}
    const bytes=dec.readVarUint8Array(d);if(dec.hasContent(d))throw new Error('trailing data');
    next=stageUpdate(this.doc,bytes);this.checkProposalTransitions(next.doc);
    await this.persist(next,a,s.user.id);
    await this.broadcast(updateMessage(next.update));ws.send(status(this.generation));
   }else if(type===1){const bytes=dec.readVarUint8Array(d);if(dec.hasContent(d))throw new Error('trailing data');await this.receiveAwareness(ws,at,bytes);}
   else throw new Error('bad message');
  }catch(e){await this.close(ws,e instanceof DocumentTooLarge?4009:e instanceof HttpError?(e.status===401?4001:e.status===409?4011:4003):4003,'message rejected');}
  finally{if(next&&next.doc!==this.doc)next.doc.destroy();}
 });}
 private awareness(at:Attachment,removed=false){const payload=enc.createEncoder();enc.writeVarUint(payload,1);enc.writeVarUint(payload,at.client!);enc.writeVarUint(payload,(at.clock??0)+(removed?1:0));enc.writeVarString(payload,JSON.stringify(removed?null:at.presence));const out=enc.createEncoder();enc.writeVarUint(out,1);enc.writeVarUint8Array(out,enc.toUint8Array(payload));return enc.toUint8Array(out);}
 private async receiveAwareness(ws:Socket,at:Attachment,bytes:Uint8Array){
  if(bytes.byteLength>4096){await this.close(ws,4008,'presence too large');return;}
  const now=Date.now();if(at.awarenessWindow===undefined||now-at.awarenessWindow>=1000){at.awarenessWindow=now;at.awarenessCount=0;}
  at.awarenessCount=(at.awarenessCount??0)+1;ws.serializeAttachment(at);
  if(at.awarenessCount>10){await this.close(ws,4008,'presence rate limit');return;}
  const d=dec.createDecoder(bytes);if(dec.readVarUint(d)!==1)throw new Error('one presence per socket');
  const client=dec.readVarUint(d),clock=dec.readVarUint(d),raw=JSON.parse(dec.readVarString(d));
  if(dec.hasContent(d)||(at.client!==undefined&&at.client!==client)||this.ctx.getWebSockets().some(x=>x!==ws&&x.deserializeAttachment().client===client))throw new Error('presence identity');
  if(at.clock!==undefined&&clock<=at.clock)return;
  at.client=client;at.clock=clock;at.presence=raw===null?null:{...navigation(raw),user:{id:at.userId??at.socketId,name:at.name,colour:colour(at.userId??at.socketId)}};
  ws.serializeAttachment(at);await this.broadcast(this.awareness(at),ws);
 }
 private async broadcast(data:Uint8Array,except?:Socket){for(const ws of this.ctx.getWebSockets())if(ws!==except){try{await this.who(ws.deserializeAttachment().identity);ws.send(data);}catch(e){await this.close(ws,e instanceof HttpError&&e.status===401?4001:4003,'access ended');}}}
 private async close(ws:Socket,code:number,reason:string){const at=ws.deserializeAttachment();try{if(ws.readyState===1){const e=enc.createEncoder();enc.writeVarUint(e,2);enc.writeVarString(e,JSON.stringify({saved:false,code}));ws.send(enc.toUint8Array(e));}ws.close(code,reason);}catch{/* already closed */}await this.env.DB.prepare('DELETE FROM live_sockets WHERE doc_id=? AND socket_id=?').bind(this.id,at.socketId).run();}
 private async recheck(){for(const ws of this.ctx.getWebSockets())try{await roomAccess(this.env,this.id,ws.deserializeAttachment().identity);}catch{await this.close(ws,4001,'access ended');}}
 async webSocketClose(ws:Socket){return this.serial(async()=>{await this.closed(ws);});}
 async webSocketError(ws:Socket){return this.serial(async()=>{await this.closed(ws);});}
 private async closed(ws:Socket){const at=ws.deserializeAttachment();const meta=await this.ctx.storage.get<{id:string}>('meta');if(!this.id&&meta)this.id=meta.id;await this.close(ws,1000,'closed');if(at.client!==undefined)await this.broadcast(this.awareness(at,true),ws);if(this.ctx.getWebSockets().filter(s=>s!==ws&&s.readyState===1).length===0){await this.ctx.storage.deleteAlarm();try{await this.load(this.id);const {a}=await this.who(at.identity);await this.checkpoint(a,at.userId??'guest');}catch{/* deleted/revoked rooms have no final checkpoint */}}}
 async alarm(){return this.serial(async()=>{const meta=await this.ctx.storage.get<{id:string}>('meta');if(!meta)return;await this.load(meta.id);await this.recheck();const sockets=this.ctx.getWebSockets().filter(s=>s.readyState===1);if(!sockets.length){await this.ctx.storage.deleteAlarm();return;}try{const at=sockets[0].deserializeAttachment(),{a}=await this.who(at.identity);if(Date.now()-(await this.ctx.storage.get<number>('checkpointTime')??0)>=300000)await this.checkpoint(a,await this.ctx.storage.get<string>('author')??at.userId??'guest');}finally{await this.ctx.storage.setAlarm(Date.now()+60000);}});}
}
export default {fetch(){return new Response('Forbidden',{status:403});}};
