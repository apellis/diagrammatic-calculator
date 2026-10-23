const text=(v:unknown,max=200)=>typeof v==='string'?v.slice(0,max):null;
const index=(v:unknown)=>Number.isSafeInteger(v)&&Number(v)>=0&&Number(v)<=1000000;
function pair(v:unknown){return Array.isArray(v)&&v.length===2&&v.every(index)?v:null;}
function relative(v:unknown):unknown {
 if(!v||typeof v!=='object')return null;const p=v as Record<string,unknown>,out:Record<string,unknown>={};
 for(const k of ['type','item'])if(p[k]!==undefined){const id=p[k] as Record<string,unknown>;if(!id||!Number.isSafeInteger(id.client)||Number(id.client)<0||!index(id.clock))return null;out[k]={client:id.client,clock:id.clock};}
 if(p.tname!==undefined){if(typeof p.tname!=='string'||p.tname.length>200)return null;out.tname=p.tname;}
 if(p.assoc!==undefined){if(!Number.isSafeInteger(p.assoc)||Math.abs(Number(p.assoc))>1)return null;out.assoc=p.assoc;}
 return out;
}
/** Only navigation data survives. The authenticated identity is stamped by the caller. */
export function navigation(raw:unknown){
 const f=raw&&typeof raw==='object'?raw as Record<string,unknown>:{};
 const ai=f.ai as Record<string,unknown>|null,sel=f.selection as Record<string,unknown>|null,cur=f.cursor as Record<string,unknown>|null;
 let selection:Record<string,unknown>|null=null;
 if(sel&&typeof sel==='object'&&typeof sel.cellId==='string'){
  const rectangle=sel.rectangle as Record<string,unknown>|null;
  selection={cellId:text(sel.cellId),word:index(sel.word)?sel.word:null,summand:index(sel.summand)?sel.summand:null,layers:Array.isArray(sel.layers)?sel.layers.filter(index).slice(0,256):[],rectangle:rectangle&&pair(rectangle.layers)&&pair(rectangle.strands)?{layers:pair(rectangle.layers),strands:pair(rectangle.strands)}:null};
 }
 return {cell:text(f.cell),branch:text(f.branch),ai:ai?.active===true?{active:true,...(typeof ai.target==='string'?{target:text(ai.target)}:{})}:null,selection,cursor:cur&&typeof cur==='object'?{anchor:relative(cur.anchor),head:relative(cur.head)}:null};
}
export function colour(id:string){const palette=['#537bc4','#9b59b6','#cf692c','#248c70','#b84371','#547a28','#97712c','#257e9c'];let h=2166136261;for(const c of id)h=Math.imul(h^c.charCodeAt(0),16777619);return palette[(h>>>0)%palette.length];}
