import type { UctReplay } from './recording';
import type { UctEvent } from './uct-playback';

export type AttemptSummary={start:number;events:number;reason:UctReplay['simulations'][number]['reason'];win:boolean;combo:boolean;wins:number};
export const comboSteps=['Spy targets its own library','Library milled into the graveyard','Dread Return targets Lotleth Giant','Lotleth Giant returns to play'];

// Presentation summary of each attempt: where it starts on the event
// timeline, its natural result and the running count of recorded wins.
export function attemptSummaries(r:UctReplay,events:UctEvent[]):AttemptSummary[] {
  const result=r.simulations.map(s=>({start:-1,events:0,reason:s.reason,win:s.reason==='win',combo:s.combo&&s.reason==='win',wins:0}));
  events.forEach((e,i)=>{const a=result[e.simulation];if(a.start<0)a.start=i;a.events++;});
  let wins=0;for(const a of result){wins+=Number(a.win);a.wins=wins;}
  return result;
}

// Ordered combo stages observed so far in this attempt, using the same
// ordering rule as the recording validator: each stage needs the previous one.
export function comboProgress(r:UctReplay,event:UctEvent,seat:number):number {
  const s=r.simulations[event.simulation],choice=s.choices[event.choice];
  const seen=event.phase==='evaluate'||event.phase==='backup'?s.rollout:event.phase==='rollout'?s.rollout.slice(0,event.move!+(event.after?1:0)):s.rollout.filter(m=>choice&&m[1]<choice.step);
  let stage=0;
  for(const m of seen){
    if(stage===0&&m[0]===seat&&(m[5]&2))stage=1;
    if(stage===1&&(m[6]&2))stage=2;
    if(stage===2&&m[0]===seat&&(m[5]&4))stage=3;
    if(stage===3&&(m[6]&4))stage=4;
  }
  return stage;
}

// Lead with what a mana ability produces, so truncated labels stay distinct.
export const shortLabel=(s:string)=>s.replace(/\s*\[c\d+\]/g,'').replace(/^Begin casting /,'Cast ').replace(/^activate mana ability: /i,'Tap ').replace(/ as your land for the turn$/,'')
  .replace(/^(\w+) (.+): add (\d+) \{(\w)\}$/,(_,verb,source,n,c)=>`${verb} for ${`{${c}}`.repeat(Number(n))}: ${source}`);
