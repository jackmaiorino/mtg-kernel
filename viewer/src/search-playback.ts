import type { SearchReplay } from './recording';
export type SearchEvent = { simulation: number; phase: 'select' | 'evaluate' | 'backup'; choice: number };
export function searchEvents(replay: SearchReplay): SearchEvent[] {
  return replay.simulations.flatMap((s,simulation) => [...s.choices.map((_,choice) => ({simulation,phase:'select' as const,choice})),
    {simulation,phase:'evaluate' as const,choice:s.choices.length-1}, {simulation,phase:'backup' as const,choice:s.choices.length-1}]);
}
export type SearchPlayback = { event: number; playing: boolean; speed: number; epoch: number };
export const initialSearchPlayback: SearchPlayback = { event:0,playing:false,speed:1,epoch:0 };
export type SearchAction = { type:'seek'; event:number } | {type:'toggle'} | {type:'speed'; speed:number} | {type:'advance'; length:number};
export function searchReducer(s:SearchPlayback,a:SearchAction):SearchPlayback {
  if(a.type==='seek')return {...s,event:a.event,playing:false,epoch:s.epoch+1};
  if(a.type==='toggle')return {...s,playing:!s.playing,epoch:s.epoch+1};
  if(a.type==='speed')return {...s,speed:a.speed,epoch:s.epoch+1};
  return s.event>=a.length-1?{...s,playing:false}:{...s,event:s.event+1};
}
export function rootHistory(replay:SearchReplay) {
  const history=[{visits:replay.nodes[0].actions.map(()=>0),sums:replay.nodes[0].actions.map(()=>0n)}];
  for(const s of replay.simulations){const prev=history.at(-1)!,visits=[...prev.visits],sums=[...prev.sums];const a=s.choices[0].action;visits[a]++;sums[a]+=BigInt(s.value);history.push({visits,sums});}
  return history;
}
