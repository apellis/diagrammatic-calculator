// Test-only ingress and alarm/storage controls. NEVER deploy this entrypoint.
import { DocRoom as ProductionRoom } from './room';
const realNow=Date.now;
let testNow:number|undefined;
Date.now=()=>testNow??realNow();
export class DocRoom extends ProductionRoom {
 constructor(private state:ConstructorParameters<typeof ProductionRoom>[0],env:ConstructorParameters<typeof ProductionRoom>[1]){super(state,env);}
 async fetch(req:Request):Promise<Response>{
  const path=new URL(req.url).pathname;
  if(path==='/test/state')return Response.json({alarm:await this.state.storage.getAlarm(),sockets:this.state.getWebSockets().map(s=>s.readyState),meta:await this.state.storage.get('meta')});
  if(path==='/test/schedule-alarm'){await this.state.storage.setAlarm(Date.now()+50);return new Response(null,{status:204});}
  if(path==='/test/alarm'){await this.alarm();return Response.json({alarm:await this.state.storage.getAlarm()});}
  if(path==='/test/clock'){const value=await req.json() as {now?:number};testNow=value.now;return Response.json({now:Date.now()});}
  return super.fetch(req);
 }
}
export default {fetch(req:Request,env:{LIVE:{idFromName(s:string):unknown;get(id:unknown):{fetch(req:Request):Promise<Response>}}}){const url=new URL(req.url);req=new Request(req);if(url.searchParams.has('doc'))req.headers.set('x-dc-doc',url.searchParams.get('doc')!);if(url.searchParams.has('identity'))req.headers.set('x-dc-identity',url.searchParams.get('identity')!);return env.LIVE.get(env.LIVE.idFromName(req.headers.get('x-dc-doc')!)).fetch(req);}};
