import { describe,it,expect } from 'vitest';
import * as Y from 'yjs';
import { stageUpdate } from './validation';
describe('room staged validation',()=>{
 it('rejects unknown roots without changing authoritative content',()=>{
  const doc=new Y.Doc(); doc.getMap('meta').set('name','safe');
  const bad=new Y.Doc(); Y.applyUpdate(bad,Y.encodeStateAsUpdate(doc));bad.getMap('meta').set('name','poison');bad.getMap('secret').set('x',1);
  expect(()=>stageUpdate(doc,Y.encodeStateAsUpdate(bad))).toThrow();
  expect(doc.getMap('meta').get('name')).toBe('safe');expect(doc.share.has('secret')).toBe(false);
 });
 it('rejects malformed cells and pending unresolved structs',()=>{
  const bad=new Y.Doc();bad.getArray('cells').push(['bad']);expect(()=>stageUpdate(new Y.Doc(),Y.encodeStateAsUpdate(bad))).toThrow();
 });
 it('accepts Y.Text and repairs duplicate public cell IDs deterministically',()=>{
  const d=new Y.Doc();for(let i=0;i<2;i++){const c=new Y.Map();c.set('id','df1');c.set('key',String(i));c.set('text',new Y.Text('x'));d.getArray('cells').push([c]);}
  const out=stageUpdate(new Y.Doc(),Y.encodeStateAsUpdate(d));
  expect(new Set(out.doc.getArray<Y.Map<unknown>>('cells').toArray().map(c=>c.get('id'))).size).toBe(2);
 });
});
