import * as Y from 'yjs';
export const DOC_BYTES = 16 * 1024 * 1024;
export const MESSAGE_BYTES = 1024 * 1024;
export class DocumentTooLarge extends Error {
 constructor(){super('document too large');}
}
const maps = ['meta','hints','occurrences','drawn','counters','proposals'];
const arrays = ['cells','cellOrder','snapshots','timeline'];
export function freshDoc(gc=true):Y.Doc {
 const d=new Y.Doc({gc});for(const k of maps)d.getMap(k);for(const k of arrays)d.getArray(k);return d;
}
/** Validate on an isolated clone. Never apply attacker-controlled bytes to the authority. */
export function stageUpdate(current:Y.Doc,update:Uint8Array,replace=false):{doc:Y.Doc;update:Uint8Array;state:Uint8Array} {
 const d=freshDoc();
 try {
  if(!replace)Y.applyUpdate(d,Y.encodeStateAsUpdate(current));
  Y.applyUpdate(d,update);
  if(d.store.pendingStructs || d.store.pendingDs)throw new Error('unresolved update');
  for(const k of d.share.keys())if(!maps.includes(k)&&!arrays.includes(k))throw new Error('unknown root');
  // Map roots may not carry array content and array roots may not carry map content.
  for(const [k,t] of d.share) {
   if(maps.includes(k) && t._start!==null)throw new Error('invalid map root');
   if(arrays.includes(k) && t._map.size)throw new Error('invalid array root');
  }
  const keys=new Set<string>(),ids=new Set<string>();
  const cells=d.getArray<Y.Map<unknown>>('cells');
  if(cells.length>10000)throw new Error('too many cells');
  for(const c of cells) {
   if(!(c instanceof Y.Map)||typeof c.get('id')!=='string'||typeof c.get('key')!=='string'||!(c.get('text') instanceof Y.Text))throw new Error('invalid cell');
   const key=c.get('key') as string,id=c.get('id') as string;
   if(!key||key.length>200||!/^[a-z]+\d+$/.test(id)||keys.has(key))throw new Error('invalid cell identity');
   keys.add(key);
   if(ids.has(id)){const prefix=id.match(/^[a-z]+/)![0];let n=1;while(ids.has(prefix+n)||cells.toArray().some(x=>x.get('id')===prefix+n))n++;c.set('id',prefix+n);}
   ids.add(c.get('id') as string);
  }
  for(const v of d.getArray('cellOrder'))if(typeof v!=='string'||v.length>200)throw new Error('invalid order');
  for(const [id,p] of d.getMap<Record<string,unknown>>('proposals')) {
   if(!p||p.id!==id||typeof p.author!=='string'||typeof p.title!=='string'||typeof p.time!=='number'||!['pending','accepted','rejected'].includes(String(p.status)))throw new Error('invalid proposal');
  }
  const state=Y.encodeStateAsUpdate(d);
  if(state.byteLength>DOC_BYTES)throw new DocumentTooLarge();
  return {doc:d,state,update:Y.encodeStateAsUpdate(d,Y.encodeStateVector(current))};
 }catch(e){d.destroy();throw e;}
}
