import {it,expect} from 'vitest';
import {navigation,colour} from './presence';
it('keeps bounded branch, AI, selection and Yjs relative cursor fields but not client identity',()=>{
 const cursor={anchor:{item:{client:123,clock:3},assoc:0},head:{tname:'cells',assoc:-1}};
 const selection={cellId:'df1',word:0,summand:1,layers:[0,1],rectangle:{layers:[0,1],strands:[1,2]}};
 expect(navigation({user:{id:'forged'},branch:'b1',cell:'df1',ai:{active:true,target:'df1'},selection,cursor})).toEqual({branch:'b1',cell:'df1',ai:{active:true,target:'df1'},selection,cursor});
 expect(colour('owner')).toBe(colour('owner'));expect(colour('owner')).not.toBe(colour('viewer'));
});
it('drops malformed relative positions and bounds arrays',()=>{
 const n=navigation({cursor:{anchor:{item:{client:-1,clock:0}},head:'bad'},selection:{cellId:'a',layers:Array(400).fill(1)}});
 expect(n.cursor).toEqual({anchor:null,head:null});expect(n.selection?.layers).toHaveLength(256);
});
it('keeps target-free AI activity without accepting other AI metadata',()=>{
 expect(navigation({branch:'personal branch',ai:{active:true,prompt:'private'}})).toEqual({cell:null,branch:'personal branch',ai:{active:true},selection:null,cursor:null});
 expect(navigation({ai:{active:false}}).ai).toBeNull();
 expect(navigation({ai:{active:'true'}}).ai).toBeNull();
});
