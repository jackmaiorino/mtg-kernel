import { ArrowRight, GitBranch, Navigation } from 'lucide-react';
import type { Frame, Recording } from './recording';
import './search-lifecycle.css';

const reasons={
  initial:'The scheduled first search begins here.',
  unsupported:'The previous tree reached this position, but no move had the eight completed rollouts required to follow it.',
  miss:'The actual game reached a history missing from the previous tree.',
  depth_limit:'The actual game reached the depth limit of the previous tree.',
  supported:'This position has a supported move in the existing tree. No new simulations ran for this decision.',
  before_search:'The frozen policy plays the opening before the scheduled search.',
};

export function SearchLifecycle({recording,frame,after,onDecision}:{recording:Recording;frame:Frame;after:boolean;onDecision:(id:string)=>void}){
  if(!recording.executionBinding)return null;
  const choices=recording.frames.filter(f=>f.searchLifecycle&&f.searchLifecycle.reason!=='before_search');
  const context=frame.searchLifecycle?frame:choices.filter(f=>f.key.episode===frame.key.episode&&f.key.step<=frame.key.step).at(-1);
  const lifecycle=context?.searchLifecycle;
  const exact=Boolean(frame.searchLifecycle);
  const title=!lifecycle?'Policy opening':!exact?'Between guide choices':lifecycle.mode==='new-search'?'Starts a new search':lifecycle.mode==='reuse'?'Follows the existing tree':'Policy choice';
  const roots=choices.filter(f=>f.searchLifecycle!.mode==='new-search');
  const ordinal=roots.findIndex(f=>f.id===lifecycle?.root)+1;
  return <section className="search-lifecycle" aria-label="Actual game search lifecycle">
    <div className="lifecycle-status"><span className={`lifecycle-icon ${exact?lifecycle?.mode:'between'}`}><GitBranch size={22}/></span><div><span className="eyebrow">ACTUAL GAME · SEARCH CONTROL</span><h2>{title}</h2><p>{exact&&lifecycle?reasons[lifecycle.reason]:lifecycle?`${frame.key.actor===recording.perspective?'A forced move or resolution':'The opponent moves'} between stored guide choices. Search ${ordinal} remains the current tree; this step has no separate tree node.`:reasons.before_search}</p></div></div>
    {lifecycle?.root&&<div className="lifecycle-location"><Navigation size={16}/><span>Search {ordinal} · rooted at decision {Number(lifecycle.root.slice(1))+1}</span><strong>{exact&&lifecycle.node!==null?`${after?'Executed choice at':'Actual choice at'} node ${lifecycle.node}`:`Last guide choice: node ${lifecycle.node??'unavailable'}`}</strong>{lifecycle.backups!==null&&<span>{lifecycle.backups} completed rollouts on this move</span>}{lifecycle.remainingAfter!==null&&<span>{lifecycle.remainingAfter.toLocaleString()} transitions remain</span>}</div>}
    {exact&&lifecycle?.previous&&<p className="lifecycle-refresh"><strong>Why search again?</strong> {reasons[lifecycle.previous.reason]} {lifecycle.previous.node!==null&&`Previous tree: node ${lifecycle.previous.node}.`}</p>}
    <div className="lifecycle-timeline" aria-label="Recorded guide choices">{choices.map(f=>{
      const e=f.searchLifecycle!,search=roots.findIndex(r=>r.id===e.root)+1;
      return <button key={f.id} className={`${e.mode} ${context?.id===f.id?'current':''}`} aria-current={context?.id===f.id?'step':undefined} onClick={()=>onDecision(f.id)}><span>Decision {f.key.step+1}</span><strong>{e.mode==='new-search'?`New search ${search}`:e.mode==='reuse'?`Follow tree ${search}`:'Policy'}</strong><small>{e.node===null?'No matched node':`Node ${e.node}`}</small><ArrowRight size={12}/></button>;
    })}</div>
    <p className="lifecycle-footnote">Recorded execution, linked by native history keys and original action IDs. The green search route below is a simulated future; the blue marker locates the actual guide choice.</p>
  </section>;
}
