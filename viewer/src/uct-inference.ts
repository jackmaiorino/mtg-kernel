import type { UctReplay } from './recording';
import type { UctEvent } from './uct-playback';

export const inferenceLabels={u:'UCT: no inference',f:'Forced: no inference',x:'Uncached forward',m:'Cache miss',h:'Cache hit'} as const;
export type InferenceCode=keyof typeof inferenceLabels;
export type Tally={requests:number;hits:number;misses:number;forwards:number;uct:number;forced:number;bytes:number|null};
export type Use={simulation:number;move:number};
export type MoveInference={code:InferenceCode;label:string;call:number|null;group:number|null;use:number|null;first:Use|null;bytes:number|null};
export type InferenceIndex={starts:Tally[];calls:Int32Array[];groups:Int32Array[];bytes:Float64Array[];uses:Map<number,Use[]>};

const empty=():Tally=>({requests:0,hits:0,misses:0,forwards:0,uct:0,forced:0,bytes:null});
function add(t:Tally,code:InferenceCode,bytes:number|null){
  if(code==='u'){t.uct++;return;}
  if(code==='f'){t.forced++;return;}
  t.requests++;t.hits+=Number(code==='h');t.misses+=Number(code==='m');t.forwards+=Number(code!=='h');
  if(bytes!==null)t.bytes=bytes;
}

// One pass over the recorded codes: running totals at each attempt's start,
// each move's call ordinal, input group and cache bytes, and every group's
// uses in timeline order. Absent when the search recorded no inference.
export function inferenceIndex(r:UctReplay):InferenceIndex|null {
  if(!r.inference)return null;
  const starts:Tally[]=[],calls:Int32Array[]=[],groups:Int32Array[]=[],bytes:Float64Array[]=[],uses=new Map<number,Use[]>();
  const running=empty();let call=0;
  for(const [simulation,s] of r.simulations.entries()){
    starts.push({...running});
    const e=s.inference!,n=e.codes.length,c=new Int32Array(n).fill(-1),g=new Int32Array(n).fill(-1),b=new Float64Array(n).fill(-1);
    let gi=0,bi=0;
    for(let move=0;move<n;move++){
      const code=e.codes[move] as InferenceCode;let cached:number|null=null;
      if(code!=='u'&&code!=='f'){
        c[move]=call++;g[move]=e.groups[gi++];
        if(r.inference.mode==='cache'){cached=e.cacheBytes[bi++];b[move]=cached;}
        const list=uses.get(g[move])??[];list.push({simulation,move});uses.set(g[move],list);
      }
      add(running,code,cached);
    }
    calls.push(c);groups.push(g);bytes.push(b);
  }
  starts.push({...running});
  return {starts,calls,groups,bytes,uses};
}

// Recorded moves of the current attempt that belong to the viewed prefix.
export function prefixMoves(r:UctReplay,event:UctEvent):number {
  const s=r.simulations[event.simulation],n=s.inference?.codes.length??s.rollout.length;
  if(event.phase==='rollout')return event.move!+1;
  if(event.phase==='select'){const step=s.choices[event.choice]?.step;return step===undefined?0:s.rollout.filter(m=>m[1]<step).length;}
  return n;
}

// Totals over the viewed timeline prefix only.
export function tallyAt(index:InferenceIndex,r:UctReplay,event:UctEvent):Tally {
  const t={...index.starts[event.simulation]},codes=r.simulations[event.simulation].inference!.codes,limit=prefixMoves(r,event),bytes=index.bytes[event.simulation];
  for(let move=0;move<limit;move++)add(t,codes[move] as InferenceCode,bytes[move]>=0?bytes[move]:null);
  return t;
}

export function moveInference(index:InferenceIndex|null,r:UctReplay,simulation:number,move:number):MoveInference|null {
  const code=r.simulations[simulation].inference?.codes[move] as InferenceCode|undefined;
  if(!index||!code)return null;
  const group=index.groups[simulation][move],call=index.calls[simulation][move],bytes=index.bytes[simulation][move];
  const list=group>=0?index.uses.get(group)!:null,use=list?list.findIndex(u=>u.simulation===simulation&&u.move===move)+1:null;
  return {code,label:inferenceLabels[code],call:call>=0?call+1:null,group:group>=0?group+1:null,use,first:list?list[0]:null,bytes:bytes>=0?bytes:null};
}

export const mib=(bytes:number)=>bytes<1048576?`${(bytes/1024).toFixed(1)} KiB`:`${(bytes/1048576).toFixed(1)} MiB`;
